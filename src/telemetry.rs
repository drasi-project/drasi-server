// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_lib::computation::v1::SourceProgressKey;
use drasi_lib::context::workers::spawn_owned_blocking_worker;
use opentelemetry::{metrics::MeterProvider, KeyValue};
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
mod runtime;
pub use runtime::StandaloneTelemetry;

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

/// Owns SDK construction and shutdown off the application's Tokio executor.
#[derive(Default)]
pub(crate) struct MetricsRuntime {
    worker: RwLock<Option<tokio::task::JoinHandle<ShutdownResult>>>,
    stop: Option<std::sync::mpsc::Sender<Instant>>,
    terminal: Option<ShutdownResult>,
    started: bool,
    deadline: Option<Instant>,
    #[cfg(test)]
    shutdown_probe: Arc<tests::ShutdownProbe>,
}

impl MetricsRuntime {
    pub(crate) async fn start(
        &mut self,
        config: MetricsConfig,
        registry: &InstanceRegistry,
    ) -> Result<()> {
        config.validate()?;
        anyhow::ensure!(!self.started, "Metrics runtime has already started");
        let endpoint: String = DtoMapper::new().resolve_typed(&config.endpoint)?;
        let timeout = Duration::from_millis(config.export_timeout_ms);
        let interval = Duration::from_millis(config.export_interval_ms);
        let instances = registry.metrics_reader();
        let (stop, stopping) = std::sync::mpsc::channel();
        let (ready, initialized) = oneshot::channel();
        self.stop = Some(stop);
        #[cfg(test)]
        let shutdown_probe = self.shutdown_probe.clone();
        spawn_owned_blocking_worker(&self.worker, move || {
            let provider = (|| -> Result<(SdkMeterProvider, ApplicationReader, MetricExporter)> {
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
                let reader = ApplicationReader(Arc::new(
                    ManualReader::builder().with_temporality(exporter.temporality()).build(),
                ));
                let provider = SdkMeterProvider::builder()
                    .with_reader(reader.clone())
                    .with_resource(
                        Resource::builder_empty()
                            .with_attributes([
                                KeyValue::new("service.name", "drasi-server"),
                                KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
                                KeyValue::new("service.instance.id", uuid::Uuid::new_v4().to_string()),
                                KeyValue::new("process.pid", i64::from(std::process::id())),
                            ])
                            .build(),
                    )
                    .build();
                provider
                    .meter("drasi.progress")
                    .u64_observable_gauge("drasi.query.progress.sequence")
                    .with_description(
                        "Committed logical input or produced output sequence of a ready query",
                    )
                    .with_callback(move |observer| {
                        let instances = match instances() {
                            Ok(instances) => instances,
                            Err(error) => {
                                log::warn!("Cannot collect query progress metrics: {error:#}");
                                return;
                            }
                        };
                        for (instance_id, instance) in instances {
                            let snapshots = match instance.get_query_progress_metrics() {
                                Ok(snapshots) => snapshots,
                                Err(error) => {
                                    log::warn!(
                                        "Cannot collect query progress for instance '{instance_id}': {error}"
                                    );
                                    continue;
                                }
                            };
                            for snapshot in snapshots {
                                let attributes = [
                                    KeyValue::new("drasi.instance.id", instance_id.clone()),
                                    KeyValue::new("drasi.query.id", snapshot.query_id),
                                    KeyValue::new(
                                        "drasi.component.generation",
                                        snapshot.component_generation.to_string(),
                                    ),
                                    KeyValue::new(
                                        "drasi.component.operation",
                                        snapshot.component_operation.to_string(),
                                    ),
                                    KeyValue::new(
                                        "drasi.query.output.generation",
                                        snapshot.output_generation.to_string(),
                                    ),
                                    KeyValue::new(
                                        "drasi.query.config_hash",
                                        snapshot.config_hash.to_string(),
                                    ),
                                    KeyValue::new("drasi.progress.persistent", snapshot.persistent),
                                ];
                                let mut output_attributes = attributes.to_vec();
                                output_attributes.push(KeyValue::new(
                                    "drasi.progress.stage",
                                    "output.produced",
                                ));
                                observe_sequence(observer, snapshot.output_sequence, &output_attributes);
                                for (key, sequence) in snapshot.input_sequences {
                                    let (kind, id) = match key {
                                        SourceProgressKey::Source(id) => ("source", id),
                                        SourceProgressKey::Stream(id) => ("stream", id.to_string()),
                                    };
                                    let mut input_attributes = attributes.to_vec();
                                    input_attributes.extend([
                                        KeyValue::new("drasi.progress.stage", "input.committed"),
                                        KeyValue::new("drasi.input.kind", kind),
                                        KeyValue::new("drasi.input.id", id),
                                    ]);
                                    observe_sequence(observer, sequence, &input_attributes);
                                }
                            }
                        }
                    })
                    .build();
                Ok((provider, reader, exporter))
            })();
            let (provider, reader, exporter) = match provider {
                Ok(provider) => provider,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return Ok(());
                }
            };
            let collect = || -> OTelSdkResult {
                let mut metrics = ResourceMetrics::default();
                reader.collect(&mut metrics)?;
                if metrics.scope_metrics().next().is_some() {
                    // The blocking HTTP client must not run inside Tokio's async context.
                    futures::executor::block_on(exporter.export(&metrics))?;
                }
                Ok(())
            };
            let mut next_interval = interval;
            let deadline = if ready.send(Ok(())).is_err() {
                Instant::now()
            } else {
                loop {
                    match stopping.recv_timeout(next_interval) {
                        Ok(deadline) => break deadline,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break Instant::now(),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            let start = std::time::Instant::now();
                            if let Err(error) = collect() {
                                log::warn!("OpenTelemetry metrics export failed: {error}");
                            }
                            // Match the legacy reader: export duration counts
                            // against the interval, and slow exports never overlap.
                            next_interval = interval.saturating_sub(start.elapsed());
                        }
                    }
                }
            };
            let mut failures = Vec::new();
            if Instant::now() < deadline {
                if let Err(error) = collect() {
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
                failure.is::<MetricsCleanupError>()
                    && matches!(failure.downcast_ref::<MetricsCleanupError>(), Some(MetricsCleanupError::PhaseExpired))
            }) {
                failures.push(MetricsCleanupError::PhaseExpired.into());
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
        self.started = true;
        initialized
            .await
            .context("Metrics initialization worker exited")??;
        Ok(())
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
            stop.send(deadline)
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

fn observe_sequence(
    observer: &dyn opentelemetry::metrics::AsyncInstrument<u64>,
    sequence: u64,
    attributes: &[KeyValue],
) {
    if sequence > i64::MAX as u64 {
        log::error!(
            "Query progress exceeds OTLP's signed integer range; not exporting a wrapped value"
        );
        return;
    }
    observer.observe(sequence, attributes);
}

impl Drop for MetricsRuntime {
    fn drop(&mut self) {
        self.stop.take();
        if self.worker.get_mut().is_some() {
            log::warn!("Metrics runtime dropped without awaited shutdown; export cleanup is not yet confirmed");
        }
    }
}
