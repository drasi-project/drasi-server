// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Owned construction and cancellation-safe cleanup, independent of API serving.
//!
//! Accepted construction has an admission guard and an owned worker before any
//! constructor is polled. Constructors must register each created Core before
//! their next suspension. Construction admission ends after result registration;
//! unpublished results never hold cleanup hostage to their waiter. `finish`
//! gates publication against shutdown under the same short lock. Abandoning
//! that waiter requests rollback instead of aborting its worker.
//!
//! Cleanup retains actual Cores, construction joins and borrowed bindings.
//! Neither a cancelled wait nor its deadline aborts that driver. An incomplete
//! owner pins itself until confirmed cleanup, including when all public handles
//! disappear. Complete drivers release that pin and never return ownership in
//! their task output, so finished task slots cannot form ownership cycles.
//! Core retirement follows its authoritative shutdown completion independently
//! of the typed result; a completed Core can still report a historical failure.
//! Normal management admission also joins finished request workers and records
//! their typed failures, so polling does not retain every historical task.
//! Vacant submission slots and unfinished/locked joins remain owned.
//!
//! This module creates no collection providers. Borrowed bindings are kept
//! usable through producer cleanup and are only dropped, never closed. An
//! explicitly registered standalone logging owner is drained and joined after
//! producer completion; errors retain its concrete cleanup slot.

use std::{
    fmt,
    future::Future,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use anyhow::{ensure, Result};
use drasi_lib::{
    context::workers::{
        join_owned_worker_gracefully, spawn_owned_blocking_worker, try_spawn_owned_worker,
        WorkerCleanupError, WorkerCompletion,
    },
    error::OperationFailures,
    observability::{ComponentLogRuntime, LoggingError},
    DrasiLib, ShutdownCompletion,
};
use tokio::{
    sync::{oneshot, watch, Mutex as AsyncMutex, RwLock},
    task::JoinHandle,
    time::{timeout_at, Instant},
};

use crate::{config::telemetry::DEFAULT_SHUTDOWN_TIMEOUT_SECONDS, TelemetryBindings};
use crate::{
    instance_registry::InstanceRegistry, plugin_orchestrator::PluginOrchestrator,
    plugin_registry::PluginRegistry,
};
use drasi_host_sdk::watcher::OwnedPollingWatcher;

const ROLLBACK_WAIT: Duration = Duration::from_secs(30);
const API_DRAIN_CAP: Duration = Duration::from_secs(5);

tokio::task_local! {
    static MANAGEMENT_REQUEST: ManagementRequest;
}

#[derive(Debug, thiserror::Error)]
#[error("Server is shutting down")]
pub(crate) struct ServerShuttingDown;

#[derive(Debug, thiserror::Error)]
#[error("Management request was abandoned before publication")]
pub(crate) struct RequestAbandoned;

/// Cancellation and publication belong to the accepted request, not its waiter.
#[derive(Clone)]
pub(crate) struct ManagementRequest {
    cleanup: ServerCleanupHandle,
    abandoned: Arc<Mutex<bool>>,
}

impl ManagementRequest {
    pub(crate) fn current() -> Option<Self> {
        MANAGEMENT_REQUEST.try_with(Clone::clone).ok()
    }

    pub(crate) fn belongs_to(&self, cleanup: &ServerCleanupHandle) -> bool {
        Arc::ptr_eq(&self.cleanup.owner, &cleanup.owner)
    }

    pub(crate) fn publish<T>(&self, publication: impl FnOnce() -> Result<T>) -> Result<T> {
        let abandoned = self
            .abandoned
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *abandoned {
            return Err(RequestAbandoned.into());
        }
        self.cleanup.publish(publication)
    }
}

pub(crate) struct ManagementWork<T> {
    request: ManagementRequest,
    result: Option<oneshot::Receiver<T>>,
    completed: bool,
}

impl<T> ManagementWork<T> {
    pub(crate) async fn finish(mut self) -> Result<T> {
        let result = self.result.take().expect("one management waiter").await?;
        self.completed = true;
        Ok(result)
    }
}

impl<T> Drop for ManagementWork<T> {
    fn drop(&mut self) {
        if !self.completed {
            *self
                .request
                .abandoned
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = true;
        }
    }
}

/// The current ownership boundary, not a component or observability graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupPhase {
    Preparing,
    Prepared,
    Running,
    Quiescing,
    StoppingProducers,
    DrainingCollection,
    ClosingProviders,
    Complete,
}

/// Completion is independent of historical errors and missed wait deadlines.
///
/// `pending_workers` includes accepted construction still running. A failed
/// phase can require an explicit retry even when no worker is currently running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupStatus {
    pub phase: CleanupPhase,
    pub deadline_missed: bool,
    pub pending_workers: usize,
    pub retry_required: bool,
}

/// A cloneable address for the same retained construction/cleanup owner.
///
/// Keep a handle before awaiting construction if cancellation must be observed.
/// Dropping handles is not awaited shutdown. The host must keep its Tokio
/// runtime alive while this owner reports pending or retry-required cleanup.
#[derive(Clone)]
pub struct ServerCleanupHandle {
    owner: Arc<Owner>,
}

impl fmt::Debug for ServerCleanupHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerCleanupHandle")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

struct Worker {
    join: RwLock<Option<JoinHandle<()>>>,
}

impl Worker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            join: RwLock::new(None),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct History {
    primary: Option<Arc<anyhow::Error>>,
    cleanup: Vec<Arc<anyhow::Error>>,
    expired: Option<Arc<anyhow::Error>>,
}

struct State {
    phase: CleanupPhase,
    admission_open: bool,
    admitted: usize,
    preparations: Vec<Arc<Worker>>,
    cores: Vec<Option<Arc<DrasiLib>>>,
    bindings: Option<TelemetryBindings>,
    owned_logging: Option<ComponentLogRuntime>,
    logging_deadline: Option<Instant>,
    budget: Option<ShutdownBudget>,
    running_registry: Option<InstanceRegistry>,
    plugin_dependencies: Option<(Arc<RwLock<PluginRegistry>>, Arc<PluginOrchestrator>)>,
    watcher: Option<OwnedPollingWatcher>,
    api: Option<Arc<ApiWorker>>,
    metrics: Option<Arc<AsyncMutex<crate::telemetry::MetricsRuntime>>>,
    prometheus: Option<Arc<crate::telemetry::prometheus::PrometheusRuntime>>,
    driver: Option<Arc<Worker>>,
    driver_active: bool,
    retry_required: bool,
    rollback_deadline: Option<Instant>,
    history: History,
    retained: Option<Arc<Owner>>,
}

impl State {
    fn status(&self) -> CleanupStatus {
        CleanupStatus {
            phase: self.phase,
            deadline_missed: self.history.expired.is_some(),
            pending_workers: self.admitted + usize::from(self.driver_active),
            retry_required: self.retry_required,
        }
    }
}

struct Owner {
    state: Mutex<State>,
    changed: watch::Sender<()>,
    retry: AsyncMutex<()>,
}

struct ApiWorker {
    join: RwLock<Option<JoinHandle<std::io::Result<()>>>>,
    stop: Mutex<Option<oneshot::Sender<()>>>,
    completed: Arc<std::sync::atomic::AtomicBool>,
}

impl Owner {
    fn state(&self) -> MutexGuard<'_, State> {
        // No user constructor, async work or resource destructor runs under this
        // lock. Retain ownership even if an internal bookkeeping panic poisoned it.
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn changed(&self) {
        self.changed.send_replace(());
    }

    fn primary(&self, error: anyhow::Error) {
        let mut state = self.state();
        if state.history.primary.is_none() {
            state.history.primary = Some(Arc::new(error));
        } else {
            state.history.cleanup.push(Arc::new(error));
        }
    }

    fn reap_preparations(&self) {
        use futures_util::FutureExt;
        let workers = self.state().preparations.clone();
        let mut joined = Vec::new();
        let mut failures = Vec::new();
        for worker in workers {
            if let Ok(mut slot) = worker.join.try_write() {
                if let Some(task) = slot.as_mut().filter(|task| task.is_finished()) {
                    if let Some(result) = task.now_or_never() {
                        *slot = None;
                        joined.push(worker.clone());
                        if let Err(error) = result {
                            failures.push(Arc::new(WorkerCleanupError::Join(error).into()));
                        }
                    }
                }
            }
        }
        let mut state = self.state();
        state
            .preparations
            .retain(|worker| !joined.iter().any(|done| Arc::ptr_eq(worker, done)));
        state.history.cleanup.extend(failures);
    }

    fn begin_driver(self: &Arc<Self>, state: &mut State) -> Result<()> {
        let worker = Worker::new();
        tokio::runtime::Handle::try_current()?;
        state.driver_active = true;
        state.retry_required = false;
        state.driver = Some(worker.clone());
        let owner = self.clone();
        try_spawn_owned_worker(&worker.join, async move {
            let mut attempt = Attempt {
                owner: owner.clone(),
                finished: false,
            };
            owner.drive().await;
            attempt.finished = true;
        })?;
        Ok(())
    }

    async fn drive(self: &Arc<Self>) {
        let mut changed = self.changed.subscribe();
        loop {
            if self.state().admitted == 0 {
                break;
            }
            if changed.changed().await.is_err() {
                return;
            }
        }

        let preparations = self.state().preparations.clone();
        for worker in preparations {
            // The admitted count fences construction and result registration. Keep the
            // slot until the actual child exit is observed, including on panic.
            let mut slot = worker.join.write().await;
            let result = if let Some(task) = slot.as_mut() {
                let result = task.await;
                *slot = None;
                result
            } else {
                Ok(())
            };
            if let Err(error) = result {
                self.primary(WorkerCleanupError::Join(error).into());
            }
        }
        {
            let mut state = self.state();
            state.preparations.clear();
            state.phase = CleanupPhase::StoppingProducers;
        }
        self.changed();

        let api = self.state().api.clone();
        if let Some(api) = &api {
            let deadline = self
                .state()
                .budget
                .as_ref()
                .expect("shutdown budget")
                .api_deadline();
            let mut slot = api.join.write().await;
            if let Some(join) = slot.as_mut() {
                match timeout_at(deadline, join).await {
                    Ok(result) => {
                        *slot = None;
                        self.record_api_result(result);
                    }
                    Err(_) => {
                        log::error!("Web API did not drain within 5 seconds; accepted connections remain owned");
                        self.state()
                            .history
                            .cleanup
                            .push(Arc::new(ApiDrainExpired { deadline }.into()));
                    }
                }
            }
        }

        let watcher = self.state().watcher.clone();
        if let Some(watcher) = watcher {
            // The admitted event consumer is already joined above. This is the
            // actual polling backend, not an acknowledgement from that consumer.
            watcher.request_stop();
            if let Err(error) = watcher.wait_for_completion().await {
                self.state()
                    .history
                    .cleanup
                    .push(Arc::new(anyhow::Error::new(error)));
            }
            if watcher.is_joined() {
                self.state().watcher.take();
            }
        }

        let count = self.state().cores.len();
        for index in 0..count {
            let core = self.state().cores[index].clone();
            if let Some(core) = core {
                let outcome = core.shutdown_with_outcome().await;
                if let Err(error) = outcome.result {
                    self.state().history.cleanup.push(Arc::new(error.into()));
                }
                if outcome.completion == ShutdownCompletion::Complete {
                    let released = self.state().cores[index].take();
                    drop(released);
                }
            }
        }

        // A graceful Serve future owns its connection tasks. A cap bounds the
        // caller's wait, never drops/aborts Serve and calls its children joined.
        // Log bodies observe this Server's admission closure independently of
        // borrowed logging channels. Core shutdown also retires its own streams.
        if let Some(api) = api {
            let mut slot = api.join.write().await;
            if let Some(join) = slot.as_mut() {
                let result = join.await;
                *slot = None;
                self.record_api_result(result);
            }
            if api.completed.load(std::sync::atomic::Ordering::Acquire) {
                self.state().api.take();
            }
        }

        let producers_complete = {
            let state = self.state();
            !state.cores.iter().any(Option::is_some)
                && state.watcher.is_none()
                && state.api.is_none()
        };
        if producers_complete {
            let metrics = self.state().metrics.clone();
            if let Some(metrics) = metrics {
                let deadline = {
                    let mut state = self.state();
                    state.phase = CleanupPhase::DrainingCollection;
                    state
                        .budget
                        .as_mut()
                        .expect("shutdown budget")
                        .begin_collection()
                };
                self.changed();
                let mut metrics = metrics.lock().await;
                if let Err(error) = metrics.request_shutdown(deadline) {
                    self.state().history.cleanup.push(Arc::new(error));
                }
                if let Err(error) = metrics.wait_shutdown_until(deadline).await {
                    self.state().history.cleanup.push(Arc::new(error));
                }
                if !metrics.is_joined().await {
                    // Only continued physical joining. The accepted worker's
                    // collection/close deadline remains unchanged.
                    if let Err(error) = metrics.wait_for_completion().await {
                        self.state().history.cleanup.push(Arc::new(error));
                    }
                }
                if metrics.is_joined().await {
                    self.state().metrics.take();
                }
            }
            let prometheus = self.state().prometheus.clone();
            if let Some(prometheus) = prometheus {
                let deadline = {
                    let mut state = self.state();
                    state.phase = CleanupPhase::DrainingCollection;
                    state.budget.as_mut().expect("shutdown budget").begin_collection()
                };
                self.changed();
                if let Err(error) = prometheus.request_shutdown(deadline) {
                    self.state().history.cleanup.push(Arc::new(error));
                }
                if let Err(error) = prometheus.wait_shutdown_until(deadline).await {
                    self.state().history.cleanup.push(Arc::new(error));
                }
                if !prometheus.is_joined().await {
                    if let Err(error) = prometheus.wait_for_completion().await {
                        self.state().history.cleanup.push(Arc::new(error));
                    }
                }
                if prometheus.is_joined().await {
                    self.state().prometheus.take();
                }
            }
            let dependencies = {
                let mut state = self.state();
                (
                    state.running_registry.take(),
                    state.plugin_dependencies.take(),
                )
            };
            let disposal = Worker::new();
            self.state().preparations.push(disposal.clone());
            if let Err(error) =
                spawn_owned_blocking_worker(&disposal.join, move || drop(dependencies)).await
            {
                self.state().history.cleanup.push(Arc::new(error.into()));
            } else {
                let mut slot = disposal.join.write().await;
                if let Some(join) = slot.as_mut() {
                    let result = join.await;
                    *slot = None;
                    if let Err(error) = result {
                        self.state()
                            .history
                            .cleanup
                            .push(Arc::new(WorkerCleanupError::Join(error).into()));
                    }
                }
            }
            self.state()
                .preparations
                .retain(|worker| !Arc::ptr_eq(worker, &disposal));
        }

        let logging = {
            let mut state = self.state();
            if state.cores.iter().any(Option::is_some)
                || state.watcher.is_some()
                || state.api.is_some()
                || state.metrics.is_some()
                || state.prometheus.is_some()
            {
                None
            } else if let Some(logging) = state.owned_logging.clone() {
                state.phase = CleanupPhase::DrainingCollection;
                let collection = state
                    .budget
                    .as_mut()
                    .expect("shutdown budget")
                    .begin_collection();
                let deadline = *state.logging_deadline.get_or_insert(collection);
                Some((logging, deadline))
            } else {
                None
            }
        };
        if let Some((logging, deadline)) = logging {
            self.changed();
            if Instant::now() >= deadline {
                self.state()
                    .history
                    .cleanup
                    .push(Arc::new(LoggingDrainExpired.into()));
            }
            let mut wait = deadline.saturating_duration_since(Instant::now());
            let mut pending_reported = false;
            loop {
                match logging.shutdown(wait).await {
                    Ok(()) => {
                        self.state().owned_logging.take();
                        break;
                    }
                    Err(error) => {
                        let pending = matches!(&error, LoggingError::TimedOut(_))
                            || matches!(&error, LoggingError::Cleanup(error)
                                if matches!(error.as_ref(), WorkerCleanupError::TimedOut { .. }));
                        let joined_failure = matches!(&error, LoggingError::WorkerPanic)
                            || matches!(&error, LoggingError::Cleanup(error)
                                if matches!(error.as_ref(), WorkerCleanupError::Join(_)));
                        if !pending || !pending_reported {
                            self.state().history.cleanup.push(Arc::new(error.into()));
                            pending_reported |= pending;
                        }
                        if joined_failure {
                            self.state().owned_logging.take();
                            break;
                        }
                        if !pending {
                            break;
                        }
                        // Capture already closed admission and requested its
                        // sole thread join. This is only continued physical
                        // joining through the public API, never another flush
                        // or a replenished collection/caller phase allowance.
                        wait = ROLLBACK_WAIT;
                    }
                }
            }
        }

        let (bindings, retained) = {
            let mut state = self.state();
            state.driver_active = false;
            state.retry_required = state.cores.iter().any(Option::is_some)
                || state.owned_logging.is_some()
                || state.metrics.is_some()
                || state.prometheus.is_some()
                || state.watcher.is_some()
                || state.api.is_some();
            if state.retry_required {
                (None, None)
            } else {
                state.phase = CleanupPhase::Complete;
                state.cores.clear();
                (state.bindings.take(), state.retained.take())
            }
        };
        // Borrowed handles are released only after all producers have joined.
        // Dropping them does not flush or shut down the application's collection.
        drop(bindings);
        drop(retained);
        self.changed();
    }

    fn record_api_result(
        &self,
        result: std::result::Result<std::io::Result<()>, tokio::task::JoinError>,
    ) {
        let error = match result {
            Ok(Ok(())) => return,
            Ok(Err(error)) => ApiWorkerError::Serve(error),
            Err(error) => ApiWorkerError::Join(error),
        };
        self.state().history.cleanup.push(Arc::new(error.into()));
    }
}

// Retain the owner even if the driver unwinds or the runtime cancels its task.
// A later waiter joins the slot to recover the actual typed JoinError.
struct Attempt {
    owner: Arc<Owner>,
    finished: bool,
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if !self.finished {
            let mut state = self.owner.state();
            state.driver_active = false;
            state.retry_required = true;
            drop(state);
            self.owner.changed();
        }
    }
}

impl ServerCleanupHandle {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new(bindings: TelemetryBindings) -> Result<Self> {
        bindings.validate()?;
        let cleanup = Self::bootstrap();
        cleanup.retain_bindings(bindings)?;
        Ok(cleanup)
    }

    /// Create the operation address before any fallible logging initialization.
    pub(crate) fn bootstrap() -> Self {
        let (changed, _) = watch::channel(());
        Self {
            owner: Arc::new(Owner {
                state: Mutex::new(State {
                    phase: CleanupPhase::Preparing,
                    admission_open: true,
                    admitted: 0,
                    preparations: Vec::new(),
                    cores: Vec::new(),
                    bindings: None,
                    owned_logging: None,
                    logging_deadline: None,
                    budget: None,
                    running_registry: None,
                    plugin_dependencies: None,
                    watcher: None,
                    api: None,
                    metrics: None,
                    prometheus: None,
                    driver: None,
                    driver_active: false,
                    retry_required: false,
                    rollback_deadline: None,
                    history: History::default(),
                    retained: None,
                }),
                changed,
                retry: AsyncMutex::new(()),
            }),
        }
    }

    pub(crate) fn prepared(cores: Vec<Arc<DrasiLib>>) -> Self {
        let cleanup = Self::bootstrap();
        {
            let mut state = cleanup.owner.state();
            state.cores = cores.into_iter().map(Some).collect();
            state.phase = CleanupPhase::Prepared;
        }
        cleanup
    }

    pub(crate) async fn shutdown(&self) -> Result<()> {
        self.request_shutdown()?;
        self.wait_until(self.rollback_deadline()).await
    }

    pub(crate) fn retain_runtime(
        &self,
        registry: InstanceRegistry,
        plugins: Arc<RwLock<PluginRegistry>>,
        orchestrator: Arc<PluginOrchestrator>,
    ) -> Result<()> {
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || state.admitted > 0,
            "Runtime must be registered before cleanup"
        );
        state.running_registry = Some(registry);
        state.plugin_dependencies = Some((plugins, orchestrator));
        Ok(())
    }

    pub(crate) fn retain_watcher(&self, watcher: OwnedPollingWatcher) -> Result<()> {
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || state.admitted > 0,
            "Watcher must be registered before cleanup"
        );
        ensure!(state.watcher.is_none(), "Watcher is already owned");
        state.watcher = Some(watcher.clone());
        let stopping = !state.admission_open;
        drop(state);
        if stopping {
            watcher.request_stop();
        }
        Ok(())
    }

    pub(crate) fn retain_metrics(
        &self,
        metrics: Arc<AsyncMutex<crate::telemetry::MetricsRuntime>>,
    ) -> Result<()> {
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || state.admitted > 0,
            "Metrics must be registered before cleanup"
        );
        ensure!(state.metrics.is_none(), "Metrics are already owned");
        ensure!(state.prometheus.is_none(), "Prometheus metrics are already owned");
        state.metrics = Some(metrics);
        Ok(())
    }

    pub(crate) fn retain_prometheus(
        &self,
        runtime: Arc<crate::telemetry::prometheus::PrometheusRuntime>,
    ) -> Result<()> {
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || state.admitted > 0,
            "Prometheus must be registered before cleanup"
        );
        ensure!(state.metrics.is_none(), "OTLP metrics are already owned");
        ensure!(state.prometheus.is_none(), "Prometheus metrics are already owned");
        state.prometheus = Some(runtime);
        Ok(())
    }

    pub(crate) fn start_api<F>(&self, stop: oneshot::Sender<()>, serve: F) -> Result<()>
    where
        F: Future<Output = std::io::Result<()>> + Send + 'static,
    {
        let api = Arc::new(ApiWorker {
            join: RwLock::new(None),
            stop: Mutex::new(Some(stop)),
            completed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || state.admitted > 0,
            "API must be registered before cleanup"
        );
        ensure!(state.api.is_none(), "API is already owned");
        state.api = Some(api.clone());
        let completed = api.completed.clone();
        try_spawn_owned_worker(&api.join, async move {
            let result = serve.await;
            completed.store(true, std::sync::atomic::Ordering::Release);
            result
        })?;
        if !state.admission_open {
            if let Some(stop) = api.stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = stop.send(());
            }
        }
        Ok(())
    }

    pub(crate) fn start_signal<F>(&self, signal: F) -> Result<()>
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        let worker = Worker::new();
        self.owner.state().preparations.push(worker.clone());
        let cleanup = self.clone();
        try_spawn_owned_worker(&worker.join, async move {
            if let Err(error) = signal.await {
                cleanup.record_failure(error);
            }
            if let Err(error) = cleanup.request_shutdown() {
                cleanup.record_failure(error);
            }
        })?;
        Ok(())
    }

    pub(crate) fn running(&self) -> Result<()> {
        let mut state = self.owner.state();
        if !state.admission_open {
            return Err(ServerShuttingDown.into());
        }
        state.phase = CleanupPhase::Running;
        Ok(())
    }

    pub(crate) fn record_failure(&self, error: anyhow::Error) {
        self.owner.primary(error);
    }

    /// Join accepted cleanup without extending its phase allowances. Returns
    /// promptly for terminal historical errors or an explicit-retry boundary.
    /// This is for a CLI retaining its runtime after a bounded waiter expires.
    pub async fn wait_for_completion(&self) -> Result<()> {
        self.request_shutdown()?;
        let worker = self.owner.state().driver.clone();
        if let Some(worker) = worker {
            let mut slot = worker.join.write().await;
            if let Some(join) = slot.as_mut() {
                let result = join.await;
                *slot = None;
                if let Err(error) = result {
                    self.owner.primary(WorkerCleanupError::Join(error).into());
                    let mut state = self.owner.state();
                    state.driver_active = false;
                    state.retry_required = true;
                }
            }
        }
        let state = self.owner.state();
        let clean = state.phase == CleanupPhase::Complete
            && state.history.primary.is_none()
            && state.history.cleanup.is_empty()
            && state.history.expired.is_none();
        drop(state);
        if clean {
            Ok(())
        } else {
            Err(self.error())
        }
    }
    pub(crate) fn retain_bindings(&self, bindings: TelemetryBindings) -> Result<()> {
        bindings.validate()?;
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || (state.phase == CleanupPhase::Quiescing && state.admitted > 0),
            "Bindings must be registered by admitted construction before producer cleanup"
        );
        ensure!(
            state.bindings.is_none(),
            "Cleanup bindings are already registered"
        );
        state.bindings = Some(bindings);
        Ok(())
    }

    pub(crate) fn bindings(&self) -> Option<TelemetryBindings> {
        self.owner.state().bindings.clone()
    }

    /// Resolve file policy on the original standalone owner, before any Core.
    pub(crate) fn configure_logging_bindings(&self, bindings: TelemetryBindings) -> Result<()> {
        bindings.validate()?;
        let mut state = self.owner.state();
        ensure!(
            state.admitted > 0 && state.cores.is_empty(),
            "Standalone policy must be resolved by admitted construction before Core creation"
        );
        let logging = state.owned_logging.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Configured standalone policy requires its original logging owner")
        })?;
        ensure!(
            Arc::ptr_eq(&logging.registry(), &bindings.logging().registry()),
            "Configured policy cannot replace the original standalone capture"
        );
        state.bindings = Some(bindings);
        Ok(())
    }

    pub(crate) fn publish<T>(&self, publication: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self.owner.state();
        if !state.admission_open {
            return Err(ServerShuttingDown.into());
        }
        publication()
    }

    pub(crate) fn start_management<T, F, Fut>(&self, operation: F) -> Result<ManagementWork<T>>
    where
        T: Send + 'static,
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        self.owner.reap_preparations();
        let admission = self.admit()?;
        let worker = Worker::new();
        self.owner.state().preparations.push(worker.clone());
        let request = ManagementRequest {
            cleanup: self.clone(),
            abandoned: Arc::new(Mutex::new(false)),
        };
        let context = request.clone();
        let (result, receiver) = oneshot::channel();
        try_spawn_owned_worker(&worker.join, async move {
            let value = MANAGEMENT_REQUEST.scope(context, operation()).await;
            let _ = result.send(value);
            drop(admission);
        })?;
        Ok(ManagementWork {
            request,
            result: Some(receiver),
            completed: false,
        })
    }

    pub(crate) async fn retire_core(&self, core: &Arc<DrasiLib>) -> Result<()> {
        let outcome = core.shutdown_with_outcome().await;
        if outcome.completion == ShutdownCompletion::Complete {
            let retired = {
                let mut state = self.owner.state();
                state
                    .cores
                    .iter_mut()
                    .filter_map(|slot| {
                        if slot.as_ref().is_some_and(|owned| Arc::ptr_eq(owned, core)) {
                            slot.take()
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            };
            drop(retired);
        }
        outcome.result.map_err(Into::into)
    }

    pub(crate) fn admission_is_open(&self) -> bool {
        self.owner.state().admission_open
    }

    pub(crate) async fn admission_closed(&self) {
        let mut changed = self.owner.changed.subscribe();
        while self.admission_is_open() {
            if changed.changed().await.is_err() {
                break;
            }
        }
    }

    pub(crate) fn start_watcher<F>(&self, admission: AdmissionGuard, operation: F) -> Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let worker = Worker::new();
        self.owner.state().preparations.push(worker.clone());
        try_spawn_owned_worker(&worker.join, async move {
            operation.await;
            drop(admission);
        })?;
        Ok(())
    }

    /// Take standalone capture ownership before its constructor suspends.
    ///
    /// This is distinct from bindings, which always borrow their logging handle.
    pub(crate) fn retain_logging(&self, logging: ComponentLogRuntime) -> Result<()> {
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || (state.phase == CleanupPhase::Quiescing && state.admitted > 0),
            "Logging ownership must be registered by admitted construction before producer cleanup"
        );
        ensure!(
            state.owned_logging.is_none(),
            "Owned logging is already registered"
        );
        state.owned_logging = Some(logging);
        Ok(())
    }

    /// Close admission and start rollback/cleanup once, without cancelling work.
    ///
    /// Repeated calls join the same attempt; they never retry a failed phase.
    /// Startup rollback's first request anchors its single 30-second wait.
    pub fn request_shutdown(&self) -> Result<()> {
        let failed = {
            let mut state = self.owner.state();
            if !state.admission_open {
                return Ok(());
            }
            state.admission_open = false;
            state.phase = CleanupPhase::Quiescing;
            state.rollback_deadline = Some(Instant::now() + ROLLBACK_WAIT);
            let telemetry = state.bindings.as_ref().map_or(
                Duration::from_secs(DEFAULT_SHUTDOWN_TIMEOUT_SECONDS),
                TelemetryBindings::shutdown_timeout,
            );
            state.budget = Some(ShutdownBudget::new(
                state.rollback_deadline.expect("overall deadline"),
                telemetry,
            ));
            if let Some(api) = &state.api {
                if let Some(stop) = api.stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = stop.send(());
                }
            }
            if let Some(watcher) = &state.watcher {
                watcher.request_stop();
            }
            state.retained = Some(self.owner.clone());
            if let Err(error) = self.owner.begin_driver(&mut state) {
                state.driver_active = false;
                state.retry_required = true;
                state.history.cleanup.push(Arc::new(error));
                true
            } else {
                false
            }
        };
        self.owner.changed();
        if failed {
            Err(self.error())
        } else {
            Ok(())
        }
    }

    pub fn status(&self) -> CleanupStatus {
        self.owner.state().status()
    }

    /// Request shutdown and join the accepted attempt within this caller's wait.
    ///
    /// Cancellation/deadline never aborts a worker or removes an unjoined slot.
    /// Failure history is sticky: `Complete` with an error means no work remains,
    /// not that the operation was successful. Plain waits never retry failures.
    pub async fn wait_until(&self, deadline: Instant) -> Result<()> {
        self.request_shutdown()?;
        loop {
            let worker = self.owner.state().driver.clone();
            if let Some(worker) = worker {
                if !self.join_until(&worker, deadline).await {
                    return Err(self.error());
                }
            }
            let outcome = {
                let state = self.owner.state();
                if state.phase == CleanupPhase::Complete || state.retry_required {
                    Some(
                        state.phase == CleanupPhase::Complete
                            && state.history.primary.is_none()
                            && state.history.cleanup.is_empty()
                            && state.history.expired.is_none(),
                    )
                } else {
                    None
                }
            };
            if let Some(clean) = outcome {
                return if clean { Ok(()) } else { Err(self.error()) };
            }
            if Instant::now() >= deadline {
                self.expired(deadline);
                return Err(self.error());
            }
        }
    }

    /// Explicitly retry one failed, unfinished attempt, then join it.
    ///
    /// A retry while work is still pending only joins that work. Concurrent
    /// retries targeting the same failed attempt cannot start duplicate teardown.
    /// Neither retry nor a later wait replenishes phase budgets or clears history.
    pub async fn retry_until(&self, deadline: Instant) -> Result<()> {
        self.request_shutdown()?;
        let (should_retry, failed) = {
            let state = self.owner.state();
            (
                state.retry_required && !state.driver_active,
                state.driver.clone(),
            )
        };
        if !should_retry {
            return self.wait_until(deadline).await;
        }
        let retry = match timeout_at(deadline, self.owner.retry.lock()).await {
            Ok(retry) => retry,
            Err(_) => {
                self.expired(deadline);
                return Err(self.error());
            }
        };
        if let Some(worker) = &failed {
            if !self.join_until(worker, deadline).await {
                return Err(self.error());
            }
        }
        {
            let mut state = self.owner.state();
            let same_attempt = match (&state.driver, &failed) {
                (Some(current), Some(failed)) => Arc::ptr_eq(current, failed),
                (None, None) => true,
                _ => false,
            };
            if same_attempt && state.retry_required && !state.driver_active {
                if let Err(error) = self.owner.begin_driver(&mut state) {
                    state.driver_active = false;
                    state.retry_required = true;
                    state.history.cleanup.push(Arc::new(error));
                }
            }
        }
        drop(retry);
        self.owner.changed();
        self.wait_until(deadline).await
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn admit(&self) -> Result<AdmissionGuard> {
        let mut state = self.owner.state();
        if !state.admission_open {
            return Err(ServerShuttingDown.into());
        }
        state.admitted += 1;
        Ok(AdmissionGuard {
            cleanup: self.clone(),
            rollback: false,
        })
    }

    /// Register ownership before another constructor suspension. After closure,
    /// only already-admitted work may add resources while quiescing.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn retain_core(&self, core: Arc<DrasiLib>) -> Result<()> {
        let mut state = self.owner.state();
        ensure!(
            state.admission_open || (state.phase == CleanupPhase::Quiescing && state.admitted > 0),
            "Core ownership must be registered by admitted construction before producer cleanup"
        );
        if !state
            .cores
            .iter()
            .flatten()
            .any(|existing| Arc::ptr_eq(existing, &core))
        {
            state.cores.push(Some(core));
        }
        Ok(())
    }

    async fn join_until(&self, worker: &Worker, deadline: Instant) -> bool {
        let mut slot = match timeout_at(deadline, worker.join.write()).await {
            Ok(slot) => slot,
            Err(_) => {
                self.expired(deadline);
                return false;
            }
        };
        match join_owned_worker_gracefully(
            &mut slot,
            deadline.saturating_duration_since(Instant::now()),
        )
        .await
        {
            Ok(WorkerCompletion::Absent | WorkerCompletion::Completed(())) => true,
            Ok(WorkerCompletion::Cancelled) => {
                let mut state = self.owner.state();
                state.history.cleanup.push(Arc::new(DriverCancelled.into()));
                state.driver_active = false;
                state.retry_required = true;
                true
            }
            Err(WorkerCleanupError::TimedOut { .. }) => {
                self.expired(deadline);
                false
            }
            Err(error) => {
                let mut state = self.owner.state();
                state.history.cleanup.push(Arc::new(error.into()));
                state.driver_active = false;
                state.retry_required = true;
                true
            }
        }
    }

    fn expired(&self, deadline: Instant) {
        let mut state = self.owner.state();
        if state.history.expired.is_none() {
            state.history.expired = Some(Arc::new(WaitExpired { deadline }.into()));
        }
    }

    fn error(&self) -> anyhow::Error {
        let state = self.owner.state();
        let history = state.history.clone();
        let cleanup = OperationFailures::new(
            "Server cleanup failures",
            history
                .cleanup
                .iter()
                .map(|cause| anyhow::Error::new(SharedCause(cause.clone())))
                .collect(),
        );
        ServerCleanupError {
            cleanup: self.clone(),
            status: state.status(),
            history,
            causes: cleanup,
        }
        .into()
    }

    pub fn shutdown_deadline(&self) -> Option<Instant> {
        self.owner.state().rollback_deadline
    }

    fn rollback_deadline(&self) -> Instant {
        self.owner
            .state()
            .rollback_deadline
            .unwrap_or_else(|| Instant::now() + ROLLBACK_WAIT)
    }
}

/// One accepted request, including construction not yet submitted/published.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct AdmissionGuard {
    cleanup: ServerCleanupHandle,
    rollback: bool,
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        {
            let mut state = self.cleanup.owner.state();
            state.admitted -= 1;
        }
        if self.rollback {
            if let Err(error) = self.cleanup.request_shutdown() {
                log::error!("Server rollback remains owned but could not be scheduled: {error}");
            }
        }
        self.cleanup.owner.changed();
    }
}

/// A result waiter for accepted construction owned independently of this value.
///
/// Dropping this value or its `finish` future requests rollback. It does not
/// abort the construction worker, release created Cores, or close bindings.
pub struct ServerPreparation<T> {
    cleanup: ServerCleanupHandle,
    result: Option<oneshot::Receiver<std::result::Result<T, ()>>>,
    published: bool,
}

impl<T: Send + 'static> ServerPreparation<T> {
    /// Submit synchronous construction without occupying an executor worker.
    ///
    /// Both submission and blocking joins belong to this owner before work can
    /// start. Admission covers the actual initializer and result registration,
    /// not the caller's publication wait, exactly as with async construction.
    pub(crate) fn start_blocking<F>(mut admission: AdmissionGuard, operation: F) -> Result<Self>
    where
        F: FnOnce(ServerCleanupHandle) -> Result<T> + Send + 'static,
    {
        tokio::runtime::Handle::try_current()?;
        let cleanup = admission.cleanup.clone();
        let submission = Worker::new();
        let initializer = Worker::new();
        cleanup
            .owner
            .state()
            .preparations
            .extend([submission.clone(), initializer.clone()]);
        let (result, receiver) = oneshot::channel();
        let submitter = cleanup.clone();
        admission.rollback = true;
        try_spawn_owned_worker(&submission.join, async move {
            let owner = submitter.clone();
            if let Err(error) = spawn_owned_blocking_worker(&initializer.join, move || {
                match operation(owner.clone()) {
                    Ok(value) => {
                        let _ = result.send(Ok(value));
                        admission.rollback = false;
                        drop(admission);
                    }
                    Err(error) => {
                        owner.owner.primary(error);
                        drop(admission);
                        let _ = result.send(Err(()));
                    }
                }
            })
            .await
            {
                submitter.owner.primary(error.into());
            }
        })?;
        Ok(Self {
            cleanup,
            result: Some(receiver),
            published: false,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn start<F, Fut>(mut admission: AdmissionGuard, operation: F) -> Result<Self>
    where
        F: FnOnce(ServerCleanupHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + Send + 'static,
    {
        tokio::runtime::Handle::try_current()?;
        let cleanup = admission.cleanup.clone();
        let worker = Worker::new();
        cleanup.owner.state().preparations.push(worker.clone());
        let (result, receiver) = oneshot::channel();
        let owner = cleanup.clone();
        admission.rollback = true;
        try_spawn_owned_worker(&worker.join, async move {
            match operation(owner.clone()).await {
                Ok(value) => {
                    // Resources and the worker join are already retained by
                    // the owner; publication remains separately shutdown-gated.
                    let _ = result.send(Ok(value));
                    admission.rollback = false;
                    drop(admission);
                }
                Err(error) => {
                    owner.owner.primary(error);
                    drop(admission);
                    let _ = result.send(Err(()));
                }
            }
        })?;
        Ok(Self {
            cleanup,
            result: Some(receiver),
            published: false,
        })
    }
}

impl<T> ServerPreparation<T> {
    /// Preserve the same operation address if construction cannot be submitted.
    pub(crate) fn failed(cleanup: ServerCleanupHandle, error: anyhow::Error) -> Self {
        cleanup.owner.primary(error);
        let (result, receiver) = oneshot::channel();
        let _ = result.send(Err(()));
        // Scheduling errors are retained in the same owner's failure history.
        let _ = cleanup.request_shutdown();
        Self {
            cleanup,
            result: Some(receiver),
            published: false,
        }
    }

    pub fn cleanup_handle(&self) -> ServerCleanupHandle {
        self.cleanup.clone()
    }

    /// Publish accepted construction, or await its same owned rollback.
    ///
    /// A shutdown racing publication wins under the admission lock. Startup
    /// failure waits at most the original rollback's overall 30-second deadline;
    /// incomplete failure results retain this same cleanup handle.
    pub async fn finish(mut self) -> Result<T> {
        let result = self
            .result
            .take()
            .expect("preparation has one result")
            .await;
        if let Ok(Ok(value)) = result {
            let publish = {
                let mut state = self.cleanup.owner.state();
                if state.admission_open {
                    state.phase = CleanupPhase::Prepared;
                    true
                } else {
                    false
                }
            };
            if publish {
                self.published = true;
                return Ok(value);
            }
            self.cleanup.owner.primary(PreparationRolledBack.into());
            drop(value);
        }
        self.cleanup.request_shutdown()?;
        let outcome = self
            .cleanup
            .wait_until(self.cleanup.rollback_deadline())
            .await;
        self.published = true;
        match outcome {
            Err(error) => Err(error),
            Ok(()) => {
                self.cleanup.owner.primary(PreparationRolledBack.into());
                Err(self.cleanup.error())
            }
        }
    }
}

impl<T> Drop for ServerPreparation<T> {
    fn drop(&mut self) {
        if !self.published {
            if let Err(error) = self.cleanup.request_shutdown() {
                log::error!("Server preparation rollback remains owned: {error}");
            }
        }
    }
}

/// A typed snapshot of failure history plus access to the retained live owner.
///
/// Historical errors can coexist with `cleanup_handle().status().phase ==
/// CleanupPhase::Complete`. Check status to distinguish a nonzero outcome from
/// actual pending/retry-required work; never wait forever merely on an error.
#[derive(Debug)]
pub struct ServerCleanupError {
    cleanup: ServerCleanupHandle,
    status: CleanupStatus,
    history: History,
    causes: OperationFailures,
}

impl ServerCleanupError {
    pub fn cleanup_handle(&self) -> ServerCleanupHandle {
        self.cleanup.clone()
    }

    pub fn primary_cause(&self) -> Option<&anyhow::Error> {
        self.history.primary.as_deref()
    }

    /// All cleanup failures in encounter order, with their original typed
    /// sources retained. Inspect chains/aggregates rather than parsing messages.
    pub fn cleanup_causes(&self) -> &[anyhow::Error] {
        self.causes.failures()
    }
}

impl fmt::Display for ServerCleanupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.status.phase == CleanupPhase::Complete {
            write!(formatter, "Server cleanup completed with a failed outcome")
        } else {
            write!(
                formatter,
                "Server cleanup is incomplete; ownership is retained"
            )
        }
    }
}

impl std::error::Error for ServerCleanupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Some(primary) = &self.history.primary {
            Some(primary.as_ref().as_ref())
        } else if !self.causes.failures().is_empty() {
            Some(&self.causes)
        } else {
            self.history
                .expired
                .as_ref()
                .map(|error| error.as_ref().as_ref())
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn owner_liveness(
        cleanup: &ServerCleanupHandle,
    ) -> impl Fn() -> bool + Send + Sync + 'static {
        let owner = Arc::downgrade(&cleanup.owner);
        move || owner.strong_count() != 0
    }

    pub(crate) fn preparation_count(cleanup: &ServerCleanupHandle) -> usize {
        cleanup.owner.state().preparations.len()
    }

    pub(crate) async fn latest_preparation_finished(cleanup: &ServerCleanupHandle) -> bool {
        let worker = cleanup.owner.state().preparations.last().cloned();
        match worker {
            Some(worker) => worker
                .join
                .read()
                .await
                .as_ref()
                .is_some_and(JoinHandle::is_finished),
            None => false,
        }
    }

    pub(crate) async fn abort_latest_preparation(cleanup: &ServerCleanupHandle) {
        let worker = cleanup
            .owner
            .state()
            .preparations
            .last()
            .cloned()
            .expect("worker");
        worker
            .join
            .read()
            .await
            .as_ref()
            .expect("installed join")
            .abort();
    }

    #[tokio::test]
    async fn maintenance_keeps_a_submission_slot_before_its_join_is_installed() -> Result<()> {
        let cleanup = ServerCleanupHandle::bootstrap();
        let worker = Worker::new();
        cleanup.owner.state().preparations.push(worker.clone());
        cleanup.owner.reap_preparations();
        assert_eq!(preparation_count(&cleanup), 1);
        try_spawn_owned_worker(&worker.join, async {})?;
        while !latest_preparation_finished(&cleanup).await {
            tokio::task::yield_now().await;
        }
        cleanup.owner.reap_preparations();
        assert_eq!(preparation_count(&cleanup), 0);
        cleanup.shutdown().await
    }
}

#[derive(Debug)]
struct SharedCause(Arc<anyhow::Error>);

impl fmt::Display for SharedCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl std::error::Error for SharedCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref().as_ref())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Server cleanup wait deadline {deadline:?} elapsed; cleanup remains owned")]
struct WaitExpired {
    deadline: Instant,
}

#[derive(Debug, thiserror::Error)]
#[error("Server cleanup driver was cancelled before confirmed completion")]
struct DriverCancelled;

#[derive(Debug, thiserror::Error)]
#[error("Web API did not drain within 5 seconds (deadline {deadline:?}); connections remain owned")]
struct ApiDrainExpired {
    deadline: Instant,
}

#[derive(Debug, thiserror::Error)]
#[error("Owned logging drain exceeded the shared collection phase deadline")]
struct LoggingDrainExpired;

#[derive(Debug, thiserror::Error)]
enum ApiWorkerError {
    #[error("Web API serving failed")]
    Serve(#[source] std::io::Error),
    #[error("Web API worker failed")]
    Join(#[source] tokio::task::JoinError),
}

#[derive(Debug, thiserror::Error)]
#[error("Server preparation was rolled back before publication")]
struct PreparationRolledBack;

/// Absolute per-phase deadlines. Entering a phase anchors its allowance once;
/// later waits/retries must reuse it, even after its deadline has elapsed.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ShutdownBudget {
    overall: Instant,
    api: Instant,
    telemetry: Duration,
    collection: Option<Instant>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ShutdownBudget {
    pub(crate) fn new(deadline: Instant, telemetry: Duration) -> Self {
        Self {
            overall: deadline,
            api: deadline.min(Instant::now() + API_DRAIN_CAP),
            telemetry,
            collection: None,
        }
    }

    pub(crate) fn api_deadline(&self) -> Instant {
        self.api
    }

    pub(crate) fn begin_collection(&mut self) -> Instant {
        *self
            .collection
            .get_or_insert_with(|| self.overall.min(Instant::now() + self.telemetry))
    }
}
