// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::*;
use crate::{
    config::telemetry::TelemetryConfig, managed_configuration::ConfigurationStoreConfig,
    telemetry::StandaloneTelemetry, DrasiServer,
};
use anyhow::Result;
use drasi_lib::{
    computation::v1::*,
    observability::{MetricKind, MetricUnit, PluginMetricDefinition, TelemetryScope},
    Query,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use opentelemetry_proto::tonic::{
    collector::metrics::v1::ExportMetricsServiceRequest,
    common::v1::any_value::Value,
    metrics::v1::{metric::Data, number_data_point::Value as Number},
};
use prost::Message;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Mutex,
};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

const CASE: &str = "DRASI_TASK4_FILE_METRICS_CASE";
const FLAVOR: &str = "DRASI_TASK4_FILE_METRICS_FLAVOR";

fn children(case: &str) -> Result<()> {
    for flavor in ["current_thread", "multi_thread"] {
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "server::lifecycle_tests::metrics::file_metrics_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CASE, case)
            .env(FLAVOR, flavor)
            .env("RUST_LOG", "info")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while child.try_wait()?.is_none() {
            if std::time::Instant::now() >= deadline {
                child.kill()?;
                let output = child.wait_with_output()?;
                anyhow::bail!(
                    "{case}/{flavor} exceeded child deadline:\n{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output()?;
        assert!(
            output.status.success(),
            "{case}/{flavor}:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

// Break caught: file preparation applies the policy only after building Cores.
#[test]
fn configured_file_binds_the_first_factory_of_each_core() -> Result<()> {
    children("configured")
}

// Break caught: CLI's transferred logger is installed twice or loses metrics.
#[test]
fn initialized_standalone_logging_keeps_capture_and_adds_one_metrics_context() -> Result<()> {
    children("initialized")
}

// Break caught: an empty startup leaves API builders outside the retained context.
#[test]
fn empty_server_exports_two_real_api_created_instances() -> Result<()> {
    children("empty-api")
}

// Break caught: run substitutes an instance map or a physical store cache.
#[test]
fn prepared_registry_and_shared_configuration_database_survive_run() -> Result<()> {
    children("continuity")
}

// Break caught: an omitted section stays Off, or the CLI's transferred
// standalone owner applies its default policy despite legacy root `metrics`.
#[test]
fn omitted_file_telemetry_is_basic_and_legacy_root_metrics_stays_off() -> Result<()> {
    children("file-default")?;
    children("file-legacy-metrics")
}

#[test]
#[ignore = "isolated global-installation subprocess entry point"]
fn file_metrics_child() -> Result<()> {
    let mut runtime = match std::env::var(FLAVOR)?.as_str() {
        "current_thread" => tokio::runtime::Builder::new_current_thread(),
        "multi_thread" => {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.worker_threads(2);
            builder
        }
        flavor => anyhow::bail!("unknown runtime flavor: {flavor}"),
    };
    let runtime = runtime.enable_all().build()?;
    let result = runtime.block_on(async {
        match std::env::var(CASE)?.as_str() {
            "continuity" => registry_continuity().await,
            case @ ("file-default" | "file-legacy-metrics") => file_default_policies(case).await,
            case => configured_metrics(case).await,
        }
    });
    result
}

struct FirstFactory {
    inner: Arc<dyn ComponentFactory>,
    observed: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ComponentFactory for FirstFactory {
    fn descriptor(&self) -> &FactoryDescriptor {
        self.inner.descriptor()
    }

    fn query_authoring(&self) -> Option<&dyn QueryAuthoring> {
        self.inner.query_authoring()
    }

    fn port_formats(&self) -> Option<&PortFormatCatalogue> {
        self.inner.port_formats()
    }

    fn primary_ports(&self) -> PortPrimaries {
        self.inner.primary_ports()
    }

    fn specification_from_config(
        &self,
        id: ComponentId,
        configuration: serde_json::Value,
    ) -> Result<ComponentSpecification> {
        self.inner.specification_from_config(id, configuration)
    }

    fn plan_update(
        &self,
        current: &ComponentSpecification,
        proposed: &ComponentSpecification,
    ) -> Result<ComponentUpdatePlan> {
        self.inner.plan_update(current, proposed)
    }

    fn supports_query_reconstruction(&self) -> bool {
        self.inner.supports_query_reconstruction()
    }

    fn validate_update_plan(
        &self,
        current: &ComponentSpecification,
        proposed: &ComponentSpecification,
        plan: ComponentUpdatePlan,
    ) -> Result<()> {
        self.inner.validate_update_plan(current, proposed, plan)
    }

    fn describe(&self, context: &ConstructionContext) -> Result<ComponentDescription> {
        self.inner.describe(context)
    }

    fn validate(&self, specification: &ComponentSpecification) -> Result<()> {
        self.inner.validate(specification)
    }

    fn validate_resources(
        &self,
        specification: &ComponentSpecification,
        declarations: &BTreeMap<ResourceId, ResourceSpecification>,
        resources: &BTreeMap<ResourceId, ResourceHandle>,
    ) -> Result<()> {
        self.inner
            .validate_resources(specification, declarations, resources)
    }

    fn validate_scope(
        &self,
        graph_id: &str,
        specification: &ComponentSpecification,
        declarations: &BTreeMap<ResourceId, ResourceSpecification>,
        resources: &BTreeMap<ResourceId, ResourceHandle>,
    ) -> Result<()> {
        self.inner
            .validate_scope(graph_id, specification, declarations, resources)
    }

    fn record_schemas(&self) -> Vec<Arc<Schema>> {
        self.inner.record_schemas()
    }

    fn binds_inputs(&self) -> bool {
        self.inner.binds_inputs()
    }

    async fn create(
        &self,
        context: ConstructionContext,
    ) -> Result<ConstructedComponent, ComponentCreationError> {
        let observe = || -> Result<()> {
            let metrics = context
                .telemetry()
                .metric_context()
                .context("first factory missing configured metric context")?;
            let scope =
                TelemetryScope::graph(context.instance_id.clone(), context.graph_id.clone())?
                    .component(
                        context.component_id.clone(),
                        ComponentRole::Transformer,
                        ComponentSemanticKind::Query,
                    );
            metrics
                .plugin(
                    context.telemetry().policy(),
                    &scope,
                    "task4_file",
                    PluginMetricDefinition {
                        leaf: "created",
                        kind: MetricKind::Counter,
                        unit: MetricUnit::One,
                        description: "First factory constructions",
                    },
                    [],
                )?
                .counter()?
                .increment(1);
            self.observed
                .lock()
                .unwrap()
                .push(context.instance_id.to_string());
            Ok(())
        };
        observe().map_err(ComponentCreationError::terminal)?;
        self.inner.create(context).await
    }
}

// File loading cannot supply Rust closures. Register the real native query
// factory on each actual, initially component-free Core before its first
// construction; no alternate Core/bindings/registry is used.
async fn construct_first_query(core: &DrasiLib, observed: Arc<Mutex<Vec<String>>>) -> Result<()> {
    core.register_query_factory(Arc::new(FirstFactory {
        inner: crate::query_test_support::factory(),
        observed,
    }))?;
    let handle = core
        .add_query_with_handle(
            Query::cypher(name("first-factory")?)
                .query("MATCH (n) RETURN n")
                .auto_start(false)
                .build()?,
        )
        .await?;
    timeout(WAIT, handle.wait_created()).await??;
    Ok(())
}

fn port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

struct WalDirectories(Vec<PathBuf>);

impl WalDirectories {
    fn for_ids(ids: &[String]) -> Self {
        let paths = ids
            .iter()
            .map(|id| PathBuf::from("data").join(crate::instance_paths::instance_storage_key(id)))
            .collect::<Vec<_>>();
        assert!(paths.iter().all(|path| !path.exists()));
        Self(paths)
    }
}

impl Drop for WalDirectories {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

async fn wait_for_api(client: &reqwest::Client, base: &str) -> Result<()> {
    timeout(WAIT, async {
        loop {
            if client
                .get(format!("{base}/health"))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

async fn create_instance(
    client: &reqwest::Client,
    base: &str,
    definition: serde_json::Value,
) -> Result<()> {
    let response = client
        .post(format!("{base}/api/v1/instances"))
        .json(&definition)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "instance creation {status}: {body}");
    Ok(())
}

async fn configured_metrics(case: &str) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("task4-file-metrics-")
        .tempdir_in(".")?;
    let ids = ["first", "second"]
        .map(|name| format!("task4-{}-{name}", uuid::Uuid::new_v4()))
        .to_vec();
    let _wal = WalDirectories::for_ids(&ids);
    let global = PrometheusBuilder::new().build_recorder();
    let global_metrics = global.handle();
    ::metrics::set_global_recorder(global)
        .map_err(|_| anyhow::anyhow!("global test recorder already installed"))?;
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&collector)
        .await;
    let config: TelemetryConfig = serde_json::from_value(json!({
        "profile": "basic",
        "tracing": { "serviceName": "task4-file" },
        "metrics": { "backend": "otlp", "otlp": {
            "endpoint": format!("{}/custom/metrics", collector.uri()),
            "protocol": "http/protobuf", "exportInterval": 1, "exportTimeout": 1
        }}
    }))?;
    let path = directory.path().join("server.yaml");
    let instances = if case == "empty-api" {
        vec![]
    } else {
        ids.iter().map(|id| json!({"id": id})).collect()
    };
    std::fs::write(
        &path,
        serde_yaml::to_string(&json!({
            "host": "127.0.0.1", "persistConfig": false, "verifyPlugins": false,
            "telemetry": config, "instances": instances
        }))?,
    )?;
    let plugins = directory.path().join("plugins");
    std::fs::create_dir(&plugins)?;
    let api_port = port()?;
    let mut original_logging = None;
    let mut original_owner = None;
    let preparation = if case == "initialized" {
        let runtime = StandaloneTelemetry::prepare("info".into()).finish().await?;
        original_logging = Some(runtime.bindings().logging().clone());
        original_owner = Some(runtime.cleanup_handle());
        DrasiServer::prepare_new_with_runtime(path, api_port, plugins, false, false, runtime)
    } else {
        DrasiServer::prepare_new(path, api_port, plugins, false, false)
    };
    let cleanup = preparation.cleanup_handle();
    let server = preparation.finish().await?;
    let registry = server.instance_registry.clone();
    cleanup.retain_registry(registry.clone())?;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut prepared = Vec::new();
    if case == "empty-api" {
        assert!(registry.is_empty().await);
    } else {
        assert_eq!(registry.list_ids().await, ids);
        for id in &ids {
            let core = registry
                .get(id)
                .await
                .context("prepared Core unpublished")?;
            if let Some(logging) = &original_logging {
                assert!(Arc::ptr_eq(&core.log_registry(), &logging.registry()));
            }
            construct_first_query(&core, observed.clone()).await?;
            prepared.push((id.clone(), core));
        }
    }
    tokio::time::sleep(Duration::from_millis(1150)).await;
    assert!(
        collector.received_requests().await.unwrap().is_empty(),
        "file preparation exported before run"
    );
    let client = reqwest::Client::builder().timeout(WAIT).build()?;
    let base = format!("http://127.0.0.1:{api_port}");
    let running = tokio::spawn(server.run());
    wait_for_api(&client, &base).await?;
    if case == "empty-api" {
        for id in &ids {
            create_instance(&client, &base, json!({"id": id})).await?;
            let core = registry
                .get(id)
                .await
                .context("API used a different registry")?;
            construct_first_query(&core, observed.clone()).await?;
            prepared.push((id.clone(), core));
        }
    }
    assert_eq!(registry.list_ids().await, ids);
    for (id, core) in &prepared {
        assert!(Arc::ptr_eq(
            core,
            &registry.get(id).await.context("run replaced a Core")?
        ));
    }
    let expected = ids.iter().cloned().collect::<BTreeSet<_>>();
    timeout(WAIT, async {
        loop {
            let (identities, _) = exported(&collector).await?;
            if identities == expected {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    cleanup.wait_until(Instant::now() + WAIT).await?;
    timeout(WAIT, running).await???;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    let (identities, resources) = exported(&collector).await?;
    assert_eq!(identities, expected);
    assert_eq!(resources.len(), 1, "resource changed after activation");
    assert_eq!(
        observed
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        observed.lock().unwrap().len(),
        2,
        "factory constructed twice"
    );
    if let Some(logging) = original_logging {
        assert!(logging.is_closed(), "transferred capture was not joined");
        assert_eq!(
            original_owner
                .context("missing original owner")?
                .status()
                .phase,
            CleanupPhase::Complete
        );
    }
    assert!(
        global_metrics.render().is_empty(),
        "Server observations leaked to the global recorder"
    );
    ::metrics::counter!("task4_file_global_sentinel").increment(1);
    assert!(global_metrics
        .render()
        .contains("task4_file_global_sentinel"));
    Ok(())
}

// The CLI passes `prepare_new_with_runtime` a standalone owner whose bindings
// carry the default policy; legacy root `metrics` must still keep Cores Off.
async fn file_default_policies(case: &str) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("task3-file-default-")
        .tempdir_in(".")?;
    let id = format!("task3-{}-{case}", uuid::Uuid::new_v4());
    let _wal = WalDirectories::for_ids(std::slice::from_ref(&id));
    let legacy = case == "file-legacy-metrics";
    let mut file = json!({
        "host": "127.0.0.1", "persistConfig": false, "verifyPlugins": false,
        "instances": [{"id": id}]
    });
    if legacy {
        file["metrics"] = json!({"endpoint": "http://127.0.0.1:4318/v1/metrics"});
    }
    let path = directory.path().join("server.yaml");
    std::fs::write(&path, serde_yaml::to_string(&file)?)?;
    let plugins = directory.path().join("plugins");
    std::fs::create_dir(&plugins)?;
    let api_port = port()?;
    let preparation = if legacy {
        let runtime = StandaloneTelemetry::prepare("info".into()).finish().await?;
        DrasiServer::prepare_new_with_runtime(path, api_port, plugins, false, false, runtime)
    } else {
        DrasiServer::prepare_new(path, api_port, plugins, false, false)
    };
    let cleanup = preparation.cleanup_handle();
    let server = preparation.finish().await?;
    let core = server
        .instance_registry
        .get(&id)
        .await
        .context("prepared Core unpublished")?;
    assert_eq!(
        core.telemetry().policy().profile(),
        if legacy {
            TelemetryProfile::Off
        } else {
            TelemetryProfile::Basic
        }
    );
    assert!(!core.telemetry().policy().enabled());
    drop(core);
    drop(server);
    cleanup.wait_until(Instant::now() + WAIT).await?;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    Ok(())
}

async fn exported(collector: &MockServer) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut identities = BTreeSet::new();
    let mut resources = BTreeSet::new();
    for request in collector.received_requests().await.unwrap() {
        assert_eq!(request.url.path(), "/custom/metrics");
        assert_eq!(request.headers["content-type"], "application/x-protobuf");
        let body = ExportMetricsServiceRequest::decode(request.body.as_slice())?;
        for resource in body.resource_metrics {
            let attributes = resource.resource.context("missing resource")?.attributes;
            let string = |key| {
                attributes
                    .iter()
                    .find(|a| a.key == key)
                    .and_then(|a| a.value.as_ref())
                    .and_then(|v| v.value.as_ref())
                    .and_then(|v| match v {
                        Value::StringValue(value) => Some(value.clone()),
                        _ => None,
                    })
            };
            assert_eq!(string("service.name").as_deref(), Some("task4-file"));
            resources.insert(string("service.instance.id").context("missing service instance")?);
            for scope in resource.scope_metrics {
                for metric in scope.metrics {
                    if metric.name != "drasi.plugin.task4_file.created" {
                        continue;
                    }
                    let Some(Data::Sum(sum)) = metric.data else {
                        anyhow::bail!("factory counter is not a sum");
                    };
                    for point in sum.data_points {
                        assert_eq!(point.value, Some(Number::AsInt(1)));
                        assert!(point.attributes.iter().any(|a| a.key == "graph_id"));
                        let identity = point
                            .attributes
                            .iter()
                            .find(|a| a.key == "instance_id")
                            .and_then(|a| a.value.as_ref())
                            .and_then(|v| v.value.as_ref());
                        let Some(Value::StringValue(id)) = identity else {
                            anyhow::bail!("factory counter missing actual instance identity");
                        };
                        identities.insert(id.clone());
                    }
                }
            }
        }
    }
    Ok((identities, resources))
}

async fn registry_continuity() -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("task4-registry-")
        .tempdir_in(".")?;
    let ids = ["configured", "api"]
        .map(|name| format!("task4-{}-{name}", uuid::Uuid::new_v4()))
        .to_vec();
    let _wal = WalDirectories::for_ids(&ids);
    let key_file = directory.path().join("key");
    std::fs::write(&key_file, [73; 32])?;
    let store = ConfigurationStoreConfig::Redb {
        path: directory.path().join("configuration.redb"),
        key_file,
    };
    let path = directory.path().join("server.yaml");
    std::fs::write(
        &path,
        serde_yaml::to_string(&json!({
            "host": "127.0.0.1", "persistConfig": false, "verifyPlugins": false,
            "instances": [{"id": ids[0], "configurationStore": store}]
        }))?,
    )?;
    let (bindings, logging) = bindings()?;
    let api_port = port()?;
    let server = DrasiServer::prepare_new_with_telemetry(
        path,
        api_port,
        directory.path().join("plugins"),
        false,
        false,
        bindings,
    )
    .finish()
    .await?;
    let cleanup = server.cleanup_handle();
    let registry = server.instance_registry.clone();
    cleanup.retain_registry(registry.clone())?;
    assert!(cleanup
        .retain_registry(crate::instance_registry::InstanceRegistry::new())
        .is_err());
    let original = registry
        .get(&ids[0])
        .await
        .context("missing configured Core")?;
    let provider = server.configuration_stores.open(&store).await?;
    let running = tokio::spawn(server.run());
    let client = reqwest::Client::builder().timeout(WAIT).build()?;
    let base = format!("http://127.0.0.1:{api_port}");
    wait_for_api(&client, &base).await?;
    // A replacement physical-store cache attempts to reopen the locked redb
    // file, rather than sharing the provider already held by the configured Core.
    create_instance(
        &client,
        &base,
        json!({"id": ids[1], "configurationStore": store}),
    )
    .await?;
    assert_eq!(registry.list_ids().await, ids);
    assert!(Arc::ptr_eq(
        &original,
        &registry
            .get(&ids[0])
            .await
            .context("run replaced configured Core")?
    ));
    assert_eq!(
        provider
            .configured_instance_ids()
            .await?
            .into_iter()
            .collect::<BTreeSet<_>>(),
        ids.iter().cloned().collect()
    );
    cleanup.wait_until(Instant::now() + WAIT).await?;
    timeout(WAIT, running).await???;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert!(!logging.is_closed(), "borrowed capture was closed");
    logging.shutdown(WAIT).await?;
    Ok(())
}
