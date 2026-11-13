// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::*;
use crate::{
    api::{
        shared::error::error_codes,
        v1::{build_plugin_router, routes::build_v1_router},
    },
    instance_registry::InstanceRegistry,
    managed_configuration::{ConfigurationStoreConfig, ConfigurationStores},
    plugin_orchestrator::PluginOrchestrator,
    plugin_registry::PluginRegistry,
    DrasiServer,
};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_host_sdk::lifecycle::PluginLifecycleManager;
use drasi_lib::observability::{ComponentLogBinding, MetricRegistrationContext};
use drasi_plugin_sdk::SourcePluginDescriptor;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde_json::{json, Value};
use std::sync::Mutex;
use tokio::sync::RwLock;
use tower::ServiceExt;

struct Probe {
    control: Arc<Control>,
    gated: AtomicBool,
    entered: Notify,
    release: Semaphore,
    bindings: Mutex<Vec<ComponentLogBinding>>,
}

impl Probe {
    fn new(gated: bool) -> Arc<Self> {
        Arc::new(Self {
            control: Control::new(false, false),
            gated: AtomicBool::new(gated),
            entered: Notify::new(),
            release: Semaphore::new(0),
            bindings: Mutex::new(Vec::new()),
        })
    }
}

struct ReleaseFactory(Arc<Probe>);

impl Drop for ReleaseFactory {
    fn drop(&mut self) {
        self.0.gated.store(false, Ordering::SeqCst);
        self.0.release.add_permits(1);
    }
}

struct Descriptor(Arc<Probe>);

#[async_trait]
impl SourcePluginDescriptor for Descriptor {
    fn kind(&self) -> &str {
        "task5"
    }

    fn config_version(&self) -> &str {
        "1.0.0"
    }

    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.into()
    }

    fn config_schema_name(&self) -> &str {
        "Task5SourceConfig"
    }

    async fn create_source(
        &self,
        id: &str,
        _config: &Value,
        auto_start: bool,
    ) -> Result<Box<dyn Source>> {
        self.0.control.constructors.fetch_add(1, Ordering::SeqCst);
        self.0.entered.notify_one();
        if self.0.gated.load(Ordering::SeqCst) {
            self.0.release.acquire().await?.forget();
        }
        let (inner, _) = ApplicationSource::new(
            id,
            ApplicationSourceConfig {
                properties: Default::default(),
                durability: None,
            },
        )?;
        Ok(Box::new(ProbedSource {
            source: GatedSource {
                inner,
                control: self.0.control.clone(),
            },
            probe: self.0.clone(),
            auto_start,
        }))
    }
}

struct ProbedSource {
    source: GatedSource,
    probe: Arc<Probe>,
    auto_start: bool,
}

#[async_trait]
impl Source for ProbedSource {
    fn id(&self) -> &str {
        self.source.id()
    }

    fn type_name(&self) -> &str {
        "task5"
    }

    fn properties(&self) -> HashMap<String, Value> {
        self.source.properties()
    }

    fn auto_start(&self) -> bool {
        self.auto_start
    }

    fn supports_replay(&self) -> bool {
        self.source.supports_replay()
    }

    async fn start(&self) -> Result<()> {
        self.source.start().await
    }

    async fn stop(&self) -> Result<()> {
        self.source.stop().await
    }

    async fn dispose(&self) -> Result<()> {
        self.source.dispose().await
    }

    async fn status(&self) -> ComponentStatus {
        self.source.status().await
    }

    async fn subscribe(
        &self,
        settings: SourceSubscriptionSettings,
    ) -> Result<SubscriptionResponse> {
        self.source.subscribe(settings).await
    }

    async fn bind_runtime_context(&self, context: SourceRuntimeContext) {
        if let Some(binding) = &context.log_binding {
            binding.registry.log(LogMessage::with_instance(
                LogLevel::Info,
                "task5 destination-bound source",
                context.instance_id(),
                context.source_id(),
                ComponentLogType::Source,
            )).await;
            self.probe.bindings.lock().expect("probe bindings").push(binding.clone());
        }
        self.source.bind_runtime_context(context).await;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn collection() -> Result<(TelemetryBindings, ComponentLogRuntime, PrometheusHandle)> {
    let logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let metrics = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    Ok((
        TelemetryBindings::new(
            TelemetrySettings::new(TelemetryProfile::Basic).with_tracing(false),
            logging.clone(),
        )
        .with_metric_context(metrics),
        logging,
        handle,
    ))
}

fn registry(
    bindings: TelemetryBindings,
    cleanup: ServerCleanupHandle,
) -> InstanceRegistry {
    InstanceRegistry::new().with_runtime_context(bindings, cleanup)
}

fn plugins(probe: Arc<Probe>) -> Arc<RwLock<PluginRegistry>> {
    let mut plugins = PluginRegistry::new();
    crate::register_core_plugins(&mut plugins);
    plugins.register_source(Arc::new(Descriptor(probe)));
    Arc::new(RwLock::new(plugins))
}

fn router(registry: InstanceRegistry, plugins: Arc<RwLock<PluginRegistry>>) -> Router {
    build_v1_router(registry, Arc::new(false), None, plugins, None)
}

fn request(uri: &str, body: Value) -> Result<Request<Body>> {
    Ok(Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?)
}

async fn response(response: axum::response::Response) -> Result<(StatusCode, Value)> {
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

async fn post(app: &Router, uri: &str, body: Value) -> Result<(StatusCode, Value)> {
    response(timeout(WAIT, app.clone().oneshot(request(uri, body)?)).await??).await
}

fn source() -> Value {
    json!({"kind":"task5", "name":"input", "autoStart":false})
}

async fn stored_source(
    directory: &std::path::Path,
    id: &str,
) -> Result<ConfigurationStoreConfig> {
    let path = directory.join("components.yaml");
    let provider = YamlConfigurationStore::new(path.clone(), true)?;
    // Instance startup waits for this source's explicit auto-start. Ordinary
    // source creation tests below keep auto-start disabled and acknowledge only
    // graph admission, so this gate does not change their readiness contract.
    let mut source = source();
    source["autoStart"] = json!(true);
    let source: crate::api::models::SourceConfig = serde_json::from_value(source)?;
    let definition = crate::managed_configuration::ordinary::source_definition(source)?
        .with_assigned_id(drasi_lib::config::ComponentId::generate());
    provider
        .initialize_if_absent_with(id, "task5-initial", move || {
            Ok(DesiredInstance {
                ordinary: vec![definition],
                ..Default::default()
            })
        })
        .await?;
    Ok(ConfigurationStoreConfig::Yaml {
        path,
        initialize_if_missing: false,
    })
}

async fn pending(cleanup: &ServerCleanupHandle) -> Result<()> {
    timeout(WAIT, async {
        while cleanup.status().pending_workers == 0 {
            tokio::task::yield_now().await;
        }
    }).await?;
    Ok(())
}

async fn finish_cleanup(cleanup: &ServerCleanupHandle) -> Result<()> {
    // A rejected publication can retain its historical operation error even
    // after all producers are retired. Completion, not success, is the oracle.
    let _ = timeout(WAIT, cleanup.wait_until(Instant::now() + WAIT)).await?;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert_eq!(cleanup.status().pending_workers, 0);
    assert!(!cleanup.status().retry_required);
    Ok(())
}

fn assert_metrics(handle: &PrometheusHandle, id: &str, component: &str) {
    let rendered = handle.render();
    let label = format!("component_id=\"{component}\"");
    let rows: Vec<_> = rendered.lines()
        .filter(|row| row.starts_with("drasi_component_") && row.contains(&label))
        .collect();
    assert!(!rows.is_empty(), "missing observations for {component}:\n{rendered}");
    assert!(rows.iter().all(|row| row.contains(&format!("instance_id=\"{id}\""))));
}

async fn assert_source_binding(
    core: &DrasiLib,
    id: &str,
    logging: &ComponentLogRuntime,
    metrics: &PrometheusHandle,
    probe: &Probe,
) -> Result<()> {
    let input = core.component_by_name("input")?;
    timeout(WAIT, input.wait_created()).await??;
    assert_eq!(core.telemetry().scope().context("missing instance scope")?.instance_id(), id);
    assert_eq!(core.telemetry().policy().profile(), TelemetryProfile::Basic);
    assert!(!core.telemetry().policy().tracing_enabled());
    assert!(core.telemetry().policy().metrics_enabled());
    assert!(Arc::ptr_eq(&core.log_registry(), &logging.registry()));
    let key = ComponentLogKey::new(id, ComponentLogType::Source, input.id().to_string());
    let bound = probe.bindings.lock().expect("probe bindings").clone();
    assert!(bound.iter().any(|binding| binding.key == key
        && Arc::ptr_eq(&binding.registry, &logging.registry())));
    logging.flush().await?;
    let history = core.subscribe_source_logs(input.id().as_str()).await?.0;
    assert!(history.iter().any(|entry| entry.message == "task5 destination-bound source"));
    assert_metrics(metrics, id, input.id().as_str());
    Ok(())
}

// Break caught: applying context after configure_host/restoration leaves
// deferred ordinary constructors on Core defaults instead of the selected host.
async fn bound_registry_restores_with_selected_logging_and_policy() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-restore-").tempdir_in(".")?;
    let (bindings, logging, metrics) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let probe = Probe::new(false);
    let plugins = plugins(probe.clone());
    let store = stored_source(directory.path(), "restored").await?;
    let mut builder = registry.bind_builder(
        DrasiLib::builder().with_id("restored").with_ordinary_component_resolver(Arc::new(
            crate::managed_configuration::ordinary::ServerOrdinaryComponents {
                plugins: plugins.clone(),
                identity_providers: Default::default(),
                bootstrap_providers: Default::default(),
            },
        )),
    )?;
    builder = ConfigurationStores::configure_host(builder, plugins.clone(), None).await?;
    builder = registry.configuration_stores().configure(
        builder, "restored", false, &store, None, Vec::new(), plugins, None,
    ).await?;
    let core = Arc::new(builder.build().await?);
    cleanup.retain_core(core.clone())?;
    crate::managed_configuration::start_instance(&core).await?;
    assert_source_binding(&core, "restored", &logging, &metrics, &probe).await?;
    finish_cleanup(&cleanup).await?;
    assert_eq!(probe.control.disposals.load(Ordering::SeqCst), 1);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: a post-closure request gets past admission into store/factory work.
async fn closed_api_instance_admission_is_structured_and_runs_no_constructor() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-closed-").tempdir_in(".")?;
    let (bindings, logging, _) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let probe = Probe::new(false);
    let store = stored_source(directory.path(), "closed").await?;
    let app = router(registry.clone(), plugins(probe.clone()));
    cleanup.request_shutdown()?;
    let (status, body) = post(&app, "/instances", json!({
        "id":"closed", "configurationStore":store
    })).await?;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], error_codes::SERVER_SHUTTING_DOWN);
    assert_eq!(probe.control.constructors.load(Ordering::SeqCst), 0);
    assert!(!registry.contains("closed").await);
    finish_cleanup(&cleanup).await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: queued work is admitted only when its worker starts, or a
// Core returned after closure is published without belonging to the owner.
async fn admitted_api_construction_delayed_by_plugin_lock_remains_owned() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-admitted-").tempdir_in(".")?;
    let (bindings, logging, _) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let probe = Probe::new(false);
    let plugins = plugins(probe.clone());
    let store = stored_source(directory.path(), "delayed").await?;
    let locked = plugins.clone().write_owned().await;
    let app = router(registry.clone(), plugins);
    let job = tokio::spawn(app.oneshot(request("/instances", json!({
        "id":"delayed", "configurationStore":store
    }))?));
    pending(&cleanup).await?;
    assert_eq!(probe.control.constructors.load(Ordering::SeqCst), 0);
    cleanup.request_shutdown()?;
    assert!(cleanup.status().pending_workers > 0);
    assert!(!logging.is_closed());
    drop(locked);
    let (status, body) = response(timeout(WAIT, job).await???).await?;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], error_codes::SERVER_SHUTTING_DOWN);
    finish_cleanup(&cleanup).await?;
    assert!(!registry.contains("delayed").await);
    assert_eq!(
        probe.control.disposals.load(Ordering::SeqCst),
        probe.control.constructors.load(Ordering::SeqCst),
        "every late constructed source must be disposed"
    );
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: aborting the HTTP waiter aborts an unpublished Core's factory
// or publishes that abandoned instance instead of rolling back owned work.
async fn cancelled_api_instance_constructor_is_joined_and_never_published() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-cancelled-").tempdir_in(".")?;
    let (bindings, logging, _) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let probe = Probe::new(true);
    let release = ReleaseFactory(probe.clone());
    let store = stored_source(directory.path(), "cancelled-api").await?;
    let app = router(registry.clone(), plugins(probe.clone()));
    let job = tokio::spawn(app.oneshot(request("/instances", json!({
        "id":"cancelled-api", "configurationStore":store
    }))?));
    notified(&probe.entered).await?;
    assert!(!registry.contains("cancelled-api").await);
    job.abort();
    assert!(job.await.expect_err("HTTP waiter aborted").is_cancelled());
    assert!(cleanup.status().pending_workers > 0);
    assert!(!logging.is_closed());
    drop(release);
    notified(&probe.control.disposed).await?;
    assert!(!registry.contains("cancelled-api").await);
    assert_eq!(probe.control.constructors.load(Ordering::SeqCst), 1);
    assert_eq!(probe.control.disposals.load(Ordering::SeqCst), 1);
    finish_cleanup(&cleanup).await?;
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: stopping admission fences instances but not plugin operations.
async fn plugin_management_uses_the_same_closed_admission() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-plugins-").tempdir_in(".")?;
    let (bindings, logging, _) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let orchestrator = Arc::new(PluginOrchestrator::with_plugins_dir(
        Arc::new(PluginLifecycleManager::new(plugins(Probe::new(false)))),
        directory.path().into(),
    ));
    orchestrator.bind_instances(registry.clone()).await?;
    let app = build_plugin_router(orchestrator, registry, Arc::new(false));
    cleanup.request_shutdown()?;
    for (uri, request) in [
        ("/load", json!({"filename":"not-a-built-plugin.dylib"})),
        ("/install", json!({"reference":"source/task5-missing"})),
    ] {
        let (status, body) = post(&app, uri, request).await?;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(body["code"], error_codes::SERVER_SHUTTING_DOWN);
    }
    finish_cleanup(&cleanup).await?;
    logging.shutdown(WAIT).await?;
    Ok(())
}

async fn instance_error_does_not_expose_store_path_in_message() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-error-").tempdir_in(".")?;
    let (bindings, logging, _) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let app = router(registry, plugins(Probe::new(false)));
    let sensitive_path = directory.path().join("private-administrative-store.yaml");
    let (status, body) = post(&app, "/instances", json!({
        "id":"bad-store",
        "configurationStore":{
            "kind":"yaml", "path":sensitive_path, "initializeIfMissing":false
        }
    })).await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], error_codes::INSTANCE_CREATE_FAILED);
    assert!(!body["message"].as_str().context("missing high-level message")?
        .contains("private-administrative-store"));
    assert!(body["details"]["technical_details"].as_str()
        .is_some_and(|details| !details.is_empty()), "{body}");
    finish_cleanup(&cleanup).await?;
    logging.shutdown(WAIT).await?;
    Ok(())
}

fn native_graph() -> Value {
    json!({"graph":{
        "resources":{"results":{"kind":"queryCatalog"}},
        "components":[{
            "name":"outlet", "kind":"drasi/query-results-outlet",
            "autoStart":false, "resources":{"catalog":"results"}, "config":{}
        }]
    }})
}

// Break caught: successful node admission is changed into a wait for factory
// readiness, or the accepted factory escapes destination-Core cleanup.
async fn accepted_source_creation_does_not_promise_readiness() -> Result<()> {
    let (bindings, logging, _) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let probe = Probe::new(true);
    let release = ReleaseFactory(probe.clone());
    let app = router(registry.clone(), plugins(probe.clone()));
    let (status, body) = post(&app, "/instances", json!({"id":"accepted-node"})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(&app, "/instances/accepted-node/sources", source()).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    notified(&probe.entered).await?;
    let core = registry.get("accepted-node").await.context("missing accepted instance")?;
    let input = core.component_by_name("input")?;
    {
        let created = input.wait_created();
        tokio::pin!(created);
        assert!(futures::poll!(&mut created).is_pending(), "factory is still gated");
    }
    cleanup.request_shutdown()?;
    assert!(!logging.is_closed());
    drop(release);
    finish_cleanup(&cleanup).await?;
    assert_eq!(probe.control.constructors.load(Ordering::SeqCst), 1);
    assert_eq!(probe.control.disposals.load(Ordering::SeqCst), 1);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

async fn closed_management_admission_never_invokes_destination_factories() -> Result<()> {
    let (bindings, logging, _) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let probe = Probe::new(false);
    let app = router(registry.clone(), plugins(probe.clone()));
    let (status, body) = post(&app, "/instances", json!({"id":"managed-target"})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    cleanup.request_shutdown()?;
    for (path, request) in [
        ("/sources", source()),
        ("/clone", json!({"sourceInstanceId":"managed-target"})),
        ("/solutions", json!({
            "yaml":"name: Task5\nsources:\n  - kind: task5\n    name: input\n    autoStart: false\n"
        })),
        ("/computation/components", json!({
            "expectedRevision":0, "requestId":"closed-native", "configuration":native_graph()
        })),
    ] {
        let (status, body) = post(
            &app, &format!("/instances/managed-target{path}"), request,
        ).await?;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(body["code"], error_codes::SERVER_SHUTTING_DOWN);
    }
    assert_eq!(probe.control.constructors.load(Ordering::SeqCst), 0);
    finish_cleanup(&cleanup).await?;
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: clone or solution factories use the source/first instance's
// scope or a shadow observer rather than the destination's authoritative Core.
async fn clone_and_solution_sources_keep_destination_identity() -> Result<()> {
    let (bindings, logging, metrics) = collection()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let registry = registry(bindings, cleanup.clone());
    let probe = Probe::new(false);
    let app = router(registry.clone(), plugins(probe.clone()));
    for id in ["origin", "clone-target", "solution-target"] {
        let (status, body) = post(&app, "/instances", json!({
            "id":id, "allowExternalComponentIds":true
        })).await?;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, body) = post(&app, "/instances/origin/sources", source()).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let origin = registry.get("origin").await.context("missing origin")?;
    timeout(WAIT, origin.component_by_name("input")?.wait_created()).await??;
    let (status, body) = post(&app, "/instances/origin/computation/components", json!({
        "expectedRevision":origin.desired_configuration()?.revision,
        "requestId":"task5-native-origin", "configuration":native_graph()
    })).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let original_outlet = origin.component_by_name("outlet")?;
    timeout(WAIT, original_outlet.wait_created()).await??;
    let (status, body) = post(&app, "/instances/clone-target/clone", json!({
        "sourceInstanceId":"origin"
    })).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["success"], true, "{body}");
    let (status, body) = post(&app, "/instances/solution-target/solutions", json!({
        "yaml":"name: Task5\nsources:\n  - kind: task5\n    name: input\n    autoStart: false\n"
    })).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["success"], true, "{body}");
    for id in ["origin", "clone-target", "solution-target"] {
        let core = registry.get(id).await.context("missing destination")?;
        assert_source_binding(&core, id, &logging, &metrics, &probe).await?;
        assert!(!core.component_by_name("input")?.observed()?.started);
    }
    let cloned = registry.get("clone-target").await.context("missing native clone")?;
    let cloned_outlet = cloned.component_by_name("outlet")?;
    timeout(WAIT, cloned_outlet.wait_created()).await??;
    assert_eq!(cloned_outlet.id(), original_outlet.id(), "clone retains imported native UUIDs");
    assert!(!cloned_outlet.observed()?.started);
    let rendered = metrics.render();
    for id in ["origin", "clone-target"] {
        assert!(rendered.lines().any(|row| row.starts_with("drasi_component_")
            && row.contains(&format!("component_id=\"{}\"", cloned_outlet.id()))
            && row.contains(&format!("instance_id=\"{id}\""))), "{rendered}");
    }
    finish_cleanup(&cleanup).await?;
    assert_eq!(probe.control.disposals.load(Ordering::SeqCst), 3);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

async fn configured_native_seed_and_restore_share_bindings() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-native-").tempdir_in(".")?;
    let config_path = directory.path().join("server.yaml");
    let store_path = directory.path().join("components.yaml");
    std::fs::write(&config_path, serde_yaml::to_string(&json!({
        "persistConfig":false, "verifyPlugins":false,
        "instances":[{
            "id":"native-owner",
            "configurationStore":{"kind":"yaml", "path":store_path},
            "computation":{"graph":{
                "resources":{"results":{"kind":"queryCatalog"}},
                "components":[{
                    "name":"outlet", "kind":"drasi/query-results-outlet",
                    "autoStart":false, "resources":{"catalog":"results"}, "config":{}
                }]
            }}
        }]
    }))?)?;
    let (bindings, logging, metrics) = collection()?;
    let mut stored_identity = None;
    for restored in [false, true] {
        let preparation = DrasiServer::prepare_new_with_telemetry(
            config_path.clone(), 0, directory.path().join("plugins"),
            false, false, bindings.clone(),
        );
        let cleanup = preparation.cleanup_handle();
        let server = preparation.finish().await?;
        let core = &server.instances[0].core;
        core.reconcile_desired_state().await?;
        let outlet = core.component_by_name("outlet")?;
        timeout(WAIT, outlet.wait_created()).await??;
        assert_eq!(core.telemetry().scope().context("native scope")?.instance_id(), "native-owner");
        assert!(Arc::ptr_eq(&core.log_registry(), &logging.registry()));
        assert_metrics(&metrics, "native-owner", outlet.id().as_str());
        let accepted = core.desired_configuration()?;
        let identity = (outlet.id().clone(), accepted.revision);
        if restored {
            assert_eq!(stored_identity.as_ref(), Some(&identity));
        } else {
            stored_identity = Some(identity);
        }
        finish_cleanup(&cleanup).await?;
        drop(server);
        assert!(!logging.is_closed());
        if !restored {
            let mut config: Value = serde_yaml::from_str(&std::fs::read_to_string(&config_path)?)?;
            config["instances"][0]["computation"]["graph"]["components"][0]["kind"] =
                json!("obsolete/ignored-seed");
            std::fs::write(&config_path, serde_yaml::to_string(&config)?)?;
        }
    }
    logging.shutdown(WAIT).await?;
    Ok(())
}

// This test reaches Server's actual registry creation, not a manually bound
// router. Aborting serving is only test teardown; Task 6's API drain is not tested.
async fn initially_empty_server_binds_future_api_instances() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-empty-").tempdir_in(".")?;
    let config_path = directory.path().join("server.yaml");
    std::fs::write(&config_path, "host: 127.0.0.1\npersistConfig: false\nverifyPlugins: false\ninstances: []\n")?;
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = reservation.local_addr()?.port();
    let (bindings, logging, metrics) = collection()?;
    let preparation = DrasiServer::prepare_new_with_telemetry(
        config_path, port, directory.path().join("plugins"), false, false, bindings,
    );
    let cleanup = preparation.cleanup_handle();
    let server = preparation.finish().await?;
    assert!(server.instances.is_empty());
    let probe = Probe::new(false);
    server.plugin_registry.write().await.register_source(Arc::new(Descriptor(probe.clone())));
    drop(reservation);
    let serving = tokio::spawn(server.run());
    let mut serving = AbortServing(serving);
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    timeout(WAIT, async {
        loop {
            if client.get(format!("{base}/health")).send().await.is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    }).await?;
    for id in ["api-first", "api-second"] {
        let created = client.post(format!("{base}/api/v1/instances"))
            .json(&json!({"id":id})).send().await?;
        assert_eq!(created.status(), StatusCode::OK);
        let created = client.post(format!("{base}/api/v1/instances/{id}/sources"))
            .json(&source()).send().await?;
        assert_eq!(created.status(), StatusCode::OK);
        timeout(WAIT, async {
            loop {
                let bound = probe.bindings.lock().expect("probe bindings").clone();
                if let Some(binding) = bound.iter().find(|binding| binding.key.instance_id == id) {
                    assert!(Arc::ptr_eq(&binding.registry, &logging.registry()));
                    assert_metrics(&metrics, id, &binding.key.component_id);
                    let response = client.get(format!(
                        "{base}/api/v1/instances/{id}/sources/{}/logs",
                        binding.key.component_id
                    )).send().await?;
                    let status = response.status();
                    let logs: Value = response.json().await?;
                    assert!(logs["data"].as_array().with_context(|| format!(
                        "source log response for {id}/{}: status={status}, body={logs}",
                        binding.key.component_id,
                    ))?.iter()
                        .any(|entry| entry["message"] == "task5 destination-bound source"));
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::task::yield_now().await;
            }
        }).await??;
    }
    serving.0.abort();
    assert!((&mut serving.0).await.expect_err("test serving aborted").is_cancelled());
    finish_cleanup(&cleanup).await?;
    assert_eq!(probe.control.disposals.load(Ordering::SeqCst), 2);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

struct AbortServing(tokio::task::JoinHandle<Result<()>>);

impl Drop for AbortServing {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[test]
fn standalone_handoff_applies_file_policy_without_replacing_capture() -> Result<()> {
    for flavor in ["current_thread", "multi_thread"] {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact", "server::lifecycle_tests::task5::runtime_handoff_child",
                "--ignored", "--nocapture", "--test-threads=1",
            ])
            .env("DRASI_TASK5_HANDOFF_FLAVOR", flavor)
            .env("RUST_LOG", "info")
            .output()?;
        assert!(output.status.success(), "{flavor}:\n{}{}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

#[test]
#[ignore = "isolated standalone owner/policy handoff entry point"]
fn runtime_handoff_child() -> Result<()> {
    let mut builder = match std::env::var("DRASI_TASK5_HANDOFF_FLAVOR")?.as_str() {
        "current_thread" => tokio::runtime::Builder::new_current_thread(),
        "multi_thread" => {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.worker_threads(2);
            builder
        }
        flavor => anyhow::bail!("unexpected handoff runtime flavor {flavor}"),
    };
    builder.enable_all().build()?.block_on(async {
        let directory = tempfile::Builder::new().prefix("task5-handoff-").tempdir_in(".")?;
        let path = directory.path().join("server.yaml");
        std::fs::write(&path, serde_yaml::to_string(&json!({
            "persistConfig":false, "verifyPlugins":false,
            "telemetry":{"profile":"basic", "shutdownTimeoutSeconds":1},
            "instances":[{"id":"runtime-file", "computation":native_graph()}]
        }))?)?;
        let standalone = crate::telemetry::StandaloneTelemetry::prepare("info".into())
            .finish().await?;
        let original_owner = standalone.cleanup_handle();
        let probe = standalone.bindings().apply_to(DrasiLib::builder().with_id("borrower"))?
            .build().await?;
        let logging = probe.component_logging().context("original capture")?.clone();
        probe.shutdown().await?;
        drop(probe);
        let preparation = DrasiServer::prepare_new_with_runtime(
            path, 0, directory.path().join("plugins"), false, false, standalone,
        );
        let server_owner = preparation.cleanup_handle();
        let server = preparation.finish().await?;
        let core = &server.instances[0].core;
        let outlet = core.component_by_name("outlet")?;
        timeout(WAIT, outlet.wait_created()).await??;
        assert_eq!(core.telemetry().policy().profile(), TelemetryProfile::Basic);
        assert!(!core.telemetry().policy().metrics_enabled());
        assert!(!core.telemetry().policy().tracing_enabled());
        assert_eq!(core.telemetry().scope().context("file instance scope")?.instance_id(), "runtime-file");
        assert!(Arc::ptr_eq(&core.log_registry(), &logging.registry()));
        assert!(!logging.is_closed(), "consuming StandaloneTelemetry dropped its actual owner");
        original_owner.wait_until(Instant::now() + WAIT).await?;
        assert_eq!(server_owner.status().phase, CleanupPhase::Complete);
        assert!(logging.is_closed());
        assert_eq!(core.shutdown_with_outcome().await.completion, ShutdownCompletion::Complete);
        drop(server);
        Ok(())
    })
}

macro_rules! both_flavors {
    ($name:ident, $multi:ident, $scenario:ident) => {
        #[tokio::test(flavor = "current_thread")]
        async fn $name() -> Result<()> {
            $scenario().await
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $multi() -> Result<()> {
            $scenario().await
        }
    };
}

both_flavors!(registry_restoration, registry_restoration_multi_thread,
    bound_registry_restores_with_selected_logging_and_policy);
both_flavors!(closed_api_admission, closed_api_admission_multi_thread,
    closed_api_instance_admission_is_structured_and_runs_no_constructor);
both_flavors!(delayed_admitted_api_constructor, delayed_admitted_api_constructor_multi_thread,
    admitted_api_construction_delayed_by_plugin_lock_remains_owned);
both_flavors!(cancelled_unpublished_api_constructor, cancelled_unpublished_api_constructor_multi_thread,
    cancelled_api_instance_constructor_is_joined_and_never_published);
both_flavors!(closed_plugin_admission, closed_plugin_admission_multi_thread,
    plugin_management_uses_the_same_closed_admission);
both_flavors!(safe_instance_failure, safe_instance_failure_multi_thread,
    instance_error_does_not_expose_store_path_in_message);
both_flavors!(accepted_node_not_readiness, accepted_node_not_readiness_multi_thread,
    accepted_source_creation_does_not_promise_readiness);
both_flavors!(closed_management_admission, closed_management_admission_multi_thread,
    closed_management_admission_never_invokes_destination_factories);
both_flavors!(destination_core_identity, destination_core_identity_multi_thread,
    clone_and_solution_sources_keep_destination_identity);
both_flavors!(configured_native_restore, configured_native_restore_multi_thread,
    configured_native_seed_and_restore_share_bindings);
both_flavors!(empty_server_future_instance, empty_server_future_instance_multi_thread,
    initially_empty_server_binds_future_api_instances);
