// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Requires separately built ABI 1.0 standard and ABI 0.15 mock/log/bootstrap libraries.
//! See tests/README.md. Missing binaries are a failed prerequisite, never a skip.

#![allow(clippy::unwrap_used)]

use std::{
    collections::HashSet,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_core::models::{Element, ElementValue, SourceChange};
use drasi_host_sdk::{
    computation::{NativeFactory, NativePlugin},
    lifecycle::PluginLifecycleManager,
    plugin_types::{PluginCategory, PluginEvent},
    registry::VerificationConfig,
};
use drasi_lib::{computation::v1::*, DrasiLib};
use drasi_server::{
    api::{
        shared::handlers::clone_instance,
        v1::{build_plugin_router, routes::build_v1_router},
    },
    computation::{
        build_components, configuration_from_snapshot, register_components, ComputationConfig,
    },
    config::DrasiServerConfig,
    dynamic_loading::load_plugins,
    instance_registry::InstanceRegistry,
    persistence::ConfigPersistence,
    plugin_operations::PluginOperations,
    plugin_orchestrator::PluginOrchestrator,
    plugin_registry::PluginRegistry,
};
use indexmap::IndexMap;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tower::ServiceExt;

const DEADLINE: Duration = Duration::from_secs(20);
const COUNTER: &str = "drasi.standard/volatile-counter";
const MIDDLEWARE: &str = "drasi.standard/middleware";
const ARITHMETIC: &str = "drasi.standard/arithmetic";
const CAPTURE: &str = "drasi.standard/capture";

fn artifact(name: &str) -> PathBuf {
    let directory = std::env::var_os("DRASI_SERVER_TEST_PLUGINS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"));
    let default_path = directory.join(format!(
        "{}{name}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let path = if name == "drasi_computation_standard" {
        std::env::var_os("DRASI_NATIVE_STANDARD_PLUGIN")
            .map(PathBuf::from)
            .unwrap_or(default_path)
    } else {
        default_path
    };
    assert!(path.is_file(), "Missing fixture {}. Build drasi-computation-standard, drasi-source-mock, drasi-reaction-log and drasi-bootstrap-scriptfile with --features dynamic-plugin --offline --target-dir ../drasi-server/target, set DRASI_SERVER_TEST_PLUGINS_DIR, or select the native library with DRASI_NATIVE_STANDARD_PLUGIN. This test does not skip missing native binaries.", path.display());
    path
}

fn plugin() -> Arc<NativePlugin> {
    static PLUGIN: OnceLock<Arc<NativePlugin>> = OnceLock::new();
    PLUGIN
        .get_or_init(|| {
            drasi_host_sdk::computation::load(artifact("drasi_computation_standard"))
                .expect("load real native ABI")
        })
        .clone()
}

fn factories() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut registry);
    registry.register_computation_plugin(plugin()).unwrap();
    registry
}

fn factory(name: &str) -> Arc<NativeFactory> {
    plugin()
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == name)
        .expect("native standard factory")
        .clone()
}

fn id(name: &str) -> ComponentId {
    ComponentId::try_new(name).unwrap()
}
fn resource(name: &str) -> ResourceId {
    ResourceId::try_new(name).unwrap()
}
fn endpoint(component: &str, port: &str) -> Endpoint {
    Endpoint::new(id(component), PortId::try_new(port).unwrap())
}
fn edge(from: &str, to: &str) -> EdgeDefinition {
    EdgeDefinition::new(endpoint(from, "out"), endpoint(to, "in"))
}

struct ReplacementConfiguration;

#[async_trait::async_trait]
impl ConfigurationResolver for ReplacementConfiguration {
    fn validate_reference(&self, key: &str) -> Result<()> {
        anyhow::ensure!(
            key == "secret-json:COUNT",
            "unknown test configuration reference"
        );
        Ok(())
    }

    async fn resolve(&self, key: &str) -> Result<Value> {
        self.validate_reference(key)?;
        Ok(json!(4))
    }
}

struct ExternalPipe(BoundedPipeConfig);

impl PipeProvider for ExternalPipe {
    fn capabilities(&self) -> std::result::Result<PipeCapabilities, PipeError> {
        self.0.capabilities()
    }

    fn create(&self) -> std::result::Result<ProvidedPipe, PipeError> {
        self.0.create()
    }
}

fn definition(output: &Path) -> Result<ComputationConfig> {
    let source = factory(COUNTER);
    let middleware = factory(MIDDLEWARE);
    let transform = factory(ARITHMETIC);
    let capture = factory(CAPTURE);
    let mut source_spec = source.specification(
        id("counter"),
        json!({"stream":"counter/out","count":4,"start":2,"step":3}),
    )?;
    source_spec.configuration.insert(
        "count".into(),
        ConfigurationValue::Reference {
            resource: resource("configuration"),
            key: "secret-json:COUNT".into(),
            secret: true,
        },
    );
    let graph = ComponentBatch::builder()
        .declare_resource(ResourceSpecification {
            id: resource("configuration"),
            role: ResourceRole::SecretStore,
            ownership: ResourceOwnership::Borrowed,
            binding: "instance-configuration".into(),
        })?
        .resource_configuration(resource("configuration"), json!({"kind":"configuration"}))?
        .component(source_spec, source)
        .component(
            middleware.specification(
                id("middleware"),
                json!({
                    "stream":"middleware/out",
                    "middleware":[{
                        "name":"rename", "kind":"relabel",
                        "config":{"labelMappings":{"Counter":"Projected"}},
                    }],
                    "pipeline":["rename"],
                }),
            )?,
            middleware,
        )
        .component(
            transform.specification(
                id("arithmetic"),
                json!({"stream":"arithmetic/out","add":10,"multiply":2}),
            )?,
            transform,
        )
        .component(
            capture.specification(id("capture"), json!({"path":output}))?,
            capture,
        )
        .connect(
            edge("counter", "middleware"),
            Box::new(BoundedPipeConfig { capacity: 1 }),
        )
        .connect(
            edge("middleware", "arithmetic"),
            Box::new(BoundedPipeConfig { capacity: 1 }),
        )
        .connect(
            edge("arithmetic", "capture"),
            Box::new(BoundedPipeConfig { capacity: 1 }),
        )
        .bind_stream(
            endpoint("counter", "out"),
            StreamId::try_new("counter/out")?,
        )
        .bind_stream(
            endpoint("middleware", "out"),
            StreamId::try_new("middleware/out")?,
        )
        .bind_stream(
            endpoint("arithmetic", "out"),
            StreamId::try_new("arithmetic/out")?,
        )
        .build()?;
    let mut definition = graph.auto_start(false).definition;
    definition.revision = GraphRevision(0);
    Ok(ComputationConfig { definition })
}

fn selected_components(config: &ComputationConfig) -> Value {
    json!({"components":config.definition.components.iter()
        .map(|component| component.descriptor.id().as_str()).collect::<Vec<_>>()})
}

fn selected_removal(config: &ComputationConfig) -> Value {
    json!({
        "components":config.definition.components.iter().map(|component| component.descriptor.id().as_str()).collect::<Vec<_>>(),
        "resources":config.definition.resources.iter().map(|resource| resource.id.as_str()).collect::<Vec<_>>(),
    })
}

async fn remove_components(core: &DrasiLib, config: &ComputationConfig) -> Result<()> {
    let control = core.computation_control()?;
    let mut changes = vec![DesiredMutation::RemoveComponents {
        selection: GraphSelection::Exact(
            config
                .definition
                .components
                .iter()
                .map(|node| node.descriptor.id().clone())
                .collect(),
        ),
        policy: RemovalPolicy::Reject,
    }];
    changes.extend(config.definition.resources.iter().map(|resource| {
        DesiredMutation::RemoveResource {
            resource: resource.id.clone(),
            policy: RemovalPolicy::Reject,
        }
    }));
    let preview = control
        .preview(control.desired_snapshot().revision, changes)
        .await?;
    let report = control
        .reconcile(preview, TopologyBindings::default())
        .await?;
    anyhow::ensure!(
        report.committed && report.summary == OperationSummary::Completed,
        "{report:?}"
    );
    Ok(())
}

async fn core(name: &str) -> Result<Arc<DrasiLib>> {
    Ok(Arc::new(
        DrasiLib::builder()
            .with_id(name)
            .with_secret_store_provider(Arc::new(
                drasi_lib::secret_store::MemorySecretStoreProvider::new().with_secret("COUNT", "4"),
            ))
            .build()
            .await?,
    ))
}

async fn assert_clone_and_save_rejected(
    source: Arc<DrasiLib>,
    original: &DrasiServerConfig,
    directory: &Path,
    reason: &str,
) -> Result<()> {
    source
        .add_query(
            drasi_lib::Query::cypher("must-not-clone")
                .query("MATCH (n) RETURN n")
                .auto_start(false)
                .build(),
        )
        .await?;
    let snapshot = source.snapshot_computation_configuration().await?;
    let error = configuration_from_snapshot(&snapshot).unwrap_err();
    assert!(format!("{error:#}").contains(reason), "{error:#}");
    let target = core("clone-target").await?;
    let instances = InstanceRegistry::new();
    let source_id = snapshot.instance.instance_id;
    instances
        .add(source_id.clone(), source.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    instances
        .add("clone-target".into(), target.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let error = clone_instance(
        instances.clone(),
        Arc::new(false),
        Arc::new(RwLock::new(factories())),
        None,
        "clone-target",
        &source_id,
    )
    .await
    .err()
    .context("unsupported bindings must reject clone")?;
    assert_eq!(error.code, "INVALID_REQUEST");
    assert!(error
        .details
        .and_then(|detail| detail.technical_details)
        .context("clone diagnostic")?
        .contains(reason));
    assert!(target.snapshot_configuration().await?.queries.is_empty());
    assert!(
        configuration_from_snapshot(&target.snapshot_computation_configuration().await?)?.is_none()
    );
    let path = directory.join("server.yaml");
    std::fs::write(&path, "id: original\n")?;
    let persistence = ConfigPersistence::new(
        path.clone(),
        instances,
        "127.0.0.1".into(),
        8080,
        "info".into(),
        true,
        IndexMap::new(),
        IndexMap::new(),
        None,
        original,
    );
    let error = persistence.save().await.unwrap_err();
    assert!(format!("{error:#}").contains(reason), "{error:#}");
    assert_eq!(std::fs::read_to_string(path)?, "id: original\n");
    source.shutdown().await?;
    target.shutdown().await?;
    Ok(())
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: &str,
    media_type: &str,
) -> Result<(StatusCode, Value)> {
    let response = tokio::time::timeout(
        DEADLINE,
        app.clone().oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", media_type)
                .body(Body::from(body.to_owned()))?,
        ),
    )
    .await??;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).with_context(|| {
            format!("unexpected HTTP body: {}", String::from_utf8_lossy(&bytes))
        })?,
    ))
}

async fn exact_output(path: &Path) -> Result<()> {
    tokio::time::timeout(DEADLINE, async {
        loop {
            match tokio::fs::read(path).await {
                Ok(bytes) if bytes.iter().filter(|byte| **byte == b'\n').count() == 4 => break,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let mut codec = EnvelopeCodec::new(NonZeroUsize::new(64 * 1024 * 1024).unwrap());
    codec.register_schema(GraphChangeCodec::schema())?;
    let bytes = tokio::fs::read(path).await?;
    let mut values = Vec::new();
    for (index, line) in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        let envelope = codec.decode(line)?;
        assert_eq!(envelope.system().sequence(), index as u64 + 1);
        assert_eq!(envelope.system().stream().as_str(), "arithmetic/out");
        let changes = GraphChangeCodec::decode_changes(&envelope)?;
        let [SourceChange::Insert {
            element:
                Element::Node {
                    properties,
                    metadata,
                },
        }] = changes.as_slice()
        else {
            anyhow::bail!("expected exactly one inserted node, got {changes:?}");
        };
        let Some(ElementValue::Integer(value)) = properties.get("value") else {
            anyhow::bail!("missing value");
        };
        assert_eq!(
            properties.get("batch_count"),
            Some(&ElementValue::Integer(index as i64 + 1))
        );
        assert_eq!(metadata.labels.as_ref(), &[Arc::<str>::from("Projected")]);
        values.push(*value);
    }
    assert_eq!(values, [24, 30, 36, 42]);
    Ok(())
}

#[tokio::test]
async fn shared_discovery_loads_both_abis_and_runtime_inventory() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for name in [
        "drasi_computation_standard",
        "drasi_source_mock",
        "drasi_reaction_log",
        "drasi_bootstrap_scriptfile",
    ] {
        let path = artifact(name);
        std::fs::copy(&path, directory.path().join(path.file_name().unwrap()))?;
    }
    let mut registry = PluginRegistry::new();
    let stats = load_plugins(directory.path(), &mut registry, None, None)?;
    assert_eq!(stats.plugins_loaded, 4);
    assert_eq!(stats.plugins_failed, 0, "{:?}", stats.failures);
    assert_eq!(stats.source_descriptors, 1);
    assert_eq!(stats.reaction_descriptors, 1);
    assert_eq!(stats.bootstrap_descriptors, 1);
    assert_eq!(stats.computation_factories, 4);
    assert!(registry.get_source("mock").is_some());
    assert!(registry.get_reaction("log").is_some());
    assert!(registry.get_bootstrapper("scriptfile").is_some());
    assert_eq!(
        registry.computation_plugin_metadata()[0].abi_version,
        "1.0.0"
    );
    for record in &stats.loaded_plugins {
        assert_eq!(
            record.sdk_version,
            if record.plugin_id.starts_with("computation:") {
                "1.0.0"
            } else {
                "0.15.0"
            }
        );
    }
    let orchestrator = Arc::new(PluginOrchestrator::with_plugins_dir(
        Arc::new(PluginLifecycleManager::new(Arc::new(RwLock::new(registry)))),
        directory.path().to_owned(),
    ));
    let mut events = orchestrator.subscribe();
    orchestrator
        .record_startup_plugins(&stats.loaded_plugins)
        .await;
    let mut native_events = 0;
    for _ in 0..4 {
        if let PluginEvent::Loaded { kinds, .. } = events.try_recv()? {
            native_events += usize::from(
                kinds
                    .iter()
                    .any(|kind| kind.category == PluginCategory::Computation),
            );
        }
    }
    assert_eq!(native_events, 1);
    let app = build_plugin_router(orchestrator, InstanceRegistry::new(), Arc::new(false));
    let (status, metadata) = request(&app, "GET", "/computation", "", "application/json").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        metadata["plugins"][0]["factories"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let (status, kinds) = request(&app, "GET", "/kinds", "", "application/json").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(kinds["computation"].as_array().unwrap().len(), 4);
    Ok(())
}

#[tokio::test]
async fn native_resource_only_configuration_can_be_inspected_and_removed() -> Result<()> {
    let core = core("resources").await?;
    let registry = InstanceRegistry::new();
    registry
        .add("resources".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let app = build_v1_router(
        registry,
        Arc::new(false),
        None,
        Arc::new(RwLock::new(factories())),
        None,
    );
    let batch = ComponentBatch::builder()
        .declare_resource(ResourceSpecification {
            id: resource("indexes"),
            role: ResourceRole::IndexBackend,
            ownership: ResourceOwnership::Graph,
            binding: "indexes".into(),
        })?
        .resource_configuration(resource("indexes"), json!({"kind":"memoryIndexes"}))?
        .build()?;
    let config = ComputationConfig {
        definition: batch.definition,
    };
    let (status, body) = request(
        &app,
        "POST",
        "/instances/resources/computation/components",
        &serde_json::to_string(&config)?,
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let snapshot = core.snapshot_computation_configuration().await?;
    let native = snapshot
        .native_components
        .as_ref()
        .context("native resource declaration")?;
    assert!(native.topology.components.is_empty());
    assert_eq!(
        native.topology.resource_configurations[&resource("indexes")],
        json!({"kind":"memoryIndexes"})
    );
    let (status, body) = request(
        &app,
        "DELETE",
        "/instances/resources/computation/components",
        &json!({"components":[],"resources":["indexes"]}).to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(core
        .snapshot_computation_configuration()
        .await?
        .native_components
        .is_none());
    assert_eq!(
        core.inspect_computation_inventory()
            .await?
            .scopes
            .values()
            .filter(|scope| scope.owner.is_none())
            .count(),
        1
    );
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn native_api_persist_restart_clone_and_empty_lists_roundtrip() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("private-capture-path.jsonl");
    let mut config = definition(&output)?;
    config
        .definition
        .control_connections
        .push((id("counter"), id("capture")));
    let source = core("first").await?;
    let target = core("second").await?;
    let instances = InstanceRegistry::new();
    instances
        .add("first".into(), source.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    instances
        .add("second".into(), target.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let plugins = Arc::new(RwLock::new(factories()));
    let path = directory.path().join("server.yaml");
    let original: DrasiServerConfig =
        serde_json::from_value(json!({"instances":[{"id":"first"},{"id":"second"}]}))?;
    let persistence = Arc::new(ConfigPersistence::new(
        path.clone(),
        instances.clone(),
        "127.0.0.1".into(),
        8080,
        "info".into(),
        true,
        IndexMap::new(),
        IndexMap::new(),
        None,
        &original,
    ));
    let app = build_v1_router(
        instances.clone(),
        Arc::new(false),
        Some(persistence.clone()),
        plugins.clone(),
        None,
    );
    let (status, body) = request(
        &app,
        "POST",
        "/instances/first/computation/components",
        &serde_yaml::to_string(&config)?,
        "application/yaml",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let handle = source.computation_control()?;
    for component in &config.definition.components {
        assert_eq!(
            handle.observed().components[component.descriptor.id()].realization,
            RealizationState::Created
        );
    }
    assert!(!output.exists(), "registration must not start components");
    let (status, body) = request(
        &app,
        "POST",
        "/instances/first/computation/start",
        &selected_components(&config).to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    exact_output(&output).await?;
    let (status, inspection) = request(
        &app,
        "GET",
        "/instances/first/computation",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        inspection["data"]["components"].as_array().unwrap().len(),
        5
    );
    for specification in &config.definition.components {
        let inspected = inspection["data"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|component| component["id"] == specification.descriptor.id().as_str())
            .expect("every desired component is inspectable");
        let implementation = match &specification.construction {
            drasi_lib::computation::v1::ComponentConstruction::Factory(factory) => {
                serde_json::to_value(&factory.implementation)?
            }
            drasi_lib::computation::v1::ComponentConstruction::External { .. } => Value::Null,
        };
        assert_eq!(inspected["implementation"], implementation);
    }
    assert!(inspection["data"]["relationships"]
        .as_array()
        .unwrap()
        .contains(&json!({
            "representation": "ControlConnection",
            "from": "counter",
            "to": "capture"
        })));
    let public = serde_json::to_string(&inspection)?;
    assert!(!public.contains("private-capture-path"));
    assert!(!public.contains("secret-json:COUNT"));
    assert!(!public.contains("configuration_version"));
    let (status, full) = request(
        &app,
        "GET",
        "/instances/first/computation/configuration",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(full["data"]["version"], 1);
    assert_eq!(
        full["data"]["native_components"]["configurations"]["counter"]["values"]["count"],
        4
    );
    assert_eq!(
        full["data"]["native_components"]["topology"]["resource_configurations"]["configuration"],
        json!({"kind":"configuration"})
    );
    let native_id = drasi_host_sdk::plugin_registry::computation_plugin_id(plugin().metadata());
    let orchestrator = Arc::new(PluginOrchestrator::new(Arc::new(
        PluginLifecycleManager::new(plugins.clone()),
    )));
    orchestrator
        .record_startup_plugins(&[drasi_server::dynamic_loading::StartupPluginRecord {
            plugin_id: native_id.clone(),
            file_path: artifact("drasi_computation_standard"),
            kinds: Vec::new(),
            plugin_version: plugin().metadata().plugin.version.to_string(),
            sdk_version: "1.0.0".into(),
        }])
        .await;
    let plugin_app = build_plugin_router(orchestrator, instances.clone(), Arc::new(false));
    let (status, dependents) = request(
        &plugin_app,
        "GET",
        &format!("/{native_id}/dependents"),
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{dependents}");
    assert_eq!(dependents["dependentCount"], 4);

    let clone = clone_instance(
        instances.clone(),
        Arc::new(false),
        plugins.clone(),
        Some(persistence.clone()),
        "second",
        "first",
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?
    .0
    .data
    .context("clone response")?;
    assert!(clone.success, "{:?}", clone.errors);
    assert_eq!(
        clone
            .components_created
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>(),
        HashSet::from(["counter", "middleware", "arithmetic", "capture"])
    );
    for component in &config.definition.components {
        assert!(
            !target
                .computation_control()?
                .desired_snapshot()
                .lifecycle_policies[component.descriptor.id()]
            .auto_start
        );
    }
    let saved = drasi_server::load_config_file(&path)?;
    assert!(saved.computation.is_none());
    assert_eq!(saved.instances.len(), 2);
    for instance in &saved.instances {
        assert_eq!(
            serde_json::to_value(&instance.computation)?,
            serde_json::to_value(Some(&config))?
        );
    }
    source.shutdown().await?;
    target.shutdown().await?;

    let restored = core("first").await?;
    register_components(
        saved.instances[0].computation.as_ref().unwrap(),
        &restored,
        &factories(),
    )
    .await?;
    std::fs::remove_file(&output)?;
    assert_eq!(
        restored
            .computation_control()?
            .start_requested(
                restored.computation_control()?.desired_snapshot().revision,
                GraphSelection::Exact(
                    config
                        .definition
                        .components
                        .iter()
                        .map(|node| node.descriptor.id().clone())
                        .collect()
                ),
            )
            .await?
            .summary,
        OperationSummary::Completed
    );
    exact_output(&output).await?;
    assert_eq!(
        serde_json::to_value(configuration_from_snapshot(
            &restored.snapshot_computation_configuration().await?
        )?)?,
        serde_json::to_value(Some(&config))?
    );
    restored.shutdown().await?;

    let first = core("first").await?;
    let single = InstanceRegistry::new();
    single
        .add("first".into(), first.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    register_components(&config, &first, &factories()).await?;
    let persistence = ConfigPersistence::new(
        path.clone(),
        single,
        "127.0.0.1".into(),
        8080,
        "info".into(),
        true,
        IndexMap::new(),
        IndexMap::new(),
        None,
        &saved,
    );
    persistence
        .register_instance(saved.instances[0].clone())
        .await;
    remove_components(&first, &config).await?;
    persistence.save().await?;
    assert!(drasi_server::load_config_file(&path)?.computation.is_none());
    first.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn component_api_rejects_missing_factories_resources_and_read_only_mutations() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = definition(&directory.path().join("capture.jsonl"))?;
    let core = core("test").await?;
    let instances = InstanceRegistry::new();
    instances
        .add("test".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let plugins = Arc::new(RwLock::new(factories()));
    let writable = build_v1_router(
        instances.clone(),
        Arc::new(false),
        None,
        plugins.clone(),
        None,
    );
    let mut missing_factory = config.clone();
    if let ComponentConstruction::Factory(spec) =
        &mut missing_factory.definition.components[0].construction
    {
        spec.implementation.name = "not-installed".into();
    }
    let mut missing_resource = config.clone();
    missing_resource.definition.resource_configurations.clear();
    for invalid in [missing_factory, missing_resource] {
        let (status, body) = request(
            &writable,
            "POST",
            "/instances/test/computation/components",
            &serde_json::to_string(&invalid)?,
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "INVALID_REQUEST");
        assert!(core.computation_component("counter").is_err());
    }
    let read_only = build_v1_router(instances, Arc::new(true), None, plugins, None);
    for (method, path, payload) in [
        (
            "POST",
            "/instances/test/computation/components",
            serde_json::to_value(&config)?,
        ),
        (
            "POST",
            "/instances/test/computation/start",
            selected_components(&config),
        ),
        (
            "POST",
            "/instances/test/computation/stop",
            selected_components(&config),
        ),
        (
            "DELETE",
            "/instances/test/computation/components",
            selected_removal(&config),
        ),
    ] {
        let (status, body) = request(
            &read_only,
            method,
            path,
            &payload.to_string(),
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["code"], "CONFIG_READ_ONLY");
    }
    let (status, _) = request(
        &read_only,
        "GET",
        "/instances/test/computation/configuration",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = request(
        &writable,
        "GET",
        "/instances/missing/computation",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn failed_native_creation_remains_inspectable_and_start_returns_an_error() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut config = definition(&directory.path().join("capture.jsonl"))?;
    if let ComponentConstruction::Factory(spec) = &mut config.definition.components[0].construction
    {
        spec.configuration
            .insert("count".into(), ConfigurationValue::Literal(json!(-1)));
    }
    let core = core("failed-native").await?;
    let instances = InstanceRegistry::new();
    instances
        .add("failed-native".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let app = build_v1_router(
        instances,
        Arc::new(false),
        None,
        Arc::new(RwLock::new(factories())),
        None,
    );
    let (status, body) = request(
        &app,
        "POST",
        "/instances/failed-native/computation/components",
        &serde_json::to_string(&config)?,
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        core.computation_component("counter")?
            .observed()?
            .realization,
        RealizationState::CreationFailed
    );
    let (status, body) = request(
        &app,
        "POST",
        "/instances/failed-native/computation/start",
        &selected_components(&config).to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(body["code"], "COMPUTATION_OPERATION_FAILED");
    let (status, body) = request(
        &app,
        "GET",
        "/instances/failed-native/computation",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let counter = body["data"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["id"] == "counter")
        .unwrap();
    assert_eq!(counter["realization"], "CreationFailed");
    let (status, body) = request(
        &app,
        "DELETE",
        "/instances/failed-native/computation/components",
        &selected_removal(&config).to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn rebind_does_not_recreate_a_stale_resource_recipe_on_save() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = definition(&directory.path().join("capture.jsonl"))?;
    let core = core("rebind").await?;
    let plugins = factories();
    register_components(&config, &core, &plugins).await?;
    let control = core.computation_control()?;
    let preview = control
        .preview(
            control.desired_snapshot().revision,
            vec![DesiredMutation::RebindResource(resource("configuration"))],
        )
        .await?;
    let rebound = control
        .reconcile(
            preview,
            TopologyBindings {
                factories: plugins.computation_factory_registry()?,
                resources: [(
                    resource("configuration"),
                    ResourceHandle::new(
                        ResourceRole::SecretStore,
                        Arc::new(ConfigurationResolverResource(Arc::new(
                            ReplacementConfiguration,
                        ))),
                    ),
                )]
                .into(),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(rebound.summary, OperationSummary::Completed);
    let snapshot = core.snapshot_computation_configuration().await?;
    assert!(!snapshot
        .native_components
        .as_ref()
        .unwrap()
        .topology
        .resource_configurations
        .contains_key(&resource("configuration")));
    assert!(configuration_from_snapshot(&snapshot).is_err());
    let original = DrasiServerConfig {
        computation: Some(config),
        ..Default::default()
    };
    assert_clone_and_save_rejected(
        core,
        &original,
        directory.path(),
        "has no construction recipe",
    )
    .await
}

#[tokio::test]
async fn snapshot_conversion_rejects_every_undeclared_resource_reference() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = definition(&directory.path().join("capture.jsonl"))?;
    let core = core("required-resources").await?;
    register_components(&config, &core, &factories()).await?;
    let snapshot = core.snapshot_computation_configuration().await?;
    for kind in [
        "configuration",
        "factory",
        "component",
        "retained",
        "ranked",
    ] {
        let mut incomplete = snapshot.clone();
        let topology = &mut incomplete.native_components.as_mut().unwrap().topology;
        match kind {
            "configuration" => {
                topology.resources.clear();
                topology.resource_configurations.clear();
            }
            "factory" => {
                let ComponentConstruction::Factory(specification) =
                    &mut topology.components[0].construction
                else {
                    anyhow::bail!("expected factory declaration");
                };
                specification
                    .dependencies
                    .insert("required".into(), vec![resource("unbound")]);
            }
            "component" => {
                topology
                    .component_resources
                    .insert(id("counter"), [resource("unbound")].into());
            }
            "retained" => {
                topology.relationships[0].pipe = DesiredPipe::Retained(RetainedPipeConfig {
                    resource: resource("unbound"),
                    capacity: NonZeroUsize::new(1).unwrap(),
                    durable: false,
                    retention: RetentionPolicy::Backpressure,
                    gap_policy: ReplayGapPolicy::Strict,
                });
            }
            "ranked" => {
                topology.relationships[0].pipe = DesiredPipe::Ranked(RankedInputPipeConfig {
                    queue: resource("unbound"),
                    capacity: 1,
                    source_rank: 0,
                    source_id: None,
                    drop_when_full: false,
                });
            }
            _ => unreachable!(),
        }
        let error = configuration_from_snapshot(&incomplete).unwrap_err();
        assert!(
            format!("{error:#}").contains("has no declaration or construction recipe"),
            "{kind}: {error:#}"
        );
    }
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn external_pipe_bindings_reject_clone_and_save_before_any_mutation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = core("external-pipe").await?;
    let counter = factory(COUNTER);
    let capture = factory(CAPTURE);
    let graph = ComponentBatch::builder()
        .component(
            counter.specification(id("counter"), json!({"stream":"counter/out","count":0}))?,
            counter,
        )
        .component(
            capture.specification(
                id("capture"),
                json!({"path":directory.path().join("capture.jsonl")}),
            )?,
            capture,
        )
        .connect(
            edge("counter", "capture"),
            Box::new(ExternalPipe(BoundedPipeConfig { capacity: 1 })),
        )
        .bind_stream(
            endpoint("counter", "out"),
            StreamId::try_new("counter/out")?,
        )
        .build()?;
    source.add_components(graph.auto_start(false)).await?;
    assert_clone_and_save_rejected(
        source,
        &DrasiServerConfig::default(),
        directory.path(),
        "external pipe",
    )
    .await
}

#[tokio::test]
async fn openapi_refreshes_native_factory_schemas_after_runtime_registration() -> Result<()> {
    use utoipa::OpenApi;
    let plugins = Arc::new(RwLock::new(PluginRegistry::new()));
    let cache = drasi_server::api::v1::OpenApiCache::new(
        drasi_server::api::ApiDocV1::openapi(),
        plugins.clone(),
        0,
    );
    let before = cache.get_spec().await;
    assert!(!before
        .components
        .unwrap()
        .schemas
        .keys()
        .any(|key| key.starts_with("NativeComputationConfig_")));
    plugins
        .write()
        .await
        .register_computation_plugin(plugin())?;
    let after = cache.get_spec().await;
    let schemas = after.components.as_ref().unwrap();
    assert_eq!(
        schemas
            .schemas
            .keys()
            .filter(|key| key.starts_with("NativeComputationConfig_"))
            .count(),
        4
    );
    for name in [
        "ComputationConfig",
        "ComputationResourceConfig",
        "ComputationGraphInfo",
        "ComputationGraphInspection",
        "ComputationConfigurationSnapshotSchema",
    ] {
        assert!(schemas.schemas.contains_key(name), "{name}");
    }
    for path in [
        "/api/v1/plugins/computation",
        "/api/v1/instances/{instanceId}/computation/configuration",
        "/api/v1/instances/{instanceId}/computation",
        "/api/v1/instances/{instanceId}/computation/components",
        "/api/v1/instances/{instanceId}/computation/start",
        "/api/v1/instances/{instanceId}/computation/stop",
    ] {
        assert!(after.paths.paths.contains_key(path), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn unsupported_external_bindings_fail_before_partial_clone_or_persistence() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = core("source").await?;
    let transformer = MiddlewareTransformer::new(
        MiddlewareTransformerDefinition {
            id: id("external"),
            output_stream: StreamId::try_new("external/out")?,
            middleware: Vec::new(),
            pipeline: Vec::new(),
        },
        source.middleware_registry(),
    )?;
    let counter = factory(COUNTER);
    let capture = factory(CAPTURE);
    let graph = ComponentBatch::builder()
        .component(
            counter.specification(id("counter"), json!({"stream":"counter/out","count":0}))?,
            counter,
        )
        .transformer(Box::new(transformer))
        .component(
            capture.specification(
                id("capture"),
                json!({"path":directory.path().join("capture.jsonl")}),
            )?,
            capture,
        )
        .connect(
            edge("counter", "external"),
            Box::new(BoundedPipeConfig { capacity: 1 }),
        )
        .connect(
            edge("external", "capture"),
            Box::new(BoundedPipeConfig { capacity: 1 }),
        )
        .bind_stream(
            endpoint("counter", "out"),
            StreamId::try_new("counter/out")?,
        )
        .bind_stream(
            endpoint("external", "out"),
            StreamId::try_new("external/out")?,
        )
        .build()?;
    source.add_components(graph.auto_start(false)).await?;
    assert_clone_and_save_rejected(
        source,
        &DrasiServerConfig::default(),
        directory.path(),
        "external component",
    )
    .await
}

#[tokio::test]
async fn native_transaction_factories_bind_explicit_index_provider_recipes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for recipe in [
        json!({"kind":"memoryIndexes"}),
        json!({"kind":"rocksdbIndexes","path":directory.path().join("indexes")}),
    ] {
        let core = core("transaction-host").await?;
        let plugins = factories();
        let registry = plugins.transactional_transformer_registry(core.middleware_registry())?;
        let transaction = TransactionTransformerDefinition {
            graph_id: core.computation_info().await?.id,
            id: id("transaction"),
            output_stream: StreamId::try_new("transaction/out")?,
            steps: vec![TransactionStepDefinition {
                id: id("arithmetic"),
                implementation: factory(ARITHMETIC).metadata().implementation.clone(),
                configuration_version: 1,
                configuration: json!({"stream":"step/out","add":1}),
            }],
            outbox_capacity: NonZeroUsize::new(8).unwrap(),
        };
        let specification =
            transaction.specification(&registry, resource("transformers"), resource("indexes"))?;
        let mut config = definition(&directory.path().join("unused.jsonl"))?;
        config.definition.allow_incomplete = true;
        config.definition.relationships.clear();
        config.definition.components = vec![DesiredComponent {
            descriptor: specification.descriptor.clone(),
            role: specification.role,
            completion: specification.completion,
            streams: Default::default(),
            lifecycle: LifecyclePolicy::default(),
            input_merge: InputMergePolicy::default(),
            construction: ComponentConstruction::Factory(specification),
        }];
        config.definition.resources = vec![
            ResourceSpecification {
                id: resource("indexes"),
                role: ResourceRole::IndexBackend,
                ownership: ResourceOwnership::Graph,
                binding: "indexes".into(),
            },
            ResourceSpecification {
                id: resource("transformers"),
                role: ResourceRole::Component,
                ownership: ResourceOwnership::Borrowed,
                binding: "transformers".into(),
            },
        ];
        config.definition.resource_configurations = [
            (resource("indexes"), recipe.clone()),
            (
                resource("transformers"),
                json!({"kind":"transactionalTransformers"}),
            ),
        ]
        .into();
        let result = build_components(
            &config,
            &core,
            plugins.computation_factory_registry()?,
            registry,
        )
        .await;
        if recipe["kind"] == "memoryIndexes" {
            let error = result
                .err()
                .context("durable transactions must reject volatile indexes")?;
            assert!(
                error.to_string().contains("persistent and atomic"),
                "{error:#}"
            );
            core.shutdown().await?;
            continue;
        }
        let graph = result?;
        let report = core.add_components(graph.auto_start(false)).await?;
        assert_eq!(report.summary, OperationSummary::Completed);
        let exported =
            configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?;
        assert_eq!(
            exported
                .as_ref()
                .unwrap()
                .definition
                .resource_configurations[&resource("indexes")],
            recipe
        );
        remove_components(&core, &config).await?;
        core.shutdown().await?;
    }
    Ok(())
}

#[tokio::test]
async fn local_auto_install_preserves_declared_abi_without_filename_inference() -> Result<()> {
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    let native_filename = format!(
        "{}drasi_source_native{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let legacy_filename = format!(
        "{}drasi_computation_legacy{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    std::fs::copy(
        artifact("drasi_computation_standard"),
        source.path().join(&native_filename),
    )?;
    std::fs::copy(
        artifact("drasi_source_mock"),
        source.path().join(&legacy_filename),
    )?;
    let config = DrasiServerConfig {
        auto_install_plugins: true,
        verify_plugins: false,
        plugin_registry: Some(source.path().to_string_lossy().into_owned()),
        plugins: ["source/native", "computation/legacy"]
            .into_iter()
            .map(|reference| drasi_server::config::PluginDependency {
                reference: reference.into(),
            })
            .collect(),
        ..Default::default()
    };
    let resolved =
        drasi_server::plugin_install::auto_install_plugins(&config, destination.path(), false)
            .await?;
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].filename, native_filename);
    assert_eq!(resolved[0].abi_family.as_deref(), Some("computation"));
    assert_eq!(resolved[0].abi_version.as_deref(), Some("1.0.0"));
    assert_eq!(resolved[0].sdk_version, "1.0.0");
    assert_eq!(resolved[1].filename, legacy_filename);
    assert_eq!(resolved[1].abi_family, None);
    assert_eq!(resolved[1].abi_version, None);
    assert_eq!(resolved[1].sdk_version, "0.15.0");
    Ok(())
}

#[tokio::test]
async fn runtime_loading_verification_events_and_replacement_policy() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = artifact("drasi_computation_standard");
    let path = directory.path().join(source.file_name().unwrap());
    std::fs::copy(&source, &path)?;
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let orchestrator = PluginOrchestrator::with_ops(
        Arc::new(PluginLifecycleManager::new(registry.clone())),
        directory.path().to_owned(),
        PluginOperations::new(
            directory.path().to_owned(),
            directory.path().to_string_lossy().into(),
        ),
        VerificationConfig::default(),
    );
    let mut events = orchestrator.subscribe();
    let info = orchestrator.load_plugin_locked(&path, None).await?;
    assert_eq!(info.sdk_version, "1.0.0");
    assert!(matches!(events.try_recv()?, PluginEvent::Loaded { .. }));
    assert_eq!(registry.read().await.computation_plugin_metadata().len(), 1);
    let hash = drasi_server::plugin_lockfile::compute_file_hash(&path)?;
    assert!(orchestrator
        .load_plugin_locked(&path, None)
        .await
        .unwrap_err()
        .to_string()
        .contains("restart"));
    assert!(orchestrator
        .install_and_load("computation/standard", None, None)
        .await
        .is_err());
    assert_eq!(
        drasi_server::plugin_lockfile::compute_file_hash(&path)?,
        hash
    );
    let verified = PluginOrchestrator::with_ops(
        Arc::new(PluginLifecycleManager::new(Arc::new(RwLock::new(
            PluginRegistry::new(),
        )))),
        directory.path().to_owned(),
        PluginOperations::new(directory.path().to_owned(), "unused".into()),
        VerificationConfig {
            enabled: true,
            ..Default::default()
        },
    );
    assert!(verified
        .load_plugin_locked(&path, None)
        .await
        .unwrap_err()
        .to_string()
        .contains("lockfile"));

    let installed_directory = tempfile::tempdir()?;
    let installed_registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let installer = PluginOrchestrator::with_ops(
        Arc::new(PluginLifecycleManager::new(installed_registry.clone())),
        installed_directory.path().to_owned(),
        PluginOperations::new(
            installed_directory.path().to_owned(),
            directory.path().to_string_lossy().into(),
        ),
        VerificationConfig::default(),
    );
    let installed = installer
        .install_and_load("computation/standard", None, None)
        .await?;
    assert_eq!(installed.sdk_version, "1.0.0");
    assert_eq!(
        installed_registry
            .read()
            .await
            .computation_plugin_metadata()
            .len(),
        1
    );
    let lock = drasi_server::plugin_lockfile::PluginLockfile::read(installed_directory.path())?
        .context("installed plugin lock")?;
    let entry = lock
        .get("computation/standard")
        .context("native lock entry")?;
    assert_eq!(entry.sdk_version, "1.0.0");
    assert!(entry.core_version.is_empty());
    assert!(entry.lib_version.is_empty());
    Ok(())
}

#[tokio::test]
async fn watcher_native_candidate_uses_the_same_verified_family_loader() -> Result<()> {
    use drasi_host_sdk::{
        plugin_types::PluginFileEvent,
        watcher::{PluginWatcher, PluginWatcherConfig},
    };
    let directory = tempfile::tempdir()?;
    let mut watcher = PluginWatcher::new(PluginWatcherConfig {
        plugins_dir: directory.path().to_owned(),
        debounce: Duration::from_millis(50),
    });
    let mut events = watcher.subscribe();
    watcher.start()?;
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let orchestrator = PluginOrchestrator::with_plugins_dir(
        Arc::new(PluginLifecycleManager::new(registry.clone())),
        directory.path().to_owned(),
    );
    let source = artifact("drasi_computation_standard");
    let path = directory.path().join(source.file_name().unwrap());
    std::fs::copy(source, &path)?;
    let candidate = tokio::time::timeout(DEADLINE, async {
        loop {
            match events.recv().await? {
                PluginFileEvent::Added(candidate) | PluginFileEvent::Changed(candidate)
                    if candidate.file_name() == path.file_name() =>
                {
                    return Ok::<_, anyhow::Error>(candidate)
                }
                _ => {}
            }
        }
    })
    .await??;
    let info = orchestrator.load_plugin_locked(&candidate, None).await?;
    assert_eq!(info.sdk_version, "1.0.0");
    assert_eq!(registry.read().await.computation_plugin_metadata().len(), 1);
    Ok(())
}

#[test]
fn allowlist_precedes_dlopen_and_wrong_native_abi_never_falls_back() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let marker = directory.path().join("called");
    let library = directory.path().join(format!(
        "{}drasi_computation_bad{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let output = std::process::Command::new("rustc")
        .args([
            "--edition=2021",
            "--crate-type=cdylib",
            "--crate-name=drasi_computation_bad",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native_bad_abi.rs"))
        .arg("-o")
        .arg(&library)
        .env("DRASI_BAD_ABI_MARKER", &marker)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut registry = PluginRegistry::new();
    let denied = load_plugins(directory.path(), &mut registry, None, Some(&HashSet::new()))?;
    assert_eq!(denied.plugins_loaded, 0);
    assert_eq!(denied.plugins_failed, 0);
    assert!(!marker.with_extension("constructor").exists());
    assert!(!marker.with_extension("metadata").exists());
    let attempted = load_plugins(directory.path(), &mut registry, None, None)?;
    assert_eq!(attempted.plugins_loaded, 0);
    assert_eq!(attempted.plugins_failed, 1);
    assert!(
        attempted.failures[0]
            .1
            .contains("incompatible native computation ABI"),
        "{:?}",
        attempted.failures
    );
    assert!(marker.with_extension("constructor").exists());
    assert!(marker.with_extension("metadata").exists());
    assert!(!marker.with_extension("native").exists());
    assert!(!marker.with_extension("legacy").exists());
    Ok(())
}

#[test]
fn native_config_preserves_component_definitions_and_rejects_unknown_fields() -> Result<()> {
    let config = definition(Path::new("capture.jsonl"))?;
    let server: DrasiServerConfig = serde_json::from_value(json!({"computation":config}))?;
    server.validate()?;
    let yaml = serde_yaml::to_string(&server)?;
    let restored: DrasiServerConfig = serde_yaml::from_str(&yaml)?;
    assert_eq!(
        serde_json::to_value(&restored.computation)?,
        serde_json::to_value(&server.computation)?
    );
    for invalid in [
        json!({"executionMode":"computationGraph"}),
        json!({"instances":[{"id":"test","executionMode":"computationGraph"}]}),
        json!({"computationGraph":[]}),
        json!({"computation":{"definition":config.definition,"typo":true}}),
    ] {
        assert!(serde_json::from_value::<DrasiServerConfig>(invalid).is_err());
    }
    let invalid: DrasiServerConfig =
        serde_json::from_value(json!({"computation":config, "instances":[{"id":"test"}]}))?;
    assert!(invalid.validate().is_err());
    let ordinary: DrasiServerConfig =
        serde_json::from_value(json!({"sources":[{"kind":"mock","id":"legacy"}]}))?;
    assert_eq!(ordinary.sources[0].kind, "mock");
    assert!(ordinary.computation.is_none());
    Ok(())
}

#[test]
fn plugin_aware_validation_checks_native_factory_availability() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let artifact = artifact("drasi_computation_standard");
    std::fs::copy(
        &artifact,
        directory.path().join(artifact.file_name().unwrap()),
    )?;
    let graph = definition(Path::new("capture.jsonl"))?;
    let mut config = DrasiServerConfig {
        computation: Some(graph),
        ..Default::default()
    };
    let valid = drasi_server::config::validate_with_plugins(&config, Some(directory.path()));
    assert_eq!(valid.plugins_loaded, 1);
    assert!(valid.config_errors.is_empty(), "{:?}", valid.config_errors);
    if let ComponentConstruction::Factory(spec) =
        &mut config.computation.as_mut().unwrap().definition.components[0].construction
    {
        spec.implementation.name = "missing-native-factory".into();
    }
    let invalid = drasi_server::config::validate_with_plugins(&config, Some(directory.path()));
    assert!(invalid.has_errors());
    assert_eq!(invalid.config_errors[0].component_type, "computation");
    assert!(invalid.config_errors[0].errors[0]
        .message
        .contains("missing-native-factory"));
    Ok(())
}
