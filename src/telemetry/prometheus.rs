// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Bounded scrape admission with physical, cancellation-safe worker ownership.
//!
//! A retained render slot is its permit: it is released only after submission
//! and actual blocking work have both been joined. Waiter cancellation and the
//! five-second HTTP wait never release that permit. No request waits for a slot.
//! One skip-missed-ticks controller joins each off-executor upkeep before the
//! next. Upkeep drains samples without scrapers, not a hard memory/RSS bound.

use super::{metrics_terminal, process::ProcessMetrics, ShutdownResult};
use crate::{
    api::shared::error::{error_codes, ErrorResponse},
    config::telemetry::{validate_path, EffectivePrometheus},
    server::ServerCleanupHandle,
};
use anyhow::{ensure, Result};
use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use drasi_lib::context::workers::{spawn_owned_blocking_worker, try_spawn_owned_worker};
use futures_util::FutureExt;
use metrics_exporter_prometheus::PrometheusHandle;
use std::{
    sync::{Arc, Mutex, MutexGuard, RwLock},
    time::Duration,
};
use tokio::{
    sync::{oneshot, watch, RwLock as AsyncRwLock},
    task::JoinHandle,
    time::{interval_at, timeout, timeout_at, Instant, MissedTickBehavior},
};

struct State {
    accepting: bool,
    renders: Vec<Arc<RenderJob>>,
    failure: Option<Arc<anyhow::Error>>,
    deadline: Option<Instant>,
}

#[derive(Default)]
struct RenderJob {
    submission: AsyncRwLock<Option<JoinHandle<ShutdownResult>>>,
    work: AsyncRwLock<Option<JoinHandle<ShutdownResult>>>,
}

#[derive(Debug, thiserror::Error)]
enum PrometheusError {
    #[error("Prometheus backend access is poisoned")]
    BackendPoisoned,
    #[error("Prometheus worker failed")]
    Join(#[source] tokio::task::JoinError),
    #[error("Prometheus cleanup wait elapsed; actual work remains owned")]
    WaitExpired,
    #[error("Prometheus upkeep shutdown channel closed unexpectedly")]
    RequestClosed,
    #[error("Prometheus render worker exited without a response")]
    ResponseClosed,
}

impl State {
    fn remember(&mut self, result: ShutdownResult) {
        if let Err(error) = result {
            log::error!("Prometheus worker failed: {error:#}");
            self.failure.get_or_insert(error);
        }
    }

    fn reap_renders(&mut self) {
        let jobs = std::mem::take(&mut self.renders);
        for job in jobs {
            let Some(submission) = reap_slot(&job.submission) else {
                self.renders.push(job);
                continue;
            };
            self.remember(submission);
            match reap_slot(&job.work) {
                Some(result) => self.remember(result),
                None => self.renders.push(job),
            }
        }
    }
}

fn reap_slot(slot: &AsyncRwLock<Option<JoinHandle<ShutdownResult>>>) -> Option<ShutdownResult> {
    let mut slot = slot.try_write().ok()?;
    let result = match slot.as_mut() {
        Some(join) if join.is_finished() => join
            .now_or_never()?
            .unwrap_or_else(|error| Err(Arc::new(PrometheusError::Join(error).into()))),
        Some(_) => return None,
        None => return Some(Ok(())),
    };
    *slot = None;
    Some(result)
}

async fn join_slot(slot: &AsyncRwLock<Option<JoinHandle<ShutdownResult>>>) -> ShutdownResult {
    let mut slot = slot.write().await;
    let result = match slot.as_mut() {
        Some(join) => join
            .await
            .unwrap_or_else(|error| Err(Arc::new(PrometheusError::Join(error).into()))),
        None => return Ok(()),
    };
    *slot = None;
    result
}

#[cfg(test)]
#[derive(Default)]
struct UpkeepProbe {
    first_tick: Mutex<Option<Instant>>,
    started: std::sync::atomic::AtomicUsize,
    completed: std::sync::atomic::AtomicUsize,
    active: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
}

/// Routing metadata lending the backend already retained by the cleanup owner.
pub(crate) struct PreparedPrometheus {
    pub(crate) config: EffectivePrometheus,
    runtime: Arc<PrometheusRuntime>,
}

impl PreparedPrometheus {
    pub(crate) fn new(config: EffectivePrometheus, runtime: Arc<PrometheusRuntime>) -> Self {
        Self { config, runtime }
    }

    pub(crate) fn router(&self) -> Result<Router> {
        scrape_router(&self.config.path, self.runtime.clone())
    }

    pub(crate) fn runtime(&self) -> Arc<PrometheusRuntime> {
        self.runtime.clone()
    }

    pub(crate) async fn bind_separate(
        &self,
        host: &str,
    ) -> Result<Option<(tokio::net::TcpListener, Router)>> {
        let Some(port) = self.config.port else {
            return Ok(None);
        };
        let app = self.router()?;
        let socket = tokio::net::TcpListener::bind((host, port)).await?;
        Ok(Some((socket, app)))
    }
}

pub(crate) fn scrape_router(path: &str, runtime: Arc<PrometheusRuntime>) -> Result<Router> {
    validate_path(path)?;
    Ok(Router::new().route(
        path,
        get(move || {
            let runtime = runtime.clone();
            async move { runtime.scrape().await }
        }),
    ))
}

pub(crate) struct PrometheusRuntime {
    backend: Arc<RwLock<PrometheusHandle>>,
    process: Option<Arc<ProcessMetrics>>,
    state: Mutex<State>,
    controller: AsyncRwLock<Option<JoinHandle<ShutdownResult>>>,
    upkeep: AsyncRwLock<Option<JoinHandle<ShutdownResult>>>,
    stop: watch::Sender<Option<Instant>>,
    #[cfg(test)]
    upkeep_probe: Arc<UpkeepProbe>,
}

impl PrometheusRuntime {
    #[cfg(test)]
    pub(crate) async fn prepare(
        owner: &ServerCleanupHandle,
        handle: PrometheusHandle,
    ) -> anyhow::Result<Arc<Self>> {
        Self::prepare_with_process(owner, handle, None).await
    }

    /// With `process`, each scrape also appends one process snapshot inside
    /// the same owned render worker.
    pub(crate) async fn prepare_with_process(
        owner: &ServerCleanupHandle,
        handle: PrometheusHandle,
        process: Option<Arc<ProcessMetrics>>,
    ) -> anyhow::Result<Arc<Self>> {
        // Admission keeps cleanup from passing this synchronous publication
        // section. There is no suspension between retention and controller start.
        let _admission = owner.admit()?;
        let (stop, stopping) = watch::channel(None);
        let runtime = Arc::new(Self {
            backend: Arc::new(RwLock::new(handle)),
            process,
            state: Mutex::new(State {
                accepting: true,
                renders: Vec::new(),
                failure: None,
                deadline: None,
            }),
            controller: AsyncRwLock::new(None),
            upkeep: AsyncRwLock::new(None),
            stop,
            #[cfg(test)]
            upkeep_probe: Arc::default(),
        });
        owner.retain_prometheus(runtime.clone())?;
        let worker = runtime.clone();
        let first_tick = Instant::now() + Duration::from_secs(5);
        if let Err(error) = try_spawn_owned_worker(&runtime.controller, async move {
            let outcome = worker.run_upkeep(stopping, first_tick).await;
            if outcome.is_err() {
                let mut state = worker.state();
                state.accepting = false;
                state.remember(outcome.clone());
            }
            outcome
        }) {
            runtime.request_shutdown(Instant::now())?;
            let error: Arc<anyhow::Error> = Arc::new(error.into());
            runtime.state().remember(Err(error.clone()));
            return Err(super::MetricsTerminal(error).into());
        }
        Ok(runtime)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // Only short ownership bookkeeping runs here, never backend work.
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Rejects overload/closure immediately; only response waiting is timed.
    pub(crate) async fn scrape(&self) -> Result<Response, ErrorResponse> {
        let response = {
            let mut state = self.state();
            state.reap_renders();
            if !state.accepting || state.renders.len() >= 4 {
                return Err(unavailable());
            }
            let job = Arc::new(RenderJob::default());
            state.renders.push(job.clone());
            let worker = job.clone();
            let backend = self.backend.clone();
            let process = self.process.clone();
            let (response, receiver) = oneshot::channel();
            if let Err(error) = try_spawn_owned_worker(&job.submission, async move {
                spawn_owned_blocking_worker(&worker.work, move || {
                    // The backend guard is released before process sampling.
                    let rendered = backend
                        .read()
                        .map(|handle| handle.render())
                        .map(|mut text| {
                            if let Some(process) = &process {
                                process.append_prometheus(&mut text);
                            }
                            text
                        })
                        .map_err(|_| {
                            Arc::new(anyhow::Error::new(PrometheusError::BackendPoisoned))
                        });
                    let outcome = rendered.as_ref().map(|_| ()).map_err(Arc::clone);
                    let _ = response.send(rendered);
                    outcome
                })
                .await
                .map_err(|error| Arc::new(error.into()))
            }) {
                state.accepting = false;
                state.remember(Err(Arc::new(error.into())));
                return Err(render_failed());
            }
            receiver
        };
        match timeout(Duration::from_secs(5), response).await {
            Ok(Ok(Ok(body))) => Ok((
                [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
                body,
            )
                .into_response()),
            Ok(Ok(Err(error))) => {
                log::error!("Prometheus render failed: {error:#}");
                Err(render_failed())
            }
            Ok(Err(_)) => {
                self.state()
                    .remember(Err(Arc::new(PrometheusError::ResponseClosed.into())));
                Err(render_failed())
            }
            Err(_) => Err(unavailable()),
        }
    }

    async fn run_upkeep(
        &self,
        mut stopping: watch::Receiver<Option<Instant>>,
        first_tick: Instant,
    ) -> ShutdownResult {
        let mut ticks = interval_at(first_tick, Duration::from_secs(5));
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        #[cfg(test)]
        {
            *self.upkeep_probe.first_tick.lock().unwrap() = Some(first_tick);
        }
        loop {
            if stopping.borrow().is_some() {
                return Ok(());
            }
            tokio::select! {
                biased;
                changed = stopping.changed() => {
                    if changed.is_err() {
                        return Err(Arc::new(PrometheusError::RequestClosed.into()));
                    }
                }
                _ = ticks.tick() => {
                    {
                        let mut state = self.state();
                        state.reap_renders();
                        if state.deadline.is_some() {
                            return Ok(());
                        }
                    }
                    let backend = self.backend.clone();
                    #[cfg(test)]
                    let probe = self.upkeep_probe.clone();
                    let submission = spawn_owned_blocking_worker(&self.upkeep, move || {
                        #[cfg(test)]
                        let _active = UpkeepActive::new(&probe);
                        backend.read()
                            .map_err(|_| Arc::new(anyhow::Error::new(PrometheusError::BackendPoisoned)))?
                            .run_upkeep();
                        #[cfg(test)]
                        probe.completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok(())
                    }).await.map_err(|error| Arc::new(error.into()));
                    submission?;
                    join_slot(&self.upkeep).await?;
                }
            }
        }
    }

    /// Quiesces HTTP ingress while upkeep still drains active producers.
    pub(crate) fn close_scrape_admission(&self) {
        self.state().accepting = false;
    }

    /// Collection-phase stop, after producers; never aborts accepted work.
    pub(crate) fn request_shutdown(&self, deadline: Instant) -> anyhow::Result<()> {
        let mut state = self.state();
        state.accepting = false;
        let deadline = state
            .deadline
            .map_or(deadline, |previous| previous.min(deadline));
        state.deadline = Some(deadline);
        self.stop.send_replace(Some(deadline));
        Ok(())
    }

    pub(crate) async fn wait_shutdown_until(&self, deadline: Instant) -> anyhow::Result<()> {
        let deadline = self
            .state()
            .deadline
            .map_or(deadline, |previous| previous.min(deadline));
        timeout_at(deadline, self.wait_for_completion())
            .await
            .map_err(|_| PrometheusError::WaitExpired)?
    }

    /// Continues only physical joins; no fresh upkeep or replenished phase budget.
    pub(crate) async fn wait_for_completion(&self) -> anyhow::Result<()> {
        ensure!(
            self.state().deadline.is_some(),
            "Prometheus shutdown has not been requested"
        );
        let controller = join_slot(&self.controller).await;
        self.state().remember(controller);
        // Also join this slot if the controller itself exited unexpectedly.
        let upkeep = join_slot(&self.upkeep).await;
        self.state().remember(upkeep);
        let jobs = self.state().renders.clone();
        for job in jobs {
            let submission = join_slot(&job.submission).await;
            self.state().remember(submission);
            let work = join_slot(&job.work).await;
            self.state().remember(work);
            self.state()
                .renders
                .retain(|owned| !Arc::ptr_eq(owned, &job));
        }
        let result = self.state().failure.clone().map_or(Ok(()), Err);
        metrics_terminal(&result)
    }

    pub(crate) async fn is_joined(&self) -> bool {
        self.controller.try_read().is_ok_and(|slot| slot.is_none())
            && self.upkeep.try_read().is_ok_and(|slot| slot.is_none())
            && self.state().renders.is_empty()
    }
}

fn unavailable() -> ErrorResponse {
    ErrorResponse::new(
        error_codes::TELEMETRY_SCRAPE_UNAVAILABLE,
        "Metrics scrape is unavailable",
    )
}

fn render_failed() -> ErrorResponse {
    ErrorResponse::new(
        error_codes::TELEMETRY_RENDER_FAILED,
        "Failed to render metrics",
    )
}

#[cfg(test)]
struct UpkeepActive<'a>(&'a UpkeepProbe);

#[cfg(test)]
impl<'a> UpkeepActive<'a> {
    fn new(probe: &'a UpkeepProbe) -> Self {
        use std::sync::atomic::Ordering;
        probe.started.fetch_add(1, Ordering::SeqCst);
        let active = probe.active.fetch_add(1, Ordering::SeqCst) + 1;
        probe.peak.fetch_max(active, Ordering::SeqCst);
        Self(probe)
    }
}

#[cfg(test)]
impl Drop for UpkeepActive<'_> {
    fn drop(&mut self) {
        self.0
            .active
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
pub(super) mod fixtures {
    use super::*;
    use std::{
        sync::{atomic::Ordering, Condvar},
        time::Duration,
    };

    pub(in crate::telemetry) struct BackendStall {
        release: Arc<(Mutex<bool>, Condvar)>,
        thread: Option<std::thread::JoinHandle<bool>>,
    }

    impl BackendStall {
        pub(in crate::telemetry) async fn acquire(runtime: &PrometheusRuntime) -> Self {
            let backend = runtime.backend.clone();
            let release = Arc::new((Mutex::new(false), Condvar::new()));
            let gate = release.clone();
            let (ready, mut acquired) = tokio::sync::oneshot::channel();
            let thread = std::thread::spawn(move || {
                let _lock = backend.write().unwrap();
                let _ = ready.send(());
                let (lock, changed) = &*gate;
                let result = changed
                    .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(15), |released| {
                        !*released
                    })
                    .unwrap();
                !result.1.timed_out()
            });
            let fixture = Self {
                release,
                thread: Some(thread),
            };
            // External thread startup must not let a paused Tokio clock race
            // ahead to the acquisition timeout or the first upkeep tick.
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            loop {
                match acquired.try_recv() {
                    Ok(()) => break,
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty)
                        if std::time::Instant::now() < deadline =>
                    {
                        tokio::task::yield_now().await
                    }
                    result => panic!("backend fixture acquisition failed: {result:?}"),
                }
            }
            fixture
        }

        pub(in crate::telemetry) fn release(&mut self) {
            let (lock, changed) = &*self.release;
            *lock.lock().unwrap_or_else(|error| error.into_inner()) = true;
            changed.notify_all();
        }

        pub(in crate::telemetry) fn finish(mut self) {
            self.release();
            assert!(
                self.thread
                    .take()
                    .unwrap()
                    .join()
                    .expect("backend fixture worker"),
                "backend fixture exceeded finite guard"
            );
        }
    }

    impl Drop for BackendStall {
        fn drop(&mut self) {
            self.release();
            if let Some(thread) = self.thread.take() {
                // `finish` asserts success in the test body; never double-panic
                // if another assertion unwinds through fixture cleanup.
                let _ = thread.join();
            }
        }
    }

    pub(in crate::telemetry) fn render_count(runtime: &PrometheusRuntime) -> usize {
        runtime.state.lock().unwrap().renders.len()
    }

    pub(in crate::telemetry) fn controller_finished(runtime: &PrometheusRuntime) -> bool {
        runtime
            .controller
            .try_read()
            .is_ok_and(|slot| slot.as_ref().is_some_and(|join| join.is_finished()))
    }

    pub(in crate::telemetry) fn failure(runtime: &PrometheusRuntime) -> Option<Arc<anyhow::Error>> {
        runtime.state().failure.clone()
    }

    pub(in crate::telemetry) fn has_backend_failure(error: &anyhow::Error) -> bool {
        error.chain().any(|cause| {
            matches!(
                cause.downcast_ref::<PrometheusError>(),
                Some(PrometheusError::BackendPoisoned)
            )
        })
    }

    pub(in crate::telemetry) fn collection_deadline(
        runtime: &PrometheusRuntime,
    ) -> Option<Instant> {
        runtime.state().deadline
    }

    pub(in crate::telemetry) fn upkeep_counts(
        runtime: &PrometheusRuntime,
    ) -> (usize, usize, usize, usize) {
        let probe = &runtime.upkeep_probe;
        (
            probe.started.load(Ordering::SeqCst),
            probe.completed.load(Ordering::SeqCst),
            probe.active.load(Ordering::SeqCst),
            probe.peak.load(Ordering::SeqCst),
        )
    }

    pub(in crate::telemetry) fn upkeep_status(runtime: &PrometheusRuntime) -> String {
        let controller = runtime
            .controller
            .try_read()
            .ok()
            .map(|slot| slot.as_ref().map(|join| join.is_finished()));
        let upkeep = runtime
            .upkeep
            .try_read()
            .ok()
            .map(|slot| slot.as_ref().map(|join| join.is_finished()));
        format!(
            "counts={:?}, now={:?}, first_tick={:?}, controller={controller:?}, upkeep={upkeep:?}",
            upkeep_counts(runtime),
            Instant::now(),
            *runtime.upkeep_probe.first_tick.lock().unwrap()
        )
    }

    pub(in crate::telemetry) fn poison_backend(runtime: &PrometheusRuntime) {
        let backend = runtime.backend.clone();
        assert!(std::thread::spawn(move || {
            let _lock = backend.write().unwrap();
            panic!("private fixture poisons the real backend lock");
        })
        .join()
        .is_err());
    }
}
