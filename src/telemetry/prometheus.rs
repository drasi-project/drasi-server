// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Concrete, retained Prometheus render and upkeep ownership, not a listener.
//!
//! Four occupied render slots act as permits until actual work exits and its
//! join is reaped. Cancellation and the five-second response timeout do not
//! cancel workers. One five-second, missed-tick-skipping upkeep schedule drains
//! histogram samples even without scrapers. Work is serialized at the backend,
//! off the Tokio executor; neither this scheduling nor buckets bound hard RSS.
//! Cleanup closes admission and retains every actual join past wait deadlines.
//! Failures are logged and retained for cleanup, never rendered as empty success.

use std::{sync::{Arc, Mutex}, time::Duration};

use anyhow::{anyhow, Context, Result};
use axum::{http::header::CONTENT_TYPE, response::{IntoResponse, Response}};
use drasi_lib::context::workers::{spawn_owned_blocking_worker, try_spawn_owned_worker};
use futures_util::FutureExt;
use metrics_exporter_prometheus::PrometheusHandle;
use tokio::{
    sync::{oneshot, watch, RwLock},
    task::JoinHandle,
    time::{interval_at, timeout, timeout_at, Instant, MissedTickBehavior},
};

use crate::{
    api::shared::error::{error_codes, ErrorResponse},
    server::ServerCleanupHandle,
};

const RENDER_SLOTS: usize = 4;
const SCRAPE_WAIT: Duration = Duration::from_secs(5);
const UPKEEP_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
struct RenderJob {
    join: RwLock<Option<JoinHandle<()>>>,
}

#[derive(Default)]
struct State {
    renders: Vec<Arc<RenderJob>>,
    deadline: Option<Instant>,
    failure: Option<Arc<anyhow::Error>>,
}

pub(crate) struct PrometheusRuntime {
    backend: Arc<Mutex<PrometheusHandle>>,
    state: Arc<Mutex<State>>,
    stop: watch::Sender<bool>,
    upkeep: RwLock<Option<JoinHandle<()>>>,
    upkeep_work: RwLock<Option<JoinHandle<Result<()>>>>,
    #[cfg(test)]
    upkeep_completed: Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(test)]
    upkeep_started: Arc<std::sync::atomic::AtomicUsize>,
}

fn remember_failure(state: &mut State, error: anyhow::Error) {
    log::error!("Prometheus work failed: {error:#}");
    state.failure.get_or_insert_with(|| Arc::new(error));
}

fn reap_renders(state: &mut State) {
    let mut failure = None;
    state.renders.retain(|job| {
        let Ok(mut slot) = job.join.try_write() else { return true; };
        let Some(join) = slot.as_mut() else { return false; };
        if !join.is_finished() {
            return true;
        }
        if let Some(result) = join.now_or_never() {
            *slot = None;
            if let Err(error) = result {
                let error = anyhow::Error::new(error);
                log::error!("Prometheus render join failed: {error:#}");
                failure.get_or_insert(error);
            }
            false
        } else {
            true
        }
    });
    if let Some(error) = failure {
        remember_failure(state, error);
    }
}

impl PrometheusRuntime {
    /// Retain the controller before starting any upkeep; never mount scrape access.
    pub(crate) async fn prepare(
        owner: &ServerCleanupHandle,
        handle: PrometheusHandle,
    ) -> Result<Arc<Self>> {
        let runtime = Arc::new(Self {
            backend: Arc::new(Mutex::new(handle)),
            state: Arc::new(Mutex::new(State::default())),
            stop: watch::channel(false).0,
            upkeep: RwLock::new(None),
            upkeep_work: RwLock::new(None),
            #[cfg(test)]
            upkeep_completed: Arc::default(),
            #[cfg(test)]
            upkeep_started: Arc::default(),
        });
        owner.retain_prometheus(runtime.clone())?;
        {
            let mut state = runtime.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.deadline.is_none() {
                let worker = runtime.clone();
                let first_tick = Instant::now() + UPKEEP_INTERVAL;
                if let Err(error) = try_spawn_owned_worker(&runtime.upkeep, async move {
                    worker.run_upkeep(first_tick).await;
                }) {
                    remember_failure(&mut state, error.into());
                    return Err(anyhow!("Cannot start the retained Prometheus upkeep worker"));
                }
            }
        }
        Ok(runtime)
    }

    /// Reject overload immediately; time out only the waiter, never its worker.
    pub(crate) async fn scrape(&self) -> Result<Response, ErrorResponse> {
        let received = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            reap_renders(&mut state);
            if state.deadline.is_some() || state.renders.len() >= RENDER_SLOTS {
                return Err(unavailable());
            }
            let job = Arc::new(RenderJob::default());
            state.renders.push(job.clone());
            let backend = self.backend.clone();
            let failures = self.state.clone();
            let (send, received) = oneshot::channel();
            // This fresh, private slot cannot contend. Polling the owned helper
            // once commits the actual blocking join under the admission lock,
            // with no cancellation gap between reserving and submitting work.
            let submitted = spawn_owned_blocking_worker(&job.join, move || {
                let rendered = backend.lock()
                    .map_err(|_| anyhow!("Prometheus backend lock was poisoned"))
                    .map(|handle| handle.render());
                match rendered {
                    Ok(body) => { let _ = send.send(Ok(body)); }
                    Err(error) => {
                        remember_failure(
                            &mut failures.lock().unwrap_or_else(|error| error.into_inner()),
                            error,
                        );
                        let _ = send.send(Err(()));
                    }
                }
            }).now_or_never();
            match submitted {
                Some(Ok(())) => received,
                result => {
                    state.renders.pop();
                    let error = match result {
                        Some(Err(error)) => anyhow::Error::new(error),
                        _ => anyhow!("Fresh Prometheus worker slot unexpectedly contended"),
                    };
                    remember_failure(&mut state, error);
                    return Err(render_failed());
                }
            }
        };
        match timeout(SCRAPE_WAIT, received).await {
            Ok(Ok(Ok(body))) => Ok(([
                (CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8"),
            ], body).into_response()),
            Ok(Ok(Err(()))) => Err(render_failed()),
            Ok(Err(error)) => {
                remember_failure(
                    &mut self.state.lock().unwrap_or_else(|error| error.into_inner()),
                    anyhow::Error::new(error).context("Prometheus render worker exited without a result"),
                );
                Err(render_failed())
            }
            Err(_) => Err(unavailable()),
        }
    }

    async fn run_upkeep(&self, first_tick: Instant) {
        let mut stop = self.stop.subscribe();
        let mut ticks = interval_at(first_tick, UPKEEP_INTERVAL);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            if *stop.borrow() {
                break;
            }
            tokio::select! {
                biased;
                changed = stop.changed() => {
                    if let Err(error) = changed {
                        remember_failure(
                            &mut self.state.lock().unwrap_or_else(|error| error.into_inner()),
                            error.into(),
                        );
                        break;
                    }
                }
                _ = ticks.tick() => {
                    {
                        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        reap_renders(&mut state);
                        if state.deadline.is_some() {
                            break;
                        }
                    }
                    let backend = self.backend.clone();
                    #[cfg(test)]
                    let completed = self.upkeep_completed.clone();
                    #[cfg(test)]
                    let started = self.upkeep_started.clone();
                    let result = async {
                        spawn_owned_blocking_worker(&self.upkeep_work, move || {
                            #[cfg(test)]
                            started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            let handle = backend.lock()
                                .map_err(|_| anyhow!("Prometheus backend lock was poisoned"))?;
                            handle.run_upkeep();
                            #[cfg(test)]
                            completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            Ok(())
                        }).await?;
                        let mut slot = self.upkeep_work.write().await;
                        let result = slot.as_mut().context("Prometheus upkeep worker is missing")?.await;
                        *slot = None;
                        result?
                    }.await;
                    if let Err(error) = result {
                        remember_failure(
                            &mut self.state.lock().unwrap_or_else(|error| error.into_inner()),
                            error,
                        );
                        break;
                    }
                }
            }
        }
    }

    /// Close admission once; repeated calls never replenish the accepted deadline.
    pub(crate) fn request_shutdown(&self, deadline: Instant) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.deadline.get_or_insert(deadline);
        self.stop.send_replace(true);
        Ok(())
    }

    /// An expired/cancelled wait keeps every actual worker available for joining.
    pub(crate) async fn wait_shutdown_until(&self, deadline: Instant) -> Result<()> {
        timeout_at(deadline, self.wait_for_completion()).await
            .context("Prometheus cleanup wait elapsed; actual work remains owned")?
    }

    pub(crate) async fn wait_for_completion(&self) -> Result<()> {
        let mut upkeep = self.upkeep.write().await;
        if let Some(join) = upkeep.as_mut() {
            let result = join.await;
            *upkeep = None;
            if let Err(error) = result {
                remember_failure(
                    &mut self.state.lock().unwrap_or_else(|error| error.into_inner()),
                    error.into(),
                );
            }
        }
        drop(upkeep);
        // Also join actual upkeep if its scheduling task itself failed.
        let mut work = self.upkeep_work.write().await;
        if let Some(join) = work.as_mut() {
            let result = join.await;
            *work = None;
            if let Err(error) = result.map_err(anyhow::Error::new).and_then(|result| result) {
                remember_failure(
                    &mut self.state.lock().unwrap_or_else(|error| error.into_inner()),
                    error,
                );
            }
        }
        drop(work);
        let jobs = self.state.lock().unwrap_or_else(|error| error.into_inner()).renders.clone();
        for job in jobs {
            let mut slot = job.join.write().await;
            if let Some(join) = slot.as_mut() {
                let result = join.await;
                *slot = None;
                if let Err(error) = result {
                    remember_failure(
                        &mut self.state.lock().unwrap_or_else(|error| error.into_inner()),
                        error.into(),
                    );
                }
            }
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        reap_renders(&mut state);
        match &state.failure {
            Some(error) => Err(PrometheusFailure(error.clone()).into()),
            None => Ok(()),
        }
    }

    pub(crate) async fn is_joined(&self) -> bool {
        if !self.upkeep.try_read().is_ok_and(|slot| slot.is_none())
            || !self.upkeep_work.try_read().is_ok_and(|slot| slot.is_none())
        {
            return false;
        }
        self.state.lock().unwrap_or_else(|error| error.into_inner())
            .renders.iter().all(|job| job.join.try_read().is_ok_and(|slot| slot.is_none()))
    }
}

fn unavailable() -> ErrorResponse {
    ErrorResponse::new(error_codes::TELEMETRY_SCRAPE_UNAVAILABLE, "Metrics scrape is unavailable")
}

fn render_failed() -> ErrorResponse {
    ErrorResponse::new(error_codes::TELEMETRY_RENDER_FAILED, "Metrics rendering failed")
}

#[derive(Debug)]
struct PrometheusFailure(Arc<anyhow::Error>);

impl std::fmt::Display for PrometheusFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Prometheus cleanup completed with a failed outcome")
    }
}

impl std::error::Error for PrometheusFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref().as_ref())
    }
}

#[cfg(test)]
pub(super) mod fixtures {
    use super::*;

    pub(in crate::telemetry) fn hold_backend(runtime: &PrometheusRuntime) -> std::sync::MutexGuard<'_, PrometheusHandle> {
        runtime.backend.lock().unwrap()
    }

    pub(in crate::telemetry) fn poison_backend(runtime: &PrometheusRuntime) {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = runtime.backend.lock().unwrap();
            panic!("private fixture poisons the real backend lock");
        })).is_err());
    }

    pub(in crate::telemetry) fn render_count(runtime: &PrometheusRuntime) -> usize {
        runtime.state.lock().unwrap().renders.len()
    }

    pub(in crate::telemetry) async fn wait_upkeep_worker(runtime: &PrometheusRuntime) -> usize {
        for _ in 0..100_000 {
            let started = runtime.upkeep_started.load(std::sync::atomic::Ordering::SeqCst);
            if started > 0 {
                return started;
            }
            tokio::task::yield_now().await;
        }
        panic!("no actual upkeep worker was retained");
    }

    pub(in crate::telemetry) fn completed_upkeep(runtime: &PrometheusRuntime) -> usize {
        runtime.upkeep_completed.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(in crate::telemetry) async fn join_render_workers(runtime: &PrometheusRuntime) {
        let jobs = runtime.state.lock().unwrap().renders.clone();
        for job in jobs {
            let mut slot = job.join.write().await;
            if let Some(join) = slot.as_mut() {
                join.await.unwrap();
                *slot = None;
            }
        }
    }
}
