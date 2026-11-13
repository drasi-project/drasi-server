// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Owned construction and cancellation-safe cleanup, independent of API serving.
//!
//! Accepted construction has an admission guard and an owned worker before any
//! constructor is polled. Constructors must register each created Core before
//! their next suspension. A successful result remains admitted until `finish`
//! publishes it; abandoning that waiter requests rollback instead of aborting
//! its worker. Admission and shutdown are serialized by the same short lock.
//!
//! Cleanup retains actual Cores, construction joins and borrowed bindings.
//! Neither a cancelled wait nor its deadline aborts that driver. An incomplete
//! owner pins itself until confirmed cleanup, including when all public handles
//! disappear. Complete drivers release that pin and never return ownership in
//! their task output, so finished task slots cannot form ownership cycles.
//! Core retirement follows its authoritative shutdown completion independently
//! of the typed result; a completed Core can still report a historical failure.
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
    observability::ComponentLogRuntime,
    DrasiLib, ShutdownCompletion,
};
use tokio::{
    sync::{oneshot, watch, Mutex as AsyncMutex, RwLock},
    task::JoinHandle,
    time::{timeout_at, Instant},
};

use crate::{config::telemetry::DEFAULT_SHUTDOWN_TIMEOUT_SECONDS, TelemetryBindings};

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
        let abandoned = self.abandoned.lock().unwrap_or_else(|error| error.into_inner());
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
            *self.request.abandoned.lock().unwrap_or_else(|error| error.into_inner()) = true;
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
/// `pending_workers` includes accepted, unpublished construction. A failed
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
            // The admitted count fences construction and publication. Keep the
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

        let logging = {
            let mut state = self.state();
            if state.cores.iter().any(Option::is_some) {
                None
            } else if let Some(logging) = state.owned_logging.clone() {
                state.phase = CleanupPhase::DrainingCollection;
                let timeout = state.bindings.as_ref().map_or(
                    Duration::from_secs(DEFAULT_SHUTDOWN_TIMEOUT_SECONDS),
                    TelemetryBindings::shutdown_timeout,
                );
                let overall = state
                    .rollback_deadline
                    .unwrap_or_else(|| Instant::now() + ROLLBACK_WAIT);
                let deadline = *state
                    .logging_deadline
                    .get_or_insert_with(|| overall.min(Instant::now() + timeout));
                Some((logging, deadline))
            } else {
                None
            }
        };
        if let Some((logging, deadline)) = logging {
            self.changed();
            match logging
                .shutdown(deadline.saturating_duration_since(Instant::now()))
                .await
            {
                Ok(()) => {
                    let released = self.state().owned_logging.take();
                    drop(released);
                }
                Err(error) => self.state().history.cleanup.push(Arc::new(error.into())),
            }
        }

        let (bindings, retained) = {
            let mut state = self.state();
            state.driver_active = false;
            state.retry_required =
                state.cores.iter().any(Option::is_some) || state.owned_logging.is_some();
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
                state.cores.iter_mut().filter_map(|slot| {
                    if slot.as_ref().is_some_and(|owned| Arc::ptr_eq(owned, core)) {
                        slot.take()
                    } else {
                        None
                    }
                }).collect::<Vec<_>>()
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

    pub(crate) fn start_watcher<F>(
        &self,
        admission: AdmissionGuard,
        operation: F,
    ) -> Result<JoinHandle<()>>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let worker = Worker::new();
        self.owner.state().preparations.push(worker.clone());
        try_spawn_owned_worker(&worker.join, async move {
            operation.await;
            drop(admission);
        })?;
        let cleanup = self.clone();
        // Serving monitors this join; aborting its waiter never aborts the
        // owned watcher. Cleanup can join the same retained slot independently.
        Ok(tokio::spawn(async move {
            let mut slot = worker.join.write().await;
            if let Some(task) = slot.as_mut() {
                let result = task.await;
                *slot = None;
                if let Err(error) = result {
                    cleanup.owner.primary(error.into());
                    log::error!("Plugin watcher failed; retained by Server cleanup");
                }
            }
        }))
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
    result: Option<oneshot::Receiver<std::result::Result<(T, AdmissionGuard), ()>>>,
    published: bool,
}

impl<T: Send + 'static> ServerPreparation<T> {
    /// Submit synchronous construction without occupying an executor worker.
    ///
    /// Both submission and blocking joins belong to this owner before work can
    /// start. Admission travels through the actual initializer and unpublished
    /// result, exactly as with async construction.
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
                        let _ = result.send(Ok((value, admission)));
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
                    // The guard remains admitted in the channel until publication
                    // or abandonment; the worker's output never retains its owner.
                    let _ = result.send(Ok((value, admission)));
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
        if let Ok(Ok((value, mut admission))) = result {
            let publish = {
                let mut state = self.cleanup.owner.state();
                if state.admission_open {
                    admission.rollback = false;
                    state.phase = CleanupPhase::Prepared;
                    true
                } else {
                    false
                }
            };
            if publish {
                self.published = true;
                drop(admission);
                return Ok(value);
            }
            self.cleanup.owner.primary(PreparationRolledBack.into());
            drop(value);
            drop(admission);
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
