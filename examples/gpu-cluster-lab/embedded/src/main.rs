use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use drasi_host_sdk::{
    computation::{NativeFactory, NativePlugin},
    PluginRegistry,
};
use drasi_lib::{computation::v1::*, ComponentStatus, DrasiLib};
use drasi_server::instance_registry::InstanceRegistry;
use futures_util::StreamExt;
use gpu_native::{
    inputs::{
        self, BootstrapBoundary, QueryBootstrapRow, QueryBootstrapWatermark, DATABASE_QUERIES,
    },
    lifecycle::RUNTIME_OBSERVATION,
    projections, RuntimeComponent, RuntimeObservation,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{watch, Mutex, RwLock};
use uuid::Uuid;

mod database;
mod diagnostics;

const INSTANCE: &str = "gpu-demo";
const NATIVE: [(&str, &str); 6] = [
    ("policy", "gpu.lab/regorus-policy"),
    ("simulator", "gpu.lab/telemetry-simulator"),
    ("placement", "gpu.lab/placement-solver"),
    ("resilience", "gpu.lab/resilience-assessor"),
    ("plan-writer", "gpu.lab/plan-writer"),
    ("runtime-status", "gpu.lab/runtime-status"),
];

fn env(name: &str) -> Result<String> {
    let value = std::env::var(name).with_context(|| format!("{name} is required"))?;
    ensure!(!value.trim().is_empty(), "{name} is empty");
    Ok(value)
}
fn id(value: &str) -> Result<ComponentId> {
    Ok(ComponentId::try_new(value)?)
}
fn endpoint(component: &str, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(id(component)?, PortId::try_new(port)?))
}
fn edge(from: &str, to: &str) -> Result<EdgeDefinition> {
    Ok(EdgeDefinition::new(
        endpoint(from, "out")?,
        endpoint(to, "in")?,
    ))
}
fn relationship(from: &str, to: &str, capacity: usize) -> Result<DesiredRelationship> {
    Ok(DesiredRelationship {
        definition: edge(from, to)?,
        policy: RelationshipPolicy::default(),
        pipe: DesiredPipe::Bounded { capacity },
    })
}

struct Secrets(String);
#[async_trait]
impl ConfigurationResolver for Secrets {
    fn validate_reference(&self, key: &str) -> Result<()> {
        ensure!(key == "internal-token", "unknown runtime secret");
        Ok(())
    }
    async fn resolve(&self, key: &str) -> Result<Value> {
        self.validate_reference(key)?;
        Ok(Value::String(self.0.clone()))
    }
}

struct Active {
    core: Arc<DrasiLib>,
    stop: watch::Sender<bool>,
    observer: tokio::task::JoinHandle<Result<()>>,
}
struct Runtime {
    registry: InstanceRegistry,
    plugins: Arc<RwLock<PluginRegistry>>,
    native: Arc<NativePlugin>,
    network: Arc<NativePlugin>,
    active: Mutex<Option<Active>>,
    view: RwLock<Value>,
    sequence: AtomicU64,
    token: String,
    host: String,
    password: String,
    plan_endpoint: String,
}

impl Runtime {
    fn factory(&self, name: &str) -> Result<Arc<NativeFactory>> {
        self.native
            .factories()
            .iter()
            .find(|factory| factory.metadata().implementation.name.as_ref() == name)
            .cloned()
            .with_context(|| format!("native factory {name} is missing"))
    }

    async fn construct(&self) -> Result<Arc<DrasiLib>> {
        let builder = DrasiLib::builder()
            .with_id(INSTANCE)
            .with_component_factories(self.plugins.read().await.computation_factory_registry()?);
        let core = Arc::new(builder.build().await?);
        if let Err(error) = self.wire(&core).await {
            core.shutdown()
                .await
                .context("failed to clean up a rejected runtime graph")?;
            return Err(error);
        }
        Ok(core)
    }

    async fn wire(&self, core: &DrasiLib) -> Result<()> {
        let diagnostics_enabled = diagnostics::enabled()?;
        let mut middleware = drasi_core::middleware::MiddlewareTypeRegistry::new();
        middleware.register(Arc::new(gpu_native::postgres::PostgresJsonFactory));
        let catalog = QueryResultsCatalog::new("__drasi_lib_runtime__")?;
        let queries = queries(catalog.clone(), diagnostics_enabled)?.auto_start(false);
        let mut start = queries
            .definition
            .components
            .iter()
            .map(|component| component.descriptor.id().clone())
            .collect::<Vec<_>>();
        core.add_components(queries).await?;
        let database = database::components(
            &self.host,
            &self.password,
            &env("GPU_STATE_DIR")?,
            catalog,
            Arc::new(middleware),
        )?;
        start.extend(
            database
                .definition
                .components
                .iter()
                .map(|node| node.descriptor.id().clone()),
        );
        core.add_components(database.auto_start(false)).await?;

        let secret = ResourceId::try_new("gpu-runtime-secrets")?;
        let mut components = ComponentBatch::builder()
            .declare_resource(ResourceSpecification {
                id: secret.clone(),
                role: ResourceRole::SecretStore,
                ownership: ResourceOwnership::Borrowed,
                binding: "gpu-runtime-environment".into(),
            })?
            .provide_resource(
                secret.clone(),
                ResourceHandle::new(
                    ResourceRole::SecretStore,
                    Arc::new(ConfigurationResolverResource(Arc::new(Secrets(
                        self.token.clone(),
                    )))),
                ),
            )?;
        for (component, implementation) in NATIVE {
            let factory = self.factory(implementation)?;
            let configuration = if component == "plan-writer" {
                json!({"endpoint":self.plan_endpoint, "token":""})
            } else {
                json!({"stream":format!("{component}/out")})
            };
            let mut spec = factory.specification(id(component)?, configuration)?;
            if component == "plan-writer" {
                spec.configuration.insert(
                    "token".into(),
                    ConfigurationValue::Reference {
                        resource: secret.clone(),
                        key: "internal-token".into(),
                        secret: true,
                    },
                );
            }
            components = components.component(spec, factory);
            if component != "plan-writer" {
                components = components.bind_stream(
                    endpoint(component, "out")?,
                    StreamId::try_new(format!("{component}/out"))?,
                );
            }
        }
        let sse = self
            .network
            .factories()
            .iter()
            .find(|factory| {
                factory.metadata().implementation.name.as_ref() == "drasi.network/sse-sink"
            })
            .context("native SSE factory is missing")?
            .clone();
        components = components.component(sse.specification(id("gpu-demo-ui")?, json!({
            "queryStreams":gpu_contracts::UI_QUERIES.iter().map(|id| (*id,format!("{id}/out"))).collect::<BTreeMap<_,_>>(),
            "host":"0.0.0.0","port":8081,"ssePath":"/events",
        }))?, sse);
        let mut relationships = gpu_contracts::UI_QUERIES
            .iter()
            .map(|query| relationship(query, "gpu-demo-ui", 64))
            .collect::<Result<Vec<_>>>()?;
        let logged = diagnostics::configured_log_queries()?;
        if !logged.is_empty() {
            components = components.sink(Box::new(diagnostics::QueryLog::new()?));
            for query in logged {
                relationships.push(relationship(&query, "gpu-query-log", 64)?);
            }
        }
        if diagnostics_enabled {
            components = components.sink(Box::new(diagnostics::Recorder::new()?));
            for producer in [
                "policy",
                "simulator",
                "placement",
                "resilience",
                "runtime-status",
            ] {
                for query in diagnostics::QUERIES {
                    relationships.push(relationship(producer, query, 64)?);
                }
                relationships.push(DesiredRelationship {
                    definition: EdgeDefinition::new(
                        endpoint(producer, "out")?,
                        endpoint(diagnostics::SINK, "graph")?,
                    ),
                    policy: RelationshipPolicy::default(),
                    pipe: DesiredPipe::Bounded { capacity: 64 },
                });
            }
            for producer in DATABASE_QUERIES
                .iter()
                .chain(gpu_contracts::UI_QUERIES.iter())
                .chain(diagnostics::QUERIES.iter())
            {
                relationships.push(DesiredRelationship {
                    definition: EdgeDefinition::new(
                        endpoint(producer, "out")?,
                        endpoint(diagnostics::SINK, "query")?,
                    ),
                    policy: RelationshipPolicy::default(),
                    pipe: DesiredPipe::Bounded { capacity: 64 },
                });
            }
        }
        for query in DATABASE_QUERIES {
            relationships.push(relationship(query, "policy", 32)?);
        }
        for (from, to) in [
            ("policy", "simulation-inputs"),
            ("policy", "scheduling-inputs"),
            ("simulation-inputs", "simulator"),
            ("simulator", "scheduling-inputs"),
            ("scheduling-inputs", "placement"),
            ("scheduling-inputs", "resilience"),
            ("placement", "plan-output"),
            ("plan-output", "plan-writer"),
            ("placement", "runtime-context"),
        ] {
            relationships.push(relationship(from, to, 32)?);
        }
        for producer in [
            "policy",
            "simulator",
            "placement",
            "resilience",
            "runtime-status",
        ] {
            for query in gpu_contracts::UI_QUERIES {
                relationships.push(relationship(producer, query, 64)?);
            }
        }
        let mut components = components.build()?;
        components.definition.relationships.extend(relationships);
        start.extend(
            components
                .definition
                .components
                .iter()
                .map(|component| component.descriptor.id().clone()),
        );
        core.add_components(components.auto_start(false)).await?;
        let control = core.computation_control()?;
        control
            .set_control_connections(vec![(id(inputs::DATABASE_QUERY)?, id("runtime-status")?)])
            .await?;
        let report = control
            .start_requested(
                control.desired_snapshot().revision,
                GraphSelection::Exact(start),
            )
            .await?;
        if report.summary != OperationSummary::Completed {
            let incomplete = report
                .components
                .iter()
                .filter(|(_, outcome)| {
                    !matches!(
                        outcome,
                        StartOutcome::Started | StartOutcome::AlreadyRunning
                    )
                })
                .collect::<Vec<_>>();
            anyhow::bail!("graph startup was incomplete: {incomplete:?}");
        }
        Ok(())
    }

    async fn start(
        self: &Arc<Self>,
        scenario: Option<String>,
        expected_plan: Option<Uuid>,
    ) -> Result<()> {
        ensure!(
            scenario
                .as_ref()
                .is_none_or(|name| gpu_contracts::fixtures::NAMES.contains(&name.as_str())),
            "unknown scenario"
        );
        let mut active = self.active.lock().await;
        ensure!(active.is_none(), "runtime is already started");
        *self.view.write().await = json!({"ready":false,"state":"starting","scenario":scenario});
        let core = self.construct().await?;
        self.registry
            .add(INSTANCE.into(), core.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let epoch = Uuid::new_v4();
        let (stop, stopped) = watch::channel(false);
        let runtime = self.clone();
        let owned = core.clone();
        let observer = tokio::spawn(async move {
            let result = runtime
                .observe(owned, epoch, scenario, expected_plan, stopped)
                .await;
            if let Err(error) = &result {
                tracing::error!("GPU runtime failed: {error:#}");
                *runtime.view.write().await =
                    json!({"ready":false,"state":"error","detail":format!("{error:#}")});
            }
            result
        });
        *active = Some(Active {
            core,
            stop,
            observer,
        });
        Ok(())
    }

    async fn observe(
        &self,
        core: Arc<DrasiLib>,
        epoch: Uuid,
        requested_scenario: Option<String>,
        expected_plan: Option<Uuid>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        core.start().await?;
        let mut queries = BTreeMap::new();
        let mut scenario = None;
        for query in DATABASE_QUERIES {
            tokio::time::timeout(
                Duration::from_secs(120),
                core.computation_component(query)?.wait_started(),
            )
            .await??;
            let reader = core
                .query_manager()
                .get_query_instance(query)
                .await
                .map_err(anyhow::Error::msg)?;
            let snapshot = tokio::time::timeout(Duration::from_secs(120), async {
                loop {
                    let current = database::current_snapshot(&self.host, &self.password, epoch).await?;
                    let snapshot = reader.fetch_snapshot().await?;
                    let rows = snapshot.to_vec();
                    ensure!(rows.len() == 1, "transactional configuration query must have one row");
                    let records = rows[0]["records"].as_array().context("configuration records missing")?;
                    let tables = inputs::partition_records(records)?;
                    let plans = &tables["input-plan"];
                    ensure!(plans.len() == 1, "configuration requires exactly one fleet plan");
                    let observed = inputs::DatabaseSnapshot::from_records(epoch, records)?;
                    if serde_json::to_value(&current)? == serde_json::to_value(&observed)?
                        && expected_plan.is_none_or(|expected| plans[0]["decision_id"] == expected.to_string()) {
                        break Ok::<_, anyhow::Error>(snapshot);
                    }
                    tokio::select! {
                        changed = stop.changed() => { changed?; ensure!(!*stop.borrow(), "GPU bootstrap cancelled"); }
                        _ = tokio::time::sleep(Duration::from_millis(25)) => {}
                    }
                }
            }).await.context("configuration query did not catch up to the current database snapshot")??;
            {
                let rows = snapshot.to_vec();
                ensure!(
                    rows.len() == 1,
                    "starting scenario requires exactly one plan query row"
                );
                let records = rows[0]["records"]
                    .as_array()
                    .context("plan query records missing")?;
                let tables = inputs::partition_records(records)?;
                let records = &tables["input-plan"];
                scenario = Some(
                    records[0]["decision_details"]["scenario"]
                        .as_str()
                        .context("saved plan starting scenario is missing")?
                        .to_owned(),
                );
            }
            let mut rows = Vec::new();
            let mut keyed = snapshot.clone().stream_keyed();
            while let Some((signature, value)) = keyed.next().await {
                ensure!(
                    value.as_object().is_some_and(|row| row.len() == 1),
                    "unexpected database query snapshot columns"
                );
                rows.push(QueryBootstrapRow {
                    signature,
                    records: serde_json::from_value(value["records"].clone())?,
                });
            }
            queries.insert(
                query.into(),
                QueryBootstrapWatermark {
                    sequence: snapshot.as_of_sequence,
                    row_count: snapshot.to_vec().len(),
                    rows: Some(rows),
                },
            );
        }
        let scenario = scenario.context("starting scenario was not observed")?;
        for query in gpu_contracts::UI_QUERIES.into_iter().chain([
            "simulation-inputs",
            "scheduling-inputs",
            "plan-output",
            "runtime-context",
        ]) {
            tokio::time::timeout(
                Duration::from_secs(120),
                core.computation_component(query)?.wait_started(),
            )
            .await??;
            core.query_manager()
                .get_query_instance(query)
                .await
                .map_err(anyhow::Error::msg)?
                .fetch_snapshot()
                .await?;
        }
        ensure!(
            gpu_contracts::fixtures::NAMES.contains(&scenario.as_str()),
            "saved starting scenario is invalid"
        );
        ensure!(
            requested_scenario
                .as_ref()
                .is_none_or(|expected| expected == &scenario),
            "saved and requested starting scenarios differ"
        );
        let control = core
            .computation_component(inputs::DATABASE_QUERY)?
            .control()?;
        let notifications =
            gpu_native::bootstrap::notifications(&BootstrapBoundary { epoch, queries })?;
        let max_wire_bytes = notifications
            .iter()
            .map(|(kind, payload)| gpu_native::bootstrap::notification_bytes(kind, payload))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .max()
            .context("GPU bootstrap produced no notifications")?;
        tracing::info!(
            messages = notifications.len(),
            max_wire_bytes,
            "GPU database bootstrap transport prepared"
        );
        let target = id("policy")?;
        tokio::time::timeout(Duration::from_secs(10), async {
            for (kind, payload) in notifications {
                loop {
                    ensure!(!*stop.borrow(), "GPU bootstrap cancelled");
                    match control.notify_neighbor(&target, ControlNotification::Custom {
                        kind: kind.clone(), payload: payload.clone(),
                    }) {
                        Ok(()) => break,
                        Err(ControlError::QueueFull { .. }) => tokio::select! {
                            changed = stop.changed() => { changed?; anyhow::bail!("GPU bootstrap cancelled"); }
                            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                        },
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        }).await.context("GPU bootstrap delivery timed out")??;
        let required: BTreeSet<String> = core
            .computation_control()?
            .observed()
            .components
            .keys()
            .map(ToString::to_string)
            .collect();
        let mut previous = None;
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                changed = stop.changed() => { changed?; if *stop.borrow() { break; } }
                _ = interval.tick() => {
                    let observed = core.computation_control()?.observed();
                    let components = observed.components.iter().map(|(id, value)| RuntimeComponent {
                        component_id: id.to_string(), status: format!("{:?}", value.lifecycle).to_lowercase(),
                        error: value.failure.as_ref().map(|failure| failure.cause.to_string()),
                    }).collect();
                    let source_ready = core.get_source_status("postgres").await? == ComponentStatus::Running;
                    let context = core.get_query_results("runtime-context").await?;
                    ensure!(context.len() <= 1, "multiple scheduling contexts");
                    let current = context.first().filter(|row| row["current"] == true);
                    let mut observation = RuntimeObservation {
                        sequence: 1, observation_epoch: epoch, scenario: scenario.clone(),
                        scheduling_signature: current.and_then(|row| row["scheduling_signature"].as_str()).unwrap_or("").into(),
                        policy_signature: current.and_then(|row| row["policy_signature"].as_str()).unwrap_or("").into(),
                        source_bootstrap_complete: source_ready,
                        query_bootstrap_complete: true,
                        query_results_current: false,
                        reset_in_progress: false,
                        detail: if source_ready { "Live graph and query bootstrap observed" } else { "PostgreSQL source is unavailable" }.into(),
                        required_components: required.clone(), components,
                    };
                    if let Some(context) = current {
                        let mut snapshots = BTreeMap::new();
                        for query in ["ui-placements", "ui-policy", "ui-resilience", "ui-decisions", "ui-workloads"] {
                            snapshots.insert(query, core.get_query_results(query).await?);
                        }
                        let replicas = context["required_replicas"].as_u64().context("missing required replica count")?;
                        observation.query_results_current = observation.projections_current(replicas, &snapshots)?;
                    }
                    if source_ready && !observation.query_results_current {
                        observation.detail = "Waiting for current policy, placement, enforcement and resilience query results".into();
                    }
                    if previous.as_ref() != Some(&observation) {
                        previous = Some(observation.clone());
                        observation.sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
                        control.notify_neighbor(&id("runtime-status")?, ControlNotification::Custom {
                            kind: RUNTIME_OBSERVATION.into(), payload: json!(observation),
                        })?;
                    }
                    let status = core.get_query_results("ui-status").await?;
                    ensure!(status.len() <= 1, "multiple readiness records");
                    *self.view.write().await = json!({
                        "ready": status.first().is_some_and(|row| row["scenario_ready"] == true),
                        "state": "running", "scenario":scenario, "observation_epoch":epoch,
                        "status":status.first(), "transaction_completion":"not-supported",
                    });
                }
            }
        }
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut active = self.active.lock().await;
        let Some(current) = active.as_mut() else {
            return Ok(());
        };
        *self.view.write().await = json!({"ready":false,"state":"stopping"});
        current.stop.send_replace(true);
        core_shutdown(&current.core).await?;
        if !current.observer.is_finished() {
            current.observer.abort();
        }
        let current = active.take().context("active runtime disappeared")?;
        match current.observer.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!("Stopped failed runtime observer: {error:#}"),
            Err(error) if error.is_cancelled() => {}
            Err(error) => return Err(error.into()),
        }
        self.registry.remove(INSTANCE).await;
        *self.view.write().await = json!({"ready":false,"state":"stopped"});
        Ok(())
    }
}

async fn core_shutdown(core: &DrasiLib) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(60), core.shutdown()).await??;
    Ok(())
}

fn queries(catalog: QueryResultsCatalog, diagnostic: bool) -> Result<ComponentBatch> {
    let indexes = ResourceId::try_new("gpu-memory-indexes")?;
    let catalog_id = ResourceId::try_new("gpu-query-catalog")?;
    let mut batch = ComponentBatch::builder();
    for (id, role, handle) in [
        (
            indexes.clone(),
            ResourceRole::IndexBackend,
            ResourceHandle::new(
                ResourceRole::IndexBackend,
                Arc::new(QueryIndexProviderResource(Arc::new(
                    drasi_core::computation::InMemoryComputationProvider,
                ))),
            ),
        ),
        (
            catalog_id.clone(),
            ResourceRole::QueryCatalog,
            ResourceHandle::new(ResourceRole::QueryCatalog, Arc::new(catalog.clone())),
        ),
    ] {
        batch = batch
            .declare_resource(ResourceSpecification {
                id: id.clone(),
                role,
                ownership: ResourceOwnership::Graph,
                binding: id.as_str().into(),
            })?
            .provide_resource(id, handle)?;
    }
    let mut definitions = projections::definitions()
        .into_iter()
        .chain(inputs::processing_queries())
        .map(|(id, text, settings)| (id, text.to_owned(), settings))
        .collect::<Vec<_>>();
    if diagnostic {
        definitions.extend(diagnostics::definitions()?);
    }
    let factory = FactoryRegistry::standard()
        .get(
            &ContinuousQueryFactory::default()
                .descriptor()
                .implementation,
        )
        .context("built-in continuous query factory is missing")?
        .clone();
    let mut relationships = Vec::new();
    for (name, text, settings) in definitions {
        let definition = ContinuousQueryDefinition {
            graph_id: catalog.graph_id().into(),
            id: id(name)?,
            query: text.clone(),
            language: ComputationQueryLanguage::Cypher,
            output_stream: StreamId::try_new(format!("{name}/out"))?,
            outbox_capacity: NonZeroUsize::new(128).unwrap(),
        };
        batch = batch
            .component(
                ComponentSpecification {
                    descriptor: definition.descriptor_with_execution(&settings),
                    role: ComponentRole::Query,
                    completion: None,
                    implementation: factory.descriptor().implementation.clone(),
                    configuration_version: 1,
                    configuration: BTreeMap::from([
                        ("query".into(), ConfigurationValue::Literal(json!(text))),
                        (
                            "stream".into(),
                            ConfigurationValue::Literal(json!(definition.output_stream)),
                        ),
                        (
                            "outbox_capacity".into(),
                            ConfigurationValue::Literal(json!(128)),
                        ),
                        (
                            "execution".into(),
                            ConfigurationValue::Literal(serde_json::to_value(settings)?),
                        ),
                    ]),
                    dependencies: BTreeMap::from([
                        ("indexes".into(), vec![indexes.clone()]),
                        ("catalog".into(), vec![catalog_id.clone()]),
                    ]),
                },
                factory.clone(),
            )
            .bind_stream(endpoint(name, "out")?, definition.output_stream);
        relationships.push(relationship(name, "query-results", 64)?);
    }
    let mut batch = batch
        .sink(Box::new(QueryResultsOutlet::new(
            id("query-results")?,
            catalog,
        )))
        .build()?;
    batch.definition.relationships.extend(relationships);
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn downstream_queries_are_native_without_legacy_subscriptions() -> Result<()> {
        let core = DrasiLib::builder().with_id(INSTANCE).build().await?;
        let catalog = QueryResultsCatalog::new("__drasi_lib_runtime__")?;
        let batch = queries(catalog.clone(), false)?;
        assert_eq!(batch.definition.components.len(), 14);
        assert!(batch
            .definition
            .components
            .iter()
            .all(|component| matches!(component.role, ComponentRole::Query | ComponentRole::Sink)));
        core.add_components(batch.auto_start(false)).await?;
        let directory = tempfile::tempdir()?;
        let mut middleware = drasi_core::middleware::MiddlewareTypeRegistry::new();
        middleware.register(Arc::new(gpu_native::postgres::PostgresJsonFactory));
        core.add_components(
            database::components(
                "127.0.0.1",
                "test-only",
                directory.path().to_str().context("test path")?,
                catalog,
                Arc::new(middleware),
            )?
            .auto_start(false),
        )
        .await?;
        core.shutdown().await?;
        Ok(())
    }
}

fn authorized(runtime: &Runtime, headers: &HeaderMap) -> Result<()> {
    ensure!(
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            == Some(format!("Bearer {}", runtime.token).as_str()),
        "runtime authorization failed"
    );
    Ok(())
}
fn failure(error: anyhow::Error) -> Response {
    tracing::error!("Runtime request failed: {error:#}");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error":format!("{error:#}")})),
    )
        .into_response()
}
async fn health(State(runtime): State<Arc<Runtime>>) -> Response {
    let view = runtime.view.read().await.clone();
    let code = if view["ready"] == true {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(view)).into_response()
}

async fn diagnostic_metrics(State(runtime): State<Arc<Runtime>>, headers: HeaderMap) -> Response {
    let result: Result<Value> = async {
        ensure!(diagnostics::enabled()?, "diagnostics are disabled");
        authorized(&runtime, &headers)?;
        let active = runtime.active.lock().await;
        let core = &active.as_ref().context("runtime is stopped")?.core;
        let mut queries = BTreeMap::new();
        for query in gpu_contracts::UI_QUERIES.into_iter().chain(diagnostics::QUERIES) {
            let m = core.get_query_output_metrics(query).await?;
            queries.insert(query, json!({
                "outbox_size":m.outbox_size,
                "outbox_earliest_seq":m.outbox_earliest_seq.to_string(),
                "outbox_latest_seq":m.outbox_latest_seq.to_string(),
                "result_seq_advances":m.result_seq_advances.to_string(),
                "live_results_count":m.live_results_count,
                "outer_transaction_duration_ns_last":m.outer_transaction_duration_ns_last.to_string(),
                "outer_transaction_duration_ns_max":m.outer_transaction_duration_ns_max.to_string(),
                "snapshot_fetch_count":m.snapshot_fetch_count.to_string(),
            }));
        }
        Ok(json!({"observed_at_ms":chrono::Utc::now().timestamp_millis(),"queries":queries,
            "scope":"Native query output metrics; outbox size is not input queue depth."}))
    }.await;
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => failure(error),
    }
}
async fn stop(State(runtime): State<Arc<Runtime>>, headers: HeaderMap) -> Response {
    if let Err(error) = authorized(&runtime, &headers) {
        return (StatusCode::UNAUTHORIZED, error.to_string()).into_response();
    }
    match runtime.stop().await {
        Ok(()) => Json(json!({"state":"stopped"})).into_response(),
        Err(error) => failure(error),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    scenario: String,
    #[serde(default)]
    expected_plan: Option<Uuid>,
}
async fn start(
    State(runtime): State<Arc<Runtime>>,
    headers: HeaderMap,
    Json(input): Json<Start>,
) -> Response {
    if let Err(error) = authorized(&runtime, &headers) {
        return (StatusCode::UNAUTHORIZED, error.to_string()).into_response();
    }
    match runtime
        .start(Some(input.scenario), input.expected_plan)
        .await
    {
        Ok(()) => Json(json!({"state":"starting"})).into_response(),
        Err(error) => failure(error),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    ensure!(
        drasi_server::ui_assets::has_embedded_ui(),
        "Drasi Server UI assets are missing. Build drasi-server/ui before compiling gpu-runtime, or use ./demo build drasi."
    );
    let native = drasi_host_sdk::computation::load(env("GPU_NATIVE_PLUGIN")?)?;
    let network = drasi_host_sdk::computation::load(env("GPU_NETWORK_PLUGIN")?)?;
    let mut plugins = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugins);
    plugins.register_computation_plugin(native.clone())?;
    plugins.register_computation_plugin(network.clone())?;
    let runtime = Arc::new(Runtime {
        registry: InstanceRegistry::new(),
        plugins: Arc::new(RwLock::new(plugins)),
        native,
        network,
        active: Mutex::new(None),
        view: RwLock::new(json!({"ready":false,"state":"starting"})),
        sequence: AtomicU64::new(0),
        token: env("INTERNAL_TOKEN")?,
        host: env("PGHOST")?,
        password: env("REPLICATION_PASSWORD")?,
        plan_endpoint: env("PLAN_ENDPOINT")?,
    });
    let api = drasi_server::api::v1::routes::build_v1_router(
        runtime.registry.clone(),
        Arc::new(true),
        None,
        runtime.plugins.clone(),
        None,
    );
    let mut app = Router::new()
        .route("/health/ready", get(health))
        .route("/internal/runtime/stop", post(stop))
        .route("/internal/runtime/start", post(start));
    if diagnostics::enabled()? {
        app = app.route("/internal/diagnostics/metrics", get(diagnostic_metrics));
    }
    let app = app
        .with_state(runtime.clone())
        .nest("/api/v1", api)
        .merge(drasi_server::ui_assets::embedded_ui_routes())
        .route(
            "/",
            get(|| async { axum::response::Redirect::temporary("/ui/?instance=gpu-demo") }),
        );
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("Drasi Server admin UI enabled at /ui/?instance=gpu-demo");
    runtime.start(None, None).await?;
    let stopping = runtime.clone();
    let served = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if let Err(error) = gpu_control::shutdown_signal().await {
                tracing::error!("Shutdown signal failed: {error:#}");
            }
            if let Err(error) = stopping.stop().await {
                tracing::error!("Runtime shutdown failed: {error:#}");
            }
        })
        .await;
    runtime.stop().await?;
    served?;
    Ok(())
}
