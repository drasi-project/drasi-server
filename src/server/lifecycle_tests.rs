// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Regression contracts for the owner, not Server startup/API integration.

use super::{
    lifecycle::ShutdownBudget, CleanupPhase, ServerCleanupError, ServerCleanupHandle,
    ServerPreparation,
};
use crate::{persistence::configuration_store::YamlConfigurationStore, TelemetryBindings};
use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::{
    channels::{ComponentStatus, SubscriptionResponse},
    computation::v1::{
        ComponentBatch, GraphError, ResourceCleanup, ResourceHandle, ResourceId, ResourceOwnership,
        ResourceRole, ResourceSpecification,
    },
    config::SourceSubscriptionSettings,
    context::SourceRuntimeContext,
    error::{DrasiError, OperationFailures},
    management::{
        AcceptanceReceipt, AcceptedComponentCreation, AcceptedComponentUpdate,
        CommittedConfiguration, ComponentUpdateCompletion, ConfigurationSession,
        ConfigurationSnapshotSummary, ConfigurationStore, ConfigurationStoreGuarantees,
        DesiredInstance,
    },
    observability::{
        ComponentLogKey, ComponentLogRuntime, ComponentLogType, LogLevel, LogMessage, LogPolicy,
        OpenTelemetryContextAdapter, TelemetryProfile, TelemetrySettings,
    },
    DrasiLib, DrasiLibBuilder, ShutdownCompletion, Source,
};
use drasi_source_application::{ApplicationSource, ApplicationSourceConfig};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Weak,
    },
    time::Duration,
};
use tokio::{
    sync::{oneshot, Notify, Semaphore},
    time::{timeout, Instant},
};

const WAIT: Duration = Duration::from_secs(5);

#[tokio::test(start_paused = true)]
async fn running_api_deadline_retains_the_actual_serve_worker() -> Result<()> {
    let logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
    let cleanup = ServerCleanupHandle::new(
        TelemetryBindings::new(
            TelemetrySettings::new(TelemetryProfile::Off),
            logging.clone(),
        )
        .with_shutdown_timeout_seconds(300),
    )?;
    let (stop, stopping) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let (entered, entering) = oneshot::channel();
    cleanup.start_api(stop, async move {
        let _ = stopping.await;
        let _ = entered.send(());
        let _ = released.await;
        Ok(())
    })?;
    cleanup.request_shutdown()?;
    entering.await?;
    let wait = cleanup.wait_until(Instant::now() + Duration::from_secs(30));
    tokio::pin!(wait);
    tokio::select! {
        result = &mut wait => panic!("API still owns accepted connections: {result:?}"),
        _ = tokio::time::sleep(Duration::from_secs(6)) => {}
    }
    assert!(cleanup.status().pending_workers > 0);
    tokio::time::advance(Duration::from_secs(25)).await;
    assert!(wait.await.is_err());
    assert!(cleanup.status().deadline_missed);
    assert!(
        !logging.is_closed(),
        "borrowed logging is not closed at deadline"
    );
    release.send(()).expect("retained serve worker");
    assert!(cleanup.wait_for_completion().await.is_err());
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert!(!cleanup.status().retry_required);
    assert!(
        !logging.is_closed(),
        "borrowed logging remains caller-owned after completion"
    );
    tokio::time::resume();
    logging.shutdown(WAIT).await?;
    Ok(())
}

async fn cancelled_real_api_drain_retains_connections(sse: bool) -> Result<()> {
    use axum::{
        response::sse::{Event, Sse},
        routing::get,
        Router,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let cleanup = ServerCleanupHandle::bootstrap();
    let (stop, stopping) = oneshot::channel();
    let gate = Arc::new(Notify::new());
    let gate_for_stream = gate.clone();
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/stream",
            get(move || {
                let gate = gate_for_stream.clone();
                async move {
                    Sse::new(async_stream::stream! {
                        yield Ok::<_, std::convert::Infallible>(Event::default().data("ready"));
                        gate.notified().await;
                    })
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    cleanup.start_api(stop, async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stopping.await;
            })
            .await
    })?;
    let mut connection = tokio::net::TcpStream::connect(addr).await?;
    if sse {
        connection
            .write_all(b"GET /stream HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await?;
        let mut buffer = [0; 1024];
        let read = timeout(WAIT, connection.read(&mut buffer)).await??;
        assert!(String::from_utf8_lossy(&buffer[..read]).contains("200"));
    } else {
        connection
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")
            .await?;
        reqwest::get(format!("http://{addr}/health"))
            .await?
            .error_for_status()?;
    }
    assert!(timeout(Duration::from_millis(20), cleanup.shutdown())
        .await
        .is_err());
    assert!(cleanup.status().pending_workers > 0);
    gate.notify_one();
    drop(connection);
    timeout(WAIT, cleanup.wait_for_completion()).await??;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_http_drain_on_current_thread_joins_real_serve() -> Result<()> {
    cancelled_real_api_drain_retains_connections(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_sse_drain_on_multiple_threads_joins_real_serve() -> Result<()> {
    cancelled_real_api_drain_retains_connections(true).await
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_sse_drain_on_current_thread_joins_real_serve() -> Result<()> {
    cancelled_real_api_drain_retains_connections(true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_http_drain_on_multiple_threads_joins_real_serve() -> Result<()> {
    cancelled_real_api_drain_retains_connections(false).await
}

// Break caught: a normal source-log subscription waits for shared-channel
// closure, forcing healthy graceful Serve to time out or never join.
async fn normal_source_log_sse_shutdown(owned_logging: bool) -> Result<()> {
    use crate::instance_registry::InstanceRegistry;
    use axum::{routing::get, Extension, Router};

    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    if owned_logging {
        cleanup.retain_logging(logging.clone())?;
    }
    let control = Control::new(false, false);
    let core = core(&cleanup, &bindings, "normal-log-sse", control.clone()).await?;
    let component_id = core.component_by_name("input")?.id().to_string();
    let key = ComponentLogKey::new("normal-log-sse", ComponentLogType::Source, &component_id);
    let (_, mut independent_logs) = logging.registry().subscribe_by_key(&key).await;
    let registry = InstanceRegistry::from_map(indexmap::indexmap! {
        "normal-log-sse".into() => core,
    })
    .with_runtime_context(bindings, cleanup.clone());
    let app = Router::new()
        .route(
            "/api/v1/instances/:instanceId/sources/:id/logs/stream",
            get(crate::api::v1::handlers::stream_source_logs),
        )
        .layer(Extension(registry));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopping) = oneshot::channel();
    cleanup.start_api(stop, async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stopping.await;
            })
            .await
    })?;
    let mut response = reqwest::get(format!(
        "http://{address}/api/v1/instances/normal-log-sse/sources/{component_id}/logs/stream"
    ))
    .await?
    .error_for_status()?;
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    assert!(logging.try_log(LogMessage::with_instance(
        LogLevel::Info,
        "normal SSE is responsive",
        "normal-log-sse",
        &component_id,
        ComponentLogType::Source,
    )));
    timeout(WAIT, async {
        let mut bytes = Vec::new();
        loop {
            bytes.extend_from_slice(&response.chunk().await?.context("SSE closed early")?);
            let text = String::from_utf8_lossy(&bytes);
            if let Some(event) = text
                .split_inclusive("\n\n")
                .find(|event| event.ends_with("\n\n") && event.contains("normal SSE is responsive"))
            {
                let data = event
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                    .find(|line| line.contains("normal SSE is responsive"))
                    .context("missing SSE log payload")?;
                let payload: serde_json::Value = serde_json::from_str(data)?;
                assert_eq!(payload["componentId"], component_id);
                assert_eq!(payload["componentType"], "Source");
                assert_eq!(payload["level"], "Info");
                return Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await??;

    // Keep the responsive HTTP subscription alive throughout cleanup.
    timeout(WAIT, cleanup.shutdown()).await??;
    timeout(WAIT, async {
        while response.chunk().await?.is_some() {}
        Ok::<_, reqwest::Error>(())
    })
    .await??;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert!(!cleanup.status().deadline_missed);
    assert!(!cleanup.status().retry_required);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    if owned_logging {
        assert!(logging.is_closed(), "owned capture must join after Serve");
    } else {
        assert!(!logging.is_closed(), "shared capture must stay open");
        assert!(logging.try_log(LogMessage::with_instance(
            LogLevel::Info,
            "independent subscription survives",
            "normal-log-sse",
            &component_id,
            ComponentLogType::Source,
        )));
        timeout(WAIT, async {
            loop {
                if independent_logs.recv().await?.message == "independent subscription survives" {
                    return Ok::<_, anyhow::Error>(());
                }
            }
        })
        .await??;
        logging.shutdown(WAIT).await?;
    }
    Ok(())
}

// Break caught: stopping A terminates B's or a legacy direct router's body,
// rather than only subscriptions bound to A's lifecycle.
async fn source_log_sse_shutdown_is_server_local() -> Result<()> {
    use crate::api::v1::handlers::ResourcePath;
    use crate::instance_registry::InstanceRegistry;
    use axum::{extract::Path, response::IntoResponse, Extension};
    use futures_util::StreamExt;

    let (bindings, logging) = bindings()?;
    let cleanup_a = ServerCleanupHandle::new(bindings.clone())?;
    let cleanup_b = ServerCleanupHandle::new(bindings.clone())?;
    let control_a = Control::new(false, false);
    let core_a = core(&cleanup_a, &bindings, "log-server-a", control_a).await?;
    let core_b = core(
        &cleanup_b,
        &bindings,
        "log-server-b",
        Control::new(false, false),
    )
    .await?;
    let component_a = core_a.component_by_name("input")?.id().to_string();
    let component_b = core_b.component_by_name("input")?.id().to_string();
    let registry_a = InstanceRegistry::from_map(indexmap::indexmap! {
        "log-server-a".into() => core_a,
    })
    .with_runtime_context(bindings.clone(), cleanup_a.clone());
    let registry_b = InstanceRegistry::from_map(indexmap::indexmap! {
        "log-server-b".into() => core_b.clone(),
    })
    .with_runtime_context(bindings, cleanup_b.clone());
    let body_a = crate::api::v1::handlers::stream_source_logs(
        Extension(registry_a),
        Path(ResourcePath {
            instance_id: "log-server-a".into(),
            id: component_a,
        }),
    )
    .await
    .expect("server A source log stream must be available")
    .into_response()
    .into_body()
    .into_data_stream();
    let body_b = crate::api::v1::handlers::stream_source_logs(
        Extension(registry_b),
        Path(ResourcePath {
            instance_id: "log-server-b".into(),
            id: component_b.clone(),
        }),
    )
    .await
    .expect("server B source log stream must be available")
    .into_response()
    .into_body()
    .into_data_stream()
    .boxed();
    let legacy_registry = InstanceRegistry::from_map(indexmap::indexmap! {
        "log-server-b".into() => core_b.clone(),
    });
    let legacy = crate::api::v1::handlers::stream_source_logs(
        Extension(legacy_registry),
        Path(ResourcePath {
            instance_id: "log-server-b".into(),
            id: component_b.clone(),
        }),
    )
    .await
    .expect("legacy source log stream must be available")
    .into_response()
    .into_body()
    .into_data_stream()
    .boxed();
    tokio::pin!(body_a, body_b, legacy);
    cleanup_a.shutdown().await?;
    assert!(timeout(WAIT, body_a.next()).await?.is_none());
    assert!(!logging.is_closed());
    assert!(logging.try_log(LogMessage::with_instance(
        LogLevel::Info,
        "B remains subscribed",
        "log-server-b",
        &component_b,
        ComponentLogType::Source,
    )));
    for body in [&mut body_b, &mut legacy] {
        timeout(WAIT, async {
            loop {
                let bytes = body.next().await.context("unrelated SSE ended")??;
                if String::from_utf8_lossy(&bytes).contains("B remains subscribed") {
                    return Ok::<_, anyhow::Error>(());
                }
            }
        })
        .await??;
    }
    cleanup_b.shutdown().await?;
    assert!(timeout(WAIT, body_b.next()).await?.is_none());
    assert!(timeout(Duration::from_millis(20), async {
        while legacy.next().await.is_some() {}
    })
    .await
    .is_err());
    drop(core_b);
    logging.shutdown(WAIT).await?;
    Ok(())
}

#[path = "task5_tests.rs"]
mod task5;

#[derive(Debug, thiserror::Error)]
#[error("injected preparation failure")]
struct PreparationCause;

#[derive(Debug, thiserror::Error)]
#[error("injected Core resource cleanup failure")]
struct ResourceCause;

struct Control {
    block_stop: bool,
    block_dispose: AtomicBool,
    fail_resource_close: AtomicBool,
    constructors: AtomicUsize,
    stops: AtomicUsize,
    disposals: AtomicUsize,
    resource_closes: AtomicUsize,
    stop_entered: Notify,
    stop_release: Semaphore,
    dispose_entered: Notify,
    dispose_release: Semaphore,
    disposed: Notify,
}

impl Control {
    fn new(block_stop: bool, fail_resource_close: bool) -> Arc<Self> {
        Arc::new(Self {
            block_stop,
            block_dispose: AtomicBool::new(false),
            fail_resource_close: AtomicBool::new(fail_resource_close),
            constructors: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            disposals: AtomicUsize::new(0),
            resource_closes: AtomicUsize::new(0),
            stop_entered: Notify::new(),
            stop_release: Semaphore::new(0),
            dispose_entered: Notify::new(),
            dispose_release: Semaphore::new(0),
            disposed: Notify::new(),
        })
    }
}

// Keep the real application source's dispatch task, context and cleanup. Only
// its stop and disposal boundaries are controlled by the fixture.
struct GatedSource {
    inner: ApplicationSource,
    control: Arc<Control>,
}

#[async_trait]
impl Source for GatedSource {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn type_name(&self) -> &str {
        self.inner.type_name()
    }

    fn properties(&self) -> HashMap<String, serde_json::Value> {
        self.inner.properties()
    }

    fn supports_replay(&self) -> bool {
        self.inner.supports_replay()
    }

    async fn start(&self) -> Result<()> {
        self.inner.start().await
    }

    async fn stop(&self) -> Result<()> {
        self.control.stops.fetch_add(1, Ordering::SeqCst);
        self.control.stop_entered.notify_one();
        if self.control.block_stop {
            self.control.stop_release.acquire().await?.forget();
        }
        self.inner.stop().await
    }

    async fn dispose(&self) -> Result<()> {
        self.control.dispose_entered.notify_one();
        if self.control.block_dispose.load(Ordering::SeqCst) {
            self.control.dispose_release.acquire().await?.forget();
        }
        self.control.disposals.fetch_add(1, Ordering::SeqCst);
        self.inner.dispose().await?;
        self.control.disposed.notify_one();
        Ok(())
    }

    async fn status(&self) -> ComponentStatus {
        self.inner.status().await
    }

    async fn subscribe(
        &self,
        settings: SourceSubscriptionSettings,
    ) -> Result<SubscriptionResponse> {
        self.inner.subscribe(settings).await
    }

    async fn bind_runtime_context(&self, context: SourceRuntimeContext) {
        self.inner.bind_runtime_context(context).await;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// This resource is owned and released by Core itself, after its graph driver
// has stopped. A failure remains registered in Core's resource cleanup list.
struct GatedResource(Arc<Control>);

#[async_trait]
impl ResourceCleanup for GatedResource {
    async fn shutdown(&self) -> Result<()> {
        self.0.resource_closes.fetch_add(1, Ordering::SeqCst);
        if self.0.fail_resource_close.load(Ordering::SeqCst) {
            return Err(ResourceCause.into());
        }
        Ok(())
    }
}

#[derive(Default)]
struct CloseAcknowledgement {
    calls: AtomicUsize,
    acknowledged: AtomicBool,
}

struct PanicAfterCloseStore {
    inner: YamlConfigurationStore,
    probe: Arc<CloseAcknowledgement>,
}

#[async_trait]
impl ConfigurationStore for PanicAfterCloseStore {
    fn guarantees(&self) -> ConfigurationStoreGuarantees {
        self.inner.guarantees()
    }

    async fn open(&self, instance_id: &str) -> Result<Arc<dyn ConfigurationSession>> {
        Ok(Arc::new(PanicAfterCloseSession {
            inner: self.inner.open(instance_id).await?,
            probe: self.probe.clone(),
        }))
    }
}

struct PanicAfterCloseSession {
    inner: Arc<dyn ConfigurationSession>,
    probe: Arc<CloseAcknowledgement>,
}

#[async_trait]
impl ConfigurationSession for PanicAfterCloseSession {
    async fn load(&self) -> Result<CommittedConfiguration> {
        self.inner.load().await
    }

    async fn new_request_id(&self) -> Result<String> {
        self.inner.new_request_id().await
    }

    async fn commit(
        &self,
        expected_revision: u64,
        request_id: &str,
        desired: &DesiredInstance,
        creation: Option<&AcceptedComponentCreation>,
        updates: &[AcceptedComponentUpdate],
    ) -> Result<AcceptanceReceipt> {
        self.inner
            .commit(expected_revision, request_id, desired, creation, updates)
            .await
    }

    async fn complete_updates(
        &self,
        expected_revision: u64,
        updates: &[ComponentUpdateCompletion],
    ) -> Result<CommittedConfiguration> {
        self.inner
            .complete_updates(expected_revision, updates)
            .await
    }

    async fn receipt(&self, request_id: &str) -> Result<Option<AcceptanceReceipt>> {
        self.inner.receipt(request_id).await
    }

    async fn snapshot(&self, name: &str) -> Result<CommittedConfiguration> {
        self.inner.snapshot(name).await
    }

    async fn load_snapshot(&self, name: &str) -> Result<Option<CommittedConfiguration>> {
        self.inner.load_snapshot(name).await
    }

    async fn list_snapshots(
        &self,
        after: Option<&str>,
        limit: std::num::NonZeroUsize,
    ) -> Result<Vec<ConfigurationSnapshotSummary>> {
        self.inner.list_snapshots(after, limit).await
    }

    async fn delete_snapshot(&self, name: &str, expected_revision: u64) -> Result<bool> {
        self.inner.delete_snapshot(name, expected_revision).await
    }

    async fn close(&self) -> Result<()> {
        self.probe.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.close().await?;
        self.probe.acknowledged.store(true, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for PanicAfterCloseSession {
    fn drop(&mut self) {
        // Core sends its real close acknowledgement before its driver drops the
        // session. Storage operations and the owned management task stay real.
        assert!(
            !self.probe.acknowledged.load(Ordering::SeqCst),
            "injected management driver panic after acknowledged store close"
        );
    }
}

fn bindings() -> Result<(TelemetryBindings, ComponentLogRuntime)> {
    let logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
    Ok((
        TelemetryBindings::new(
            TelemetrySettings::new(TelemetryProfile::Off),
            logging.clone(),
        ),
        logging,
    ))
}

fn controlled_core_builder(id: &str, control: Arc<Control>) -> Result<DrasiLibBuilder> {
    let resource = Arc::new(GatedResource(control.clone()));
    let resource_id = ResourceId::try_new("lifecycle-test-resource")?;
    let resources = ComponentBatch::builder()
        .declare_resource(ResourceSpecification {
            id: resource_id.clone(),
            role: ResourceRole::StateStore,
            ownership: ResourceOwnership::Graph,
            binding: "lifecycle-test-resource".into(),
        })?
        .provide_resource(
            resource_id,
            ResourceHandle::new(ResourceRole::StateStore, resource.clone()).with_cleanup(resource),
        )?
        .build()?;
    Ok(DrasiLib::builder()
        .with_id(id)
        .with_components(resources)
        .with_source_factory("input", true, move |id| {
            control.constructors.fetch_add(1, Ordering::SeqCst);
            let (inner, _) = ApplicationSource::new(
                id.as_str(),
                ApplicationSourceConfig {
                    properties: Default::default(),
                    durability: None,
                },
            )?;
            Ok(GatedSource { inner, control })
        }))
}

async fn core(
    owner: &ServerCleanupHandle,
    bindings: &TelemetryBindings,
    id: &str,
    control: Arc<Control>,
) -> Result<Arc<DrasiLib>> {
    let core = Arc::new(
        bindings
            .apply_to(controlled_core_builder(id, control)?)?
            .build()
            .await?,
    );
    owner.retain_core(core.clone())?;
    core.start().await?;
    timeout(WAIT, core.component_by_name("input")?.wait_started()).await??;
    Ok(core)
}

async fn notified(notification: &Notify) -> Result<()> {
    timeout(WAIT, notification.notified()).await?;
    Ok(())
}

async fn released(core: &Weak<DrasiLib>) -> Result<()> {
    timeout(WAIT, async {
        while core.strong_count() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

fn has_cause<E: std::error::Error + Send + Sync + 'static>(error: &anyhow::Error) -> bool {
    has_error_cause::<E>(error.as_ref())
}

fn has_error_cause<E: std::error::Error + Send + Sync + 'static>(
    error: &(dyn std::error::Error + 'static),
) -> bool {
    if error.downcast_ref::<E>().is_some() {
        return true;
    }
    if let Some(DrasiError::Internal(cause)) = error.downcast_ref::<DrasiError>() {
        return has_cause::<E>(cause);
    }
    // Core aggregates retain secondary failures outside the standard primary
    // source chain; inspect those typed collections rather than their messages.
    if let Some(failures) = error.downcast_ref::<OperationFailures>() {
        return failures.failures().iter().any(has_cause::<E>);
    }
    if let Some(graph) = error.downcast_ref::<GraphError>() {
        match graph {
            GraphError::Cleanup { primary, errors } => {
                return primary
                    .as_deref()
                    .is_some_and(|cause| has_error_cause::<E>(cause))
                    || errors.iter().any(|cause| has_error_cause::<E>(cause));
            }
            GraphError::Reported { cause } => return has_error_cause::<E>(cause.as_ref()),
            _ => {}
        }
    }
    error
        .source()
        .is_some_and(|cause| has_error_cause::<E>(cause))
}

fn preparation_and_cleanup_causes(error: &anyhow::Error) -> Result<&ServerCleanupError> {
    let error = error
        .downcast_ref::<ServerCleanupError>()
        .context("missing typed Server cleanup aggregate")?;
    assert!(error
        .primary_cause()
        .is_some_and(has_cause::<PreparationCause>));
    assert!(error
        .cleanup_causes()
        .iter()
        .any(has_cause::<ResourceCause>));
    Ok(error)
}

// Break caught: dropping finish drops Core A or aborts accepted construction
// instead of closing admission and retaining A through rollback.
async fn cancelled_preparation_retains_created_core() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let control = Control::new(true, false);
    let (created, ready) = oneshot::channel();
    let (release, construction) = oneshot::channel();
    let preparation = ServerPreparation::start(cleanup.admit()?, {
        let control = control.clone();
        move |owner| async move {
            let core = core(&owner, &bindings, "cancelled-preparation", control).await?;
            created
                .send(Arc::downgrade(&core))
                .expect("observer present");
            drop(core);
            construction.await?;
            Ok(())
        }
    })?;
    let operation_handle = preparation.cleanup_handle();
    let weak = timeout(WAIT, ready).await??;
    {
        let finish = preparation.finish();
        tokio::pin!(finish);
        assert!(futures::poll!(&mut finish).is_pending());
    }
    assert!(operation_handle.admit().is_err());
    assert!(operation_handle.status().pending_workers > 0);
    assert!(weak.upgrade().is_some());
    assert_eq!(control.disposals.load(Ordering::SeqCst), 0);
    assert!(!logging.is_closed());

    release.send(()).expect("construction remains owned");
    notified(&control.stop_entered).await?;
    assert!(weak.upgrade().is_some());
    assert!(!logging.is_closed());
    control.stop_release.add_permits(1);
    timeout(WAIT, cleanup.wait_until(Instant::now() + WAIT)).await??;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert_eq!(cleanup.status().pending_workers, 0);
    assert_eq!(control.stops.load(Ordering::SeqCst), 1);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    released(&weak).await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: a cancelled/repeated waiter cancels the real Core stop or
// starts a second teardown instead of joining the already accepted one.
async fn cancelled_wait_does_not_cancel_cleanup() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let control = Control::new(true, false);
    let core = core(&cleanup, &bindings, "cancelled-wait", control.clone()).await?;
    let weak = Arc::downgrade(&core);
    drop(core);
    cleanup.request_shutdown()?;
    notified(&control.stop_entered).await?;
    {
        let wait = cleanup.wait_until(Instant::now() + WAIT);
        tokio::pin!(wait);
        assert!(futures::poll!(&mut wait).is_pending());
    }
    cleanup.request_shutdown()?;
    assert_eq!(control.stops.load(Ordering::SeqCst), 1);
    assert!(cleanup.status().pending_workers > 0);
    assert!(!cleanup.status().deadline_missed);
    assert!(!cleanup.status().retry_required);
    assert!(weak.upgrade().is_some());
    assert!(!logging.is_closed());

    control.stop_release.add_permits(1);
    let (first, second) = tokio::join!(
        cleanup.wait_until(Instant::now() + WAIT),
        cleanup.wait_until(Instant::now() + WAIT)
    );
    first?;
    second?;
    cleanup.wait_until(Instant::now() + WAIT).await?;
    assert_eq!(control.stops.load(Ordering::SeqCst), 1);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    released(&weak).await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: an expired wait aborts producer cleanup, advances providers,
// requests an automatic retry, or later erases the missed deadline.
async fn expired_wait_retains_pending_cleanup_and_outcome() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let control = Control::new(true, false);
    let core = core(&cleanup, &bindings, "expired-wait", control.clone()).await?;
    let weak = Arc::downgrade(&core);
    drop(core);
    cleanup.request_shutdown()?;
    notified(&control.stop_entered).await?;
    let error = cleanup
        .wait_until(Instant::now())
        .await
        .expect_err("pending");
    let retained = error
        .downcast_ref::<ServerCleanupError>()
        .context("deadline must retain a typed owner")?
        .cleanup_handle();
    drop(cleanup);
    let status = retained.status();
    assert_eq!(status.phase, CleanupPhase::StoppingProducers);
    assert!(status.deadline_missed);
    assert!(status.pending_workers > 0);
    assert!(!status.retry_required);
    assert!(weak.upgrade().is_some());
    assert_eq!(control.disposals.load(Ordering::SeqCst), 0);
    assert!(!logging.is_closed());

    control.stop_release.add_permits(1);
    let terminal = timeout(WAIT, retained.wait_until(Instant::now() + WAIT))
        .await?
        .expect_err("a late clean join cannot erase the expired wait");
    assert!(terminal.downcast_ref::<ServerCleanupError>().is_some());
    assert_eq!(retained.status().phase, CleanupPhase::Complete);
    assert!(retained.status().deadline_missed);
    assert_eq!(retained.status().pending_workers, 0);
    assert_eq!(control.stops.load(Ordering::SeqCst), 1);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    released(&weak).await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: an error is stringified, failed cleanup is retried by a plain
// wait, or explicit retry re-runs already successful Core/producer phases.
async fn failure_causes_survive_retry() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let successful = Control::new(false, false);
    let failing = Control::new(false, true);
    let (created, ready) = oneshot::channel();
    let preparation = ServerPreparation::start(cleanup.admit()?, {
        let successful = successful.clone();
        let failing = failing.clone();
        move |owner| async move {
            drop(core(&owner, &bindings, "successful-cleanup", successful).await?);
            let b = core(&owner, &bindings, "failed-cleanup", failing).await?;
            let weak = Arc::downgrade(&b);
            drop(b);
            created.send(weak).expect("observer present");
            Err::<(), _>(PreparationCause.into())
        }
    })?;
    let weak = timeout(WAIT, ready).await??;
    let initial = timeout(WAIT, preparation.finish())
        .await?
        .expect_err("failure");
    let retained = preparation_and_cleanup_causes(&initial)?.cleanup_handle();
    assert!(retained.status().retry_required);
    assert_ne!(retained.status().phase, CleanupPhase::Complete);
    assert!(weak.upgrade().is_some());
    assert!(!logging.is_closed());
    let failed_closes = failing.resource_closes.load(Ordering::SeqCst);
    assert!(failed_closes > 0);

    let repeated = retained
        .wait_until(Instant::now() + WAIT)
        .await
        .expect_err("plain wait reports the cached failure, without retrying");
    preparation_and_cleanup_causes(&repeated)?;
    assert_eq!(successful.disposals.load(Ordering::SeqCst), 1);
    assert_eq!(
        failing.resource_closes.load(Ordering::SeqCst),
        failed_closes
    );

    failing.fail_resource_close.store(false, Ordering::SeqCst);
    let retried = timeout(WAIT, retained.retry_until(Instant::now() + WAIT))
        .await?
        .expect_err("completed retry retains the original failure history");
    preparation_and_cleanup_causes(&retried)?;
    assert_eq!(successful.stops.load(Ordering::SeqCst), 1);
    assert_eq!(successful.disposals.load(Ordering::SeqCst), 1);
    assert_eq!(successful.resource_closes.load(Ordering::SeqCst), 1);
    assert_eq!(failing.stops.load(Ordering::SeqCst), 1);
    assert_eq!(failing.disposals.load(Ordering::SeqCst), 1);
    assert_eq!(
        failing.resource_closes.load(Ordering::SeqCst),
        failed_closes + 1
    );
    assert_eq!(retained.status().phase, CleanupPhase::Complete);
    assert_eq!(retained.status().pending_workers, 0);
    assert!(!retained.status().retry_required);
    let cached = retained
        .wait_until(Instant::now() + WAIT)
        .await
        .expect_err("terminal failure is cached");
    preparation_and_cleanup_causes(&cached)?;
    assert_eq!(
        failing.resource_closes.load(Ordering::SeqCst),
        failed_closes + 1
    );
    released(&weak).await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: shutdown ignores an accepted but unpublished constructor, or a
// rejected request can execute a constructor after admission has closed.
async fn shutdown_closes_admission_atomically() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let accepted = cleanup.admit()?;
    cleanup.request_shutdown()?;
    assert!(cleanup.status().pending_workers > 0);
    assert_ne!(cleanup.status().phase, CleanupPhase::Complete);
    let rejected_calls = Arc::new(AtomicUsize::new(0));
    let rejected = cleanup.admit().and_then({
        let calls = rejected_calls.clone();
        move |admission| {
            ServerPreparation::start(admission, move |_| async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    });
    assert!(rejected.is_err());
    assert_eq!(rejected_calls.load(Ordering::SeqCst), 0);

    let control = Control::new(false, false);
    let (created, ready) = oneshot::channel();
    let (release, construction) = oneshot::channel();
    let preparation = ServerPreparation::start(accepted, {
        let control = control.clone();
        move |owner| async move {
            construction.await?;
            let core = core(&owner, &bindings, "accepted-before-shutdown", control).await?;
            let weak = Arc::downgrade(&core);
            drop(core);
            created.send(weak).expect("observer present");
            Ok(())
        }
    })?;
    // Drop without ever polling finish: accepted work is still owned.
    drop(preparation);
    {
        let wait = cleanup.wait_until(Instant::now() + WAIT);
        tokio::pin!(wait);
        assert!(futures::poll!(&mut wait).is_pending());
    }
    assert_eq!(control.constructors.load(Ordering::SeqCst), 0);
    assert_eq!(cleanup.status().phase, CleanupPhase::Quiescing);
    release.send(()).expect("admitted constructor retained");
    let weak = timeout(WAIT, ready).await??;
    timeout(WAIT, cleanup.wait_until(Instant::now() + WAIT)).await??;
    assert_eq!(control.constructors.load(Ordering::SeqCst), 1);
    assert_eq!(control.stops.load(Ordering::SeqCst), 1);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    assert_eq!(cleanup.status().pending_workers, 0);
    released(&weak).await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: a completed constructor leaves its admission in an unconsumed
// result, making cleanup wait for the caller who is waiting for cleanup.
async fn unpublished_preparations_release_admission() -> Result<()> {
    use super::lifecycle::test_support::latest_preparation_finished;

    for blocking in [false, true] {
        for suspended_finish in [false, true] {
            let (bindings, logging) = bindings()?;
            let cleanup = ServerCleanupHandle::new(bindings.clone())?;
            let control = Control::new(false, false);
            let (created, ready) = oneshot::channel();
            let (release, construction) = oneshot::channel();
            let operation = {
                let logging = logging.clone();
                let control = control.clone();
                move |owner: ServerCleanupHandle| async move {
                    owner.retain_logging(logging)?;
                    let core = core(&owner, &bindings, "unpublished-preparation", control).await?;
                    created
                        .send(Arc::downgrade(&core))
                        .expect("observer present");
                    let server = super::DrasiServer::from_prepared_cores(
                        vec![(core.as_ref().clone(), None)],
                        false,
                        false,
                        "127.0.0.1".into(),
                        0,
                        None,
                        owner,
                        true,
                    );
                    drop(core);
                    construction.await?;
                    Ok(server)
                }
            };
            let preparation = if blocking {
                ServerPreparation::start_blocking(cleanup.admit()?, move |owner| {
                    tokio::runtime::Handle::current().block_on(operation(owner))
                })?
            } else {
                ServerPreparation::start(cleanup.admit()?, operation)?
            };
            let weak = timeout(WAIT, ready).await??;
            let complete_constructor = async {
                release.send(()).expect("constructor remains owned");
                timeout(WAIT, async {
                    while !latest_preparation_finished(&cleanup).await {
                        tokio::task::yield_now().await;
                    }
                })
                .await?;
                cleanup.request_shutdown()?;
                timeout(WAIT, cleanup.wait_for_completion()).await??;
                assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
                assert_eq!(cleanup.status().pending_workers, 0);
                assert!(!cleanup.status().deadline_missed);
                assert!(!cleanup.status().retry_required);
                assert!(
                    logging.is_closed(),
                    "capture cleanup is independent of publication"
                );
                assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
                released(&weak).await?;
                Ok::<_, anyhow::Error>(())
            };
            let error = if suspended_finish {
                let finish = preparation.finish();
                tokio::pin!(finish);
                assert!(futures::poll!(&mut finish).is_pending());
                complete_constructor.await?;
                finish
                    .await
                    .err()
                    .context("late Server publication must reject")?
            } else {
                complete_constructor.await?;
                preparation
                    .finish()
                    .await
                    .err()
                    .context("late Server publication must reject")?
            };
            let aggregate = error
                .downcast_ref::<ServerCleanupError>()
                .context("late publication keeps the typed cleanup outcome")?;
            assert!(aggregate.primary_cause().is_some());
            assert_eq!(
                aggregate.cleanup_handle().status().phase,
                CleanupPhase::Complete
            );
        }
    }
    Ok(())
}

// Break caught: dropping the last handle frees/aborts pending Core cleanup, or
// a completed driver's ownership cycle keeps its actual Core alive forever.
async fn dropped_handles_retain_resources_until_join_then_release() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let control = Control::new(true, false);
    let (created, ready) = oneshot::channel();
    let preparation = ServerPreparation::start(cleanup.admit()?, {
        let control = control.clone();
        move |owner| async move {
            let core = core(&owner, &bindings, "last-handle-dropped", control).await?;
            let weak = Arc::downgrade(&core);
            drop(core);
            created.send(weak).expect("observer present");
            Ok(())
        }
    })?;
    let weak = timeout(WAIT, ready).await??;
    drop(preparation);
    notified(&control.stop_entered).await?;
    assert!(weak.upgrade().is_some());
    drop(cleanup);
    tokio::task::yield_now().await;
    assert!(weak.upgrade().is_some());
    assert_eq!(control.disposals.load(Ordering::SeqCst), 0);
    assert!(!logging.is_closed());

    control.stop_release.add_permits(1);
    notified(&control.disposed).await?;
    released(&weak).await?;
    assert_eq!(control.stops.load(Ordering::SeqCst), 1);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: treating every Core error as unfinished cleanup retains a
// completed Core/bindings forever and retries the same cached terminal failure.
async fn completed_core_error_releases_ownership() -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("lifecycle-terminal-error-")
        .tempdir_in(".")?;
    let store = Arc::new(PanicAfterCloseStore {
        inner: YamlConfigurationStore::new(directory.path().join("configuration.yaml"), true)?,
        probe: Arc::new(CloseAcknowledgement::default()),
    });
    let (bindings, logging) = bindings()?;
    let adapter = Arc::new(OpenTelemetryContextAdapter);
    let weak_binding = Arc::downgrade(&adapter);
    let bindings = bindings.with_context_adapter(adapter);
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let instance_id = "completed-core-error";
    let core = Arc::new(
        bindings
            .apply_to(
                DrasiLib::builder()
                    .with_id(instance_id)
                    .with_configuration_store(store.clone()),
            )?
            .build()
            .await?,
    );
    cleanup.retain_core(core.clone())?;
    core.start().await?;
    let weak_core = Arc::downgrade(&core);
    drop(bindings);

    let failure = timeout(WAIT, cleanup.wait_until(Instant::now() + WAIT))
        .await?
        .expect_err("completed cleanup must retain its typed failure");
    let aggregate = failure
        .downcast_ref::<ServerCleanupError>()
        .context("missing retained cleanup failure")?;
    assert!(aggregate
        .cleanup_causes()
        .iter()
        .any(has_cause::<tokio::task::JoinError>));
    let cause_count = aggregate.cleanup_causes().len();

    let outcome = timeout(WAIT, core.shutdown_with_outcome()).await?;
    assert_eq!(outcome.completion, ShutdownCompletion::Complete);
    let core_failure = outcome.result.expect_err("the Core failure stays cached");
    assert!(core_failure
        .downcast_ref::<tokio::task::JoinError>()
        .is_some_and(tokio::task::JoinError::is_panic));
    assert!(store.probe.acknowledged.load(Ordering::SeqCst));
    assert_eq!(store.probe.calls.load(Ordering::SeqCst), 1);
    store.inner.open(instance_id).await?.close().await?;

    let status = cleanup.status();
    assert_eq!(
        status.phase,
        CleanupPhase::Complete,
        "authoritative Core completion must release the producer owner even on error"
    );
    assert_eq!(status.pending_workers, 0);
    assert!(!status.retry_required);
    assert!(!status.deadline_missed);
    assert!(!logging.is_closed());

    let cached = cleanup
        .wait_until(Instant::now() + WAIT)
        .await
        .expect_err("terminal failure history remains sticky");
    let retried = cleanup
        .retry_until(Instant::now() + WAIT)
        .await
        .expect_err("retry must not re-run a completed Core's failed shutdown");
    for error in [&cached, &retried] {
        let aggregate = error
            .downcast_ref::<ServerCleanupError>()
            .context("cached failure lost its type")?;
        assert_eq!(aggregate.cleanup_causes().len(), cause_count);
        assert!(aggregate
            .cleanup_causes()
            .iter()
            .any(has_cause::<tokio::task::JoinError>));
    }
    assert_eq!(store.probe.calls.load(Ordering::SeqCst), 1);

    drop((failure, cached, retried, core_failure));
    drop(cleanup);
    drop(core);
    released(&weak_core).await?;
    timeout(WAIT, async {
        while weak_binding.strong_count() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    drop(store);
    directory.close()?;
    Ok(())
}

// Break caught: failed/cancelled preparation drops its actual owned capture, or
// cleanup closes capture before producers finish, or a cancelled waiter loses
// the capture join. Borrowed-only cases above must continue to leave it open.
async fn owned_capture_survives_rollback_and_cancelled_wait() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    let control = Control::new(true, false);
    let (created, ready) = oneshot::channel();
    let (release, construction) = oneshot::channel();
    let preparation = ServerPreparation::start(cleanup.admit()?, {
        let logging = logging.clone();
        let control = control.clone();
        move |owner| async move {
            owner.retain_logging(logging.clone())?;
            let core = core(&owner, &bindings, "owned-capture", control).await?;
            let id = core.component_by_name("input")?.id().to_string();
            created.send(id).expect("observer present");
            construction.await?;
            Err::<(), _>(PreparationCause.into())
        }
    })?;
    let operation = preparation.cleanup_handle();
    let component_id = timeout(WAIT, ready).await??;
    let key = ComponentLogKey::new("owned-capture", ComponentLogType::Source, &component_id);
    {
        let finish = preparation.finish();
        tokio::pin!(finish);
        assert!(futures::poll!(&mut finish).is_pending());
    }
    assert!(!logging.is_closed());
    assert!(logging.try_log(LogMessage::with_instance(
        LogLevel::Info,
        "capture retained during construction rollback",
        "owned-capture",
        &component_id,
        ComponentLogType::Source,
    )));
    release.send(()).expect("construction remains owned");
    notified(&control.stop_entered).await?;
    {
        let wait = operation.wait_until(Instant::now() + WAIT);
        tokio::pin!(wait);
        assert!(futures::poll!(&mut wait).is_pending());
    }
    assert!(!logging.is_closed(), "producer stop has not completed");
    assert!(logging.try_log(LogMessage::with_instance(
        LogLevel::Info,
        "capture retained after cancelled cleanup wait",
        "owned-capture",
        &component_id,
        ComponentLogType::Source,
    )));
    control.stop_release.add_permits(1);
    let error = timeout(WAIT, operation.wait_until(Instant::now() + WAIT))
        .await?
        .expect_err("the constructor failure remains visible");
    let aggregate = error
        .downcast_ref::<ServerCleanupError>()
        .context("missing cleanup aggregate")?;
    assert!(aggregate
        .primary_cause()
        .is_some_and(has_cause::<PreparationCause>));
    assert_eq!(operation.status().phase, CleanupPhase::Complete);
    assert_eq!(operation.status().pending_workers, 0);
    assert!(logging.is_closed(), "owned capture was not joined");
    assert_eq!(control.stops.load(Ordering::SeqCst), 1);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    let history = logging.registry().get_history_by_key(&key).await;
    for message in [
        "capture retained during construction rollback",
        "capture retained after cancelled cleanup wait",
    ] {
        assert!(history.iter().any(|entry| entry.message == message));
    }
    Ok(())
}

async fn actual_producer_then_legacy_export_then_owned_logging() -> Result<()> {
    use axum::{routing::post, Json, Router};
    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings.clone())?;
    cleanup.retain_logging(logging.clone())?;
    let control = Control::new(true, false);
    let core = Arc::new(
        bindings
            .apply_to(
                controlled_core_builder("actual-shutdown-order", control.clone())?
                    .with_query_factory(crate::query_test_support::factory())
                    .with_query(
                        drasi_lib::Query::cypher("items")
                            .query("MATCH (n:Item) RETURN n")
                            .from_input("input")
                            .enable_bootstrap(false)
                            .auto_start(true)
                            .build()?,
                    ),
            )?
            .build()
            .await?,
    );
    cleanup.retain_core(core.clone())?;
    core.start().await?;
    timeout(WAIT, core.component_by_name("items")?.wait_started()).await??;
    let registry = crate::instance_registry::InstanceRegistry::new();
    registry
        .add("actual-shutdown-order".into(), core)
        .await
        .map_err(anyhow::Error::msg)?;
    let stalled = Arc::new(AtomicBool::new(true));
    let release = Arc::new(Notify::new());
    let entered = Arc::new(Semaphore::new(0));
    let app = Router::new().route(
        "/v1/metrics",
        post({
            let stalled = stalled.clone();
            let release = release.clone();
            let entered = entered.clone();
            move || {
                let stalled = stalled.clone();
                let release = release.clone();
                let entered = entered.clone();
                async move {
                    entered.add_permits(1);
                    while stalled.load(Ordering::Acquire) {
                        let notified = release.notified();
                        if !stalled.load(Ordering::Acquire) {
                            break;
                        }
                        notified.await;
                    }
                    Json(serde_json::json!({}))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}/v1/metrics", listener.local_addr()?);
    let (stop_collector, stopping_collector) = oneshot::channel();
    let collector = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stopping_collector.await;
            })
            .await
    });
    let metrics = Arc::new(tokio::sync::Mutex::new(
        crate::telemetry::MetricsRuntime::default(),
    ));
    cleanup.retain_metrics(metrics.clone())?;
    metrics
        .lock()
        .await
        .start(
            crate::telemetry::MetricsConfig {
                endpoint: crate::api::models::ConfigValue::Static(endpoint),
                export_interval_ms: 100,
                export_timeout_ms: 5_000,
            },
            &registry,
        )
        .await?;
    timeout(WAIT, entered.acquire()).await??.forget();
    cleanup.request_shutdown()?;
    notified(&control.stop_entered).await?;
    assert!(!logging.is_closed());
    assert!(!metrics.lock().await.is_joined().await);
    control.stop_release.add_permits(1);
    timeout(WAIT, async {
        while cleanup.status().phase != CleanupPhase::DrainingCollection {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    assert!(
        !logging.is_closed(),
        "logging must outlive the actual stalled export"
    );
    assert!(cleanup.status().pending_workers > 0);
    stalled.store(false, Ordering::Release);
    release.notify_waiters();
    timeout(WAIT, cleanup.wait_for_completion()).await??;
    assert!(metrics.lock().await.is_joined().await);
    assert!(logging.is_closed(), "owned logging must close last");
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    stop_collector.send(()).expect("collector owner");
    timeout(WAIT, collector).await???;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn actual_shutdown_order_on_current_thread() -> Result<()> {
    actual_producer_then_legacy_export_then_owned_logging().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_shutdown_order_on_multiple_threads() -> Result<()> {
    actual_producer_then_legacy_export_then_owned_logging().await
}

macro_rules! both_flavors {
    ($current:ident, $multi:ident, $case:ident) => {
        #[tokio::test(flavor = "current_thread")]
        async fn $current() -> Result<()> {
            $case().await
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $multi() -> Result<()> {
            $case().await
        }
    };
}

async fn normal_source_log_sse_borrowed_and_owned_shutdown() -> Result<()> {
    normal_source_log_sse_shutdown(false).await?;
    normal_source_log_sse_shutdown(true).await
}

both_flavors!(
    normal_source_log_sse_shutdown_current_thread,
    normal_source_log_sse_shutdown_multi_thread,
    normal_source_log_sse_borrowed_and_owned_shutdown
);
both_flavors!(
    source_log_sse_shutdown_is_server_local_current_thread,
    source_log_sse_shutdown_is_server_local_multi_thread,
    source_log_sse_shutdown_is_server_local
);
both_flavors!(
    unpublished_preparations_release_admission_current_thread,
    unpublished_preparations_release_admission_multi_thread,
    unpublished_preparations_release_admission
);

both_flavors!(
    cancelled_preparation_retains_created_core_current_thread,
    cancelled_preparation_retains_created_core_multi_thread,
    cancelled_preparation_retains_created_core
);
both_flavors!(
    cancelled_wait_does_not_cancel_cleanup_current_thread,
    cancelled_wait_does_not_cancel_cleanup_multi_thread,
    cancelled_wait_does_not_cancel_cleanup
);
both_flavors!(
    expired_wait_retains_pending_cleanup_and_outcome_current_thread,
    expired_wait_retains_pending_cleanup_and_outcome_multi_thread,
    expired_wait_retains_pending_cleanup_and_outcome
);
both_flavors!(
    failure_causes_survive_retry_current_thread,
    failure_causes_survive_retry_multi_thread,
    failure_causes_survive_retry
);
both_flavors!(
    shutdown_closes_admission_atomically_current_thread,
    shutdown_closes_admission_atomically_multi_thread,
    shutdown_closes_admission_atomically
);
both_flavors!(
    dropped_handles_retain_resources_until_join_then_release_current_thread,
    dropped_handles_retain_resources_until_join_then_release_multi_thread,
    dropped_handles_retain_resources_until_join_then_release
);
both_flavors!(
    completed_core_error_releases_ownership_current_thread,
    completed_core_error_releases_ownership_multi_thread,
    completed_core_error_releases_ownership
);
both_flavors!(
    owned_capture_survives_rollback_and_cancelled_wait_current_thread,
    owned_capture_survives_rollback_and_cancelled_wait_multi_thread,
    owned_capture_survives_rollback_and_cancelled_wait
);

async fn management_retention_tracks_concurrency_not_total_requests() -> Result<()> {
    use super::lifecycle::test_support::{latest_preparation_finished, preparation_count};

    let (bindings, logging) = bindings()?;
    let cleanup = ServerCleanupHandle::new(bindings)?;
    let release = Arc::new(Semaphore::new(0));
    let mut pending = Vec::new();
    for _ in 0..8 {
        let release = release.clone();
        pending.push(cleanup.start_management(move || async move {
            release.acquire().await.expect("release").forget();
        })?);
    }
    for index in 0..128 {
        assert_eq!(
            cleanup
                .start_management(move || async move { index })?
                .finish()
                .await?,
            index
        );
        timeout(WAIT, async {
            while !latest_preparation_finished(&cleanup).await {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert_eq!(
            preparation_count(&cleanup),
            9,
            "eight unfinished owners plus the most recent joined-on-next-admission worker"
        );
    }
    assert_eq!(cleanup.status().pending_workers, 8);
    cleanup.request_shutdown()?;
    release.add_permits(8);
    for work in pending {
        work.finish().await?;
    }
    cleanup.wait_until(Instant::now() + WAIT).await?;
    assert_eq!(preparation_count(&cleanup), 0);
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

async fn reaped_management_failures_retain_typed_join_history() -> Result<()> {
    use super::lifecycle::test_support::{
        abort_latest_preparation, latest_preparation_finished, preparation_count,
    };

    for cancelled in [false, true] {
        let (bindings, logging) = bindings()?;
        let cleanup = ServerCleanupHandle::new(bindings)?;
        let work = cleanup.start_management(move || async move {
            if cancelled {
                std::future::pending::<()>().await;
            } else {
                panic!("injected management-worker panic");
            }
        })?;
        if cancelled {
            abort_latest_preparation(&cleanup).await;
        }
        assert!(work.finish().await.is_err());
        timeout(WAIT, async {
            while !latest_preparation_finished(&cleanup).await {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        cleanup.start_management(|| async {})?.finish().await?;
        assert_eq!(preparation_count(&cleanup), 1);

        let error = cleanup
            .wait_until(Instant::now() + WAIT)
            .await
            .expect_err("a reaped join failure remains historical");
        let aggregate = error
            .downcast_ref::<ServerCleanupError>()
            .context("cleanup type")?;
        assert_eq!(aggregate.cleanup_causes().len(), 1);
        let join = aggregate
            .cleanup_causes()
            .iter()
            .flat_map(anyhow::Error::chain)
            .find_map(|cause| cause.downcast_ref::<tokio::task::JoinError>())
            .context("reaping lost the actual JoinError")?;
        assert_eq!(join.is_cancelled(), cancelled);
        assert_eq!(join.is_panic(), !cancelled);
        assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
        assert!(!cleanup.status().retry_required);
        let cached = cleanup
            .wait_until(Instant::now() + WAIT)
            .await
            .expect_err("joining again cannot erase failure history");
        assert_eq!(
            cached
                .downcast_ref::<ServerCleanupError>()
                .context("cached cleanup type")?
                .cleanup_causes()
                .len(),
            1
        );
        assert!(!logging.is_closed());
        logging.shutdown(WAIT).await?;
    }
    Ok(())
}

both_flavors!(
    management_retention_tracks_concurrency_current_thread,
    management_retention_tracks_concurrency_multi_thread,
    management_retention_tracks_concurrency_not_total_requests
);
both_flavors!(
    reaped_management_failures_keep_types_current_thread,
    reaped_management_failures_keep_types_multi_thread,
    reaped_management_failures_retain_typed_join_history
);

#[derive(Debug, thiserror::Error)]
#[error("injected second Core configuration-store open failure")]
struct BuilderOpenCause;

struct BuilderOpenGate {
    entered: Notify,
    release: Semaphore,
    returned: AtomicBool,
}

impl BuilderOpenGate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: Notify::new(),
            release: Semaphore::new(0),
            returned: AtomicBool::new(false),
        })
    }
}

struct FailingBuilderStore(Arc<BuilderOpenGate>);

#[async_trait]
impl ConfigurationStore for FailingBuilderStore {
    fn guarantees(&self) -> ConfigurationStoreGuarantees {
        ConfigurationStoreGuarantees::MEMORY
    }

    async fn open(&self, _instance_id: &str) -> Result<Arc<dyn ConfigurationSession>> {
        self.0.entered.notify_one();
        self.0.release.acquire().await?.forget();
        self.0.returned.store(true, Ordering::SeqCst);
        Err(BuilderOpenCause.into())
    }
}

fn pending_two_core_builder(
    bindings: TelemetryBindings,
    control: Arc<Control>,
    gate: Arc<BuilderOpenGate>,
) -> crate::DrasiServerBuilder {
    crate::DrasiServerBuilder::new()
        .with_id("builder-first")
        .with_source_factory("input", true, move |id| {
            control.constructors.fetch_add(1, Ordering::SeqCst);
            let (inner, _) = ApplicationSource::new(
                id.as_str(),
                ApplicationSourceConfig {
                    properties: Default::default(),
                    durability: None,
                },
            )?;
            Ok(GatedSource { inner, control })
        })
        .add_instance_builder(
            DrasiLib::builder()
                .with_id("builder-second")
                .with_configuration_store(Arc::new(FailingBuilderStore(gate))),
        )
        .with_telemetry_bindings(bindings)
}

async fn constructor_ran(control: &Control) -> Result<()> {
    timeout(WAIT, async {
        while control.constructors.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

// Break caught: build_with_owned_handles returns the second Core's error without
// joining the already-created first Core, or flattens the typed build cause.
// Store open is a real fallible build boundary, not deferred component health.
async fn builder_second_core_failure_rolls_back_first() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let control = Control::new(false, false);
    control.block_dispose.store(true, Ordering::SeqCst);
    let gate = BuilderOpenGate::new();
    let building = tokio::spawn(
        pending_two_core_builder(bindings, control.clone(), gate.clone())
            .build_with_owned_handles(),
    );
    notified(&gate.entered).await?;
    constructor_ran(&control).await?;
    gate.release.add_permits(1);
    notified(&control.dispose_entered).await?;
    assert!(
        !building.is_finished(),
        "returned before first Core cleanup"
    );
    assert_eq!(control.disposals.load(Ordering::SeqCst), 0);
    assert!(!logging.is_closed());
    control.dispose_release.add_permits(1);
    let error = match timeout(WAIT, building).await?? {
        Err(error) => anyhow::Error::new(error),
        Ok(_) => anyhow::bail!("second Core store failure was accepted"),
    };
    assert!(has_cause::<BuilderOpenCause>(&error), "{error:#}");
    assert!(has_cause::<ServerCleanupError>(&error), "{error:#}");
    assert!(gate.returned.load(Ordering::SeqCst));
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: cancelling finish aborts a pending Core build or loses the
// first Core. The cleanup address exists before either async build is awaited.
async fn builder_cancelled_preparation_keeps_pending_construction_owned() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let control = Control::new(false, false);
    control.block_dispose.store(true, Ordering::SeqCst);
    let gate = BuilderOpenGate::new();
    let preparation = pending_two_core_builder(bindings, control.clone(), gate.clone())
        .prepare_with_owned_handles();
    let cleanup = preparation.cleanup_handle();
    assert!(cleanup.status().pending_workers > 0);
    notified(&gate.entered).await?;
    constructor_ran(&control).await?;
    {
        let finish = preparation.finish();
        tokio::pin!(finish);
        assert!(futures::poll!(&mut finish).is_pending());
    }
    assert_ne!(cleanup.status().phase, CleanupPhase::Complete);
    assert!(cleanup.status().pending_workers > 0);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 0);
    assert!(!gate.returned.load(Ordering::SeqCst));
    assert!(!logging.is_closed());
    tokio::spawn(async {}).await?;

    gate.release.add_permits(1);
    notified(&control.dispose_entered).await?;
    assert_eq!(control.disposals.load(Ordering::SeqCst), 0);
    assert!(!logging.is_closed());
    control.dispose_release.add_permits(1);
    let error = cleanup
        .wait_until(Instant::now() + WAIT)
        .await
        .expect_err("the late store-open failure remains visible");
    assert!(has_cause::<BuilderOpenCause>(&error), "{error:#}");
    assert!(gate.returned.load(Ordering::SeqCst));
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert_eq!(cleanup.status().pending_workers, 0);
    assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: a checked wrapper either patches mismatched prebuilt policy or
// drops supplied Cores on validation error without shutting down all of them.
async fn builder_checked_prebuilt_failure_cleans_every_supplied_core() -> Result<()> {
    let (bindings, logging) = bindings()?;
    let other_logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
    let first = Control::new(false, false);
    let second = Control::new(false, false);
    let first_core = bindings
        .apply_to(controlled_core_builder("checked-first", first.clone())?)?
        .build()
        .await?;
    let second_core = TelemetryBindings::new(
        TelemetrySettings::new(TelemetryProfile::Off),
        other_logging.clone(),
    )
    .apply_to(controlled_core_builder("checked-second", second.clone())?)?
    .build()
    .await?;
    for core in [&first_core, &second_core] {
        core.start().await?;
        timeout(WAIT, core.component_by_name("input")?.wait_started()).await??;
    }
    let retained = [first_core.clone(), second_core.clone()];
    let result = super::DrasiServer::from_cores_with_telemetry(
        vec![(first_core, None), (second_core, None)],
        false,
        false,
        "127.0.0.1".into(),
        8080,
        None,
        bindings,
    )
    .await;
    let error = match result {
        Err(error) => error,
        Ok(_) => anyhow::bail!("checked wrapper replaced mismatched prebuilt logging"),
    };
    assert!(
        format!("{error:#}").contains("logging registry"),
        "{error:#}"
    );
    for control in [&first, &second] {
        assert_eq!(control.constructors.load(Ordering::SeqCst), 1);
        assert_eq!(control.disposals.load(Ordering::SeqCst), 1);
        assert_eq!(control.resource_closes.load(Ordering::SeqCst), 1);
    }
    assert!(Arc::ptr_eq(
        &retained[0].log_registry(),
        &logging.registry()
    ));
    assert!(Arc::ptr_eq(
        &retained[1].log_registry(),
        &other_logging.registry()
    ));
    assert!(!logging.is_closed());
    assert!(!other_logging.is_closed());
    logging.shutdown(WAIT).await?;
    other_logging.shutdown(WAIT).await?;
    Ok(())
}

both_flavors!(
    builder_second_core_failure_rolls_back_first_current_thread,
    builder_second_core_failure_rolls_back_first_multi_thread,
    builder_second_core_failure_rolls_back_first
);
both_flavors!(
    builder_cancelled_preparation_keeps_pending_construction_owned_current_thread,
    builder_cancelled_preparation_keeps_pending_construction_owned_multi_thread,
    builder_cancelled_preparation_keeps_pending_construction_owned
);
both_flavors!(
    builder_checked_prebuilt_failure_cleans_every_supplied_core_current_thread,
    builder_checked_prebuilt_failure_cleans_every_supplied_core_multi_thread,
    builder_checked_prebuilt_failure_cleans_every_supplied_core
);

// Break caught: collection/provider phases each get a fresh timeout, rather
// than sharing the single second left in the original overall budget.
#[tokio::test(start_paused = true)]
async fn collection_uses_remaining_budget() {
    let overall_deadline = Instant::now() + Duration::from_secs(30);
    let mut budget = ShutdownBudget::new(overall_deadline, Duration::from_secs(5));
    let api_deadline = budget.api_deadline();
    assert_eq!(
        budget.api_deadline() - Instant::now(),
        Duration::from_secs(5)
    );
    tokio::time::advance(Duration::from_secs(29)).await;
    let collection_deadline = budget.begin_collection();
    assert_eq!(collection_deadline, overall_deadline);
    assert_eq!(collection_deadline - Instant::now(), Duration::from_secs(1));
    assert_eq!(budget.api_deadline(), api_deadline);
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(budget.begin_collection(), collection_deadline);
    assert!(budget.begin_collection() < Instant::now());
}

// Break caught: the telemetry allowance is ignored when the overall budget is
// longer, or provider flush obtains a new allowance after collection draining.
#[tokio::test(start_paused = true)]
async fn collection_and_flush_share_one_telemetry_allowance() {
    let start = Instant::now();
    let mut budget = ShutdownBudget::new(start + Duration::from_secs(30), Duration::from_secs(5));
    let collection_deadline = budget.begin_collection();
    assert_eq!(collection_deadline, start + Duration::from_secs(5));
    tokio::time::advance(Duration::from_secs(4)).await;
    assert_eq!(budget.begin_collection(), collection_deadline);
    assert_eq!(
        budget.begin_collection() - Instant::now(),
        Duration::from_secs(1)
    );
}
