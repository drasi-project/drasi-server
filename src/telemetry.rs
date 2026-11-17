// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_lib::context::workers::spawn_owned_blocking_worker;
use drasi_lib::observability::{MetricRegistrationContext, RegistrationLimits};
use opentelemetry_otlp::{MetricExporter, Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    error::{OTelSdkError, OTelSdkResult},
    metrics::{
        data::ResourceMetrics, exporter::PushMetricExporter, reader::MetricReader, InstrumentKind,
        ManualReader, Pipeline, SdkMeterProvider, Temporality,
    },
    Resource,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::{oneshot, RwLock};
use tokio::time::{timeout_at, Instant};

use crate::{
    api::mappings::DtoMapper, api::models::ConfigValue, instance_registry::InstanceRegistry,
};

mod bindings;
pub use bindings::TelemetryBindings;
pub(crate) mod runtime;
pub use runtime::StandaloneTelemetry;
mod progress;
pub(crate) mod traces;
pub(crate) mod tracing_layer;
mod transport;
use transport::{OtlpSignal, ResolvedOtlpHttpClient as CoreHttpClient};
#[allow(dead_code)] // Removed in Task 4 once startup uses the sampler.
pub(crate) mod process;
pub(crate) mod prometheus;
pub(crate) mod recorders;

#[cfg(test)]
mod metrics_tests;
#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;

/// Optional, application-owned OTLP/HTTP JSON metrics export.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricsConfig {
    /// Full OTLP metrics endpoint, normally ending in `/v1/metrics`.
    pub endpoint: ConfigValue<String>,
    #[serde(default = "default_interval")]
    pub export_interval_ms: u64,
    #[serde(default = "default_timeout")]
    pub export_timeout_ms: u64,
}

fn default_interval() -> u64 {
    10_000
}

fn default_timeout() -> u64 {
    5_000
}

impl MetricsConfig {
    pub fn validate(&self) -> Result<()> {
        let endpoint: String = DtoMapper::new().resolve_typed(&self.endpoint)?;
        let url = reqwest::Url::parse(&endpoint).context("Invalid metrics endpoint URL")?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Metrics endpoint requires HTTP(S), a host, and no embedded credentials, query or fragment"
        );
        anyhow::ensure!(
            (100..=3_600_000).contains(&self.export_interval_ms),
            "Metrics exportIntervalMs must be between 100 and 3600000"
        );
        anyhow::ensure!(
            (100..=30_000).contains(&self.export_timeout_ms),
            "Metrics exportTimeoutMs must be between 100 and 30000"
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ApplicationReader(Arc<ManualReader>);

impl MetricReader for ApplicationReader {
    fn register_pipeline(&self, pipeline: Weak<Pipeline>) {
        self.0.register_pipeline(pipeline);
    }
    fn collect(&self, metrics: &mut ResourceMetrics) -> OTelSdkResult {
        self.0.collect(metrics)
    }
    fn force_flush(&self) -> OTelSdkResult {
        self.0.force_flush()
    }
    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }
    fn temporality(&self, kind: InstrumentKind) -> Temporality {
        self.0.temporality(kind)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MetricsCleanupError {
    #[error("Metrics collection phase deadline elapsed; no fresh final export was started")]
    PhaseExpired,
    #[error("Metrics cleanup wait elapsed; the actual worker remains owned")]
    WaitExpired,
    #[error("Metrics SDK/exporter cleanup failed")]
    Sdk(#[source] OTelSdkError),
    #[error("Metrics worker failed")]
    Join(#[source] tokio::task::JoinError),
    #[error("Metrics worker is missing before confirmed cleanup")]
    MissingWorker,
    #[error("Metrics shutdown request channel closed unexpectedly")]
    RequestClosed,
}

type ShutdownResult = std::result::Result<(), Arc<anyhow::Error>>;

enum MetricsCommand {
    Activate,
    Stop(Instant),
}

struct PreparedMetrics {
    provider: SdkMeterProvider,
    reader: ApplicationReader,
    exporter: MetricExporter,
    context: Option<MetricRegistrationContext>,
    transport: Option<CoreHttpClient>,
    interval: Duration,
    process: Option<process::OtlpProcessInstruments>,
}

impl PreparedMetrics {
    fn build(mode: MetricsMode, registry: &InstanceRegistry) -> Result<Self> {
        let core = matches!(&mode, MetricsMode::CoreOtlp { .. });
        let view = if core {
            Some(recorders::core_metric_view(RegistrationLimits::default())?)
        } else {
            None
        };
        let (exporter, resource, transport, interval, progress_enabled, process) = match mode {
            MetricsMode::Legacy(config) => {
                let endpoint: String = DtoMapper::new().resolve_typed(&config.endpoint)?;
                let timeout = Duration::from_millis(config.export_timeout_ms);
                let client = reqwest_otel::blocking::Client::builder()
                    .timeout(timeout)
                    .redirect(reqwest_otel::redirect::Policy::none())
                    .build()?;
                let exporter = MetricExporter::builder()
                    .with_http()
                    .with_http_client(client)
                    .with_protocol(Protocol::HttpJson)
                    .with_endpoint(endpoint)
                    .with_timeout(timeout)
                    .build()?;
                (
                    exporter,
                    recorders::telemetry_resource("drasi-server"),
                    None,
                    Duration::from_millis(config.export_interval_ms),
                    true,
                    None,
                )
            }
            MetricsMode::CoreOtlp {
                destination,
                export_interval,
                export_timeout,
                resource,
                progress_enabled,
                process,
            } => {
                anyhow::ensure!(
                    !export_interval.is_zero(),
                    "Metrics interval must be nonzero"
                );
                anyhow::ensure!(!export_timeout.is_zero(), "Metrics timeout must be nonzero");
                let transport =
                    CoreHttpClient::new(destination, export_timeout, OtlpSignal::Metrics)?;
                let exporter = tracing::subscriber::with_default(
                    tracing::subscriber::NoSubscriber::default(),
                    || {
                        MetricExporter::builder()
                            .with_http()
                            .with_http_client(transport.clone())
                            .with_protocol(Protocol::HttpBinary)
                            .with_endpoint(transport.destination.endpoint().as_str())
                            .with_timeout(export_timeout)
                            .with_temporality(Temporality::Cumulative)
                            .with_retry_policy(
                                opentelemetry_otlp::RetryPolicy::default().with_max_retries(0),
                            )
                            .build()
                    },
                )
                .map_err(|_| anyhow::anyhow!("Cannot initialize resolved OTLP metrics exporter"))?;
                (
                    exporter,
                    resource,
                    Some(transport),
                    export_interval,
                    progress_enabled,
                    process,
                )
            }
        };
        let reader = ApplicationReader(Arc::new(
            ManualReader::builder()
                .with_temporality(exporter.temporality())
                .build(),
        ));
        let mut builder = SdkMeterProvider::builder()
            .with_reader(reader.clone())
            .with_resource(resource);
        if let Some(view) = view {
            builder = builder.with_view(view);
        }
        let provider = builder.build();
        if progress_enabled {
            progress::register_progress(&provider, registry);
        }
        let process = process.map(|process| process.register_otlp(&provider));
        let context = if core {
            Some(recorders::otlp_recorder(&provider)?)
        } else {
            None
        };
        Ok(Self {
            provider,
            reader,
            exporter,
            context,
            transport,
            interval,
            process,
        })
    }
}

#[derive(Default)]
struct ExportWarnings {
    last_warning: Option<Instant>,
    suppressed: u64,
    failing: bool,
}

impl ExportWarnings {
    fn failed(&mut self, now: Instant) -> Option<u64> {
        self.failing = true;
        if self
            .last_warning
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(30))
        {
            self.suppressed = self.suppressed.saturating_add(1);
            return None;
        }
        self.last_warning = Some(now);
        Some(std::mem::take(&mut self.suppressed))
    }

    fn recovered(&mut self) -> Option<u64> {
        std::mem::take(&mut self.failing).then(|| std::mem::take(&mut self.suppressed))
    }
}

pub(crate) enum MetricsMode {
    Legacy(MetricsConfig),
    CoreOtlp {
        destination: crate::config::telemetry::OtlpDestination,
        export_interval: Duration,
        export_timeout: Duration,
        resource: Resource,
        progress_enabled: bool,
        process: Option<Arc<process::ProcessMetrics>>,
    },
}

/// Owns SDK construction and shutdown off the application's Tokio executor.
#[derive(Default)]
pub(crate) struct MetricsRuntime {
    worker: RwLock<Option<tokio::task::JoinHandle<ShutdownResult>>>,
    stop: Option<std::sync::mpsc::Sender<MetricsCommand>>,
    terminal: Option<ShutdownResult>,
    started: bool,
    activated: bool,
    resource: Option<Resource>,
    deadline: Option<Instant>,
    #[cfg(test)]
    shutdown_probe: Arc<tests::ShutdownProbe>,
}

impl MetricsRuntime {
    pub(crate) async fn prepare(
        &mut self,
        mode: MetricsMode,
        registry: &InstanceRegistry,
    ) -> Result<Option<MetricRegistrationContext>> {
        anyhow::ensure!(!self.started, "Metrics runtime has already started");
        if let MetricsMode::Legacy(config) = &mode {
            config.validate()?;
        }
        self.resource = match &mode {
            MetricsMode::CoreOtlp { resource, .. } => Some(resource.clone()),
            MetricsMode::Legacy(_) => None,
        };
        let registry = registry.clone();
        let (stop, stopping) = std::sync::mpsc::channel();
        let (ready, initialized) = oneshot::channel();
        self.stop = Some(stop);
        self.started = true;
        #[cfg(test)]
        let shutdown_probe = self.shutdown_probe.clone();
        spawn_owned_blocking_worker(&self.worker, move || {
            #[cfg(test)]
            shutdown_probe.pause_preparation();
            let prepared = match PreparedMetrics::build(mode, &registry) {
                Ok(prepared) => prepared,
                Err(error) => {
                    let error = Arc::new(error);
                    let _ = ready.send(Err(MetricsTerminal(error.clone()).into()));
                    return Err(error);
                }
            };
            drop(registry);
            let PreparedMetrics { provider, reader, exporter, context, transport, interval, process } = prepared;
            let collect = |deadline: Option<Instant>| -> OTelSdkResult {
                // Exactly one process snapshot per periodic or final collection.
                if let Some(process) = &process {
                    process.refresh();
                }
                let mut metrics = ResourceMetrics::default();
                reader.collect(&mut metrics)?;
                if metrics.scope_metrics().next().is_some() {
                    if let Some(transport) = &transport {
                        transport.allow_until(deadline)?;
                        // Pinned SDK retry/response diagnostics are not policy-aware
                        // and may echo collector data. Report sanitized, rate-limited
                        // failures below instead, on this owned worker only.
                        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                            futures::executor::block_on(exporter.export(&metrics))
                        })?;
                    } else {
                        futures::executor::block_on(exporter.export(&metrics))?;
                    }
                }
                Ok(())
            };
            let mut active = false;
            let mut warnings = ExportWarnings::default();
            let deadline = if ready.send(Ok(context)).is_ok() {
                // Preparation never exports, even if its caller is cancelled.
                // Only an explicit activation command starts the periodic clock.
                let mut next_tick: Option<Instant> = None;
                loop {
                    let command = match next_tick {
                        Some(tick) => stopping.recv_timeout(
                            tick.saturating_duration_since(Instant::now())
                        ),
                        None => stopping.recv().map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected),
                    };
                    match command {
                        Ok(MetricsCommand::Stop(deadline)) => break deadline,
                        Ok(MetricsCommand::Activate) => {
                            active = true;
                            next_tick = Some(Instant::now() + interval);
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break Instant::now(),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            let start = Instant::now();
                            let outcome = collect(None);
                            #[cfg(test)]
                            shutdown_probe.periodic_exports.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            match outcome {
                                Err(error) if transport.is_none() => {
                                    log::warn!("OpenTelemetry metrics export failed: {error}");
                                }
                                Err(_) => {
                                    if let Some(suppressed) = warnings.failed(Instant::now()) {
                                        log::warn!("OpenTelemetry metrics export failed; {suppressed} repeated failures suppressed");
                                    }
                                }
                                Ok(()) => {
                                    if let Some(suppressed) = warnings.recovered() {
                                        log::info!("OpenTelemetry metrics export recovered; {suppressed} repeated failures suppressed");
                                    }
                                }
                            }
                            next_tick = Some(if transport.is_none() {
                                // Preserve legacy duration-counts-against-interval timing.
                                Instant::now() + interval.saturating_sub(start.elapsed())
                            } else {
                                let mut tick = next_tick.expect("an active export has a periodic tick") + interval;
                                while tick <= Instant::now() {
                                    tick += interval;
                                }
                                tick
                            });
                        }
                    }
                }
            } else {
                Instant::now()
            };
            let mut failures = Vec::new();
            if active {
                if Instant::now() < deadline {
                    if let Err(error) = collect(Some(deadline)) {
                        failures.push(MetricsCleanupError::Sdk(error).into());
                    }
                } else {
                    failures.push(MetricsCleanupError::PhaseExpired.into());
                }
                if Instant::now() < deadline {
                    if let Err(error) = exporter.force_flush() {
                        failures.push(MetricsCleanupError::Sdk(error).into());
                    }
                }
                if Instant::now() >= deadline && !failures.iter().any(|failure: &anyhow::Error| {
                    matches!(failure.downcast_ref::<MetricsCleanupError>(), Some(MetricsCleanupError::PhaseExpired))
                }) {
                    failures.push(MetricsCleanupError::PhaseExpired.into());
                }
            }
            // ManualReader has no worker. All collection, transport, close and
            // Drop work remain on this one retained application worker.
            #[cfg(test)]
            shutdown_probe.sdk.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if let Err(error) = provider.shutdown_with_timeout(
                deadline.saturating_duration_since(Instant::now()),
            ) {
                failures.push(MetricsCleanupError::Sdk(error).into());
            }
            #[cfg(test)]
            shutdown_probe.exporter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if let Err(error) = exporter.shutdown_with_timeout(
                deadline.saturating_duration_since(Instant::now()),
            ) {
                failures.push(MetricsCleanupError::Sdk(error).into());
            }
            drop((provider, reader, exporter));
            if failures.is_empty() {
                Ok(())
            } else {
                Err(Arc::new(drasi_lib::error::OperationFailures::new(
                    "Metrics cleanup failures", failures,
                ).into()))
            }
        })
        .await?;
        initialized
            .await
            .context("Metrics initialization worker exited")?
    }

    pub(crate) fn activate(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.started && !self.activated,
            "Metrics runtime is not prepared or is already active"
        );
        self.stop
            .as_ref()
            .context("Metrics runtime is stopping")?
            .send(MetricsCommand::Activate)
            .map_err(|_| MetricsCleanupError::RequestClosed)?;
        self.activated = true;
        Ok(())
    }

    pub(crate) async fn start(
        &mut self,
        config: MetricsConfig,
        registry: &InstanceRegistry,
    ) -> Result<()> {
        self.prepare(MetricsMode::Legacy(config), registry).await?;
        self.activate()
    }

    #[cfg(test)]
    pub(crate) async fn shutdown(&mut self) -> Result<()> {
        let deadline = *self.deadline.get_or_insert_with(|| {
            Instant::now()
                + Duration::from_secs(crate::config::telemetry::DEFAULT_SHUTDOWN_TIMEOUT_SECONDS)
        });
        self.request_shutdown(deadline)?;
        self.wait_shutdown_until(deadline).await
    }

    pub(crate) fn request_shutdown(&mut self, deadline: Instant) -> Result<()> {
        let deadline = *self.deadline.get_or_insert(deadline);
        if let Some(stop) = self.stop.take() {
            stop.send(MetricsCommand::Stop(deadline))
                .map_err(|_| MetricsCleanupError::RequestClosed)?;
        }
        Ok(())
    }

    pub(crate) async fn wait_shutdown_until(&mut self, deadline: Instant) -> Result<()> {
        if let Some(result) = &self.terminal {
            return metrics_terminal(result);
        }
        let result = {
            let mut slot = timeout_at(deadline, self.worker.write())
                .await
                .map_err(|_| MetricsCleanupError::WaitExpired)?;
            let result = match slot.as_mut() {
                Some(join) => match timeout_at(deadline, join).await {
                    Ok(Ok(result)) => result,
                    Ok(Err(error)) => Err(Arc::new(MetricsCleanupError::Join(error).into())),
                    Err(_) => return Err(MetricsCleanupError::WaitExpired.into()),
                },
                None if !self.started => Ok(()),
                None => Err(Arc::new(MetricsCleanupError::MissingWorker.into())),
            };
            *slot = None;
            result
        };
        self.terminal = Some(result.clone());
        metrics_terminal(&result)
    }

    pub(crate) async fn wait_for_completion(&mut self) -> Result<()> {
        if let Some(result) = &self.terminal {
            return metrics_terminal(result);
        }
        let result = {
            let mut slot = self.worker.write().await;
            let result = match slot.as_mut() {
                Some(join) => match join.await {
                    Ok(result) => result,
                    Err(error) => Err(Arc::new(MetricsCleanupError::Join(error).into())),
                },
                None if !self.started => Ok(()),
                None => Err(Arc::new(MetricsCleanupError::MissingWorker.into())),
            };
            *slot = None;
            result
        };
        self.terminal = Some(result.clone());
        metrics_terminal(&result)
    }

    pub(crate) async fn is_joined(&self) -> bool {
        self.worker.read().await.is_none()
    }
}

#[derive(Debug)]
struct MetricsTerminal(Arc<anyhow::Error>);

impl std::fmt::Display for MetricsTerminal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Metrics cleanup completed with a failed outcome")
    }
}

impl std::error::Error for MetricsTerminal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref().as_ref())
    }
}

fn metrics_terminal(result: &ShutdownResult) -> Result<()> {
    result
        .clone()
        .map_err(|error| MetricsTerminal(error).into())
}

impl Drop for MetricsRuntime {
    fn drop(&mut self) {
        self.stop.take();
        if self.worker.get_mut().is_some() {
            log::warn!("Metrics runtime dropped without awaited shutdown; export cleanup is not yet confirmed");
        }
    }
}
