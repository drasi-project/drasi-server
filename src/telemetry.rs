// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_lib::computation::v1::SourceProgressKey;
use drasi_lib::context::workers::{
    join_owned_worker_gracefully, spawn_owned_blocking_worker, WorkerCompletion,
};
use opentelemetry::{metrics::MeterProvider, KeyValue};
use opentelemetry_otlp::{MetricExporter, Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    error::{OTelSdkError, OTelSdkResult},
    metrics::{
        data::ResourceMetrics, exporter::PushMetricExporter, PeriodicReader, SdkMeterProvider,
        Temporality,
    },
    Resource,
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::sync::{oneshot, RwLock};

use crate::{
    api::mappings::DtoMapper, api::models::ConfigValue, instance_registry::InstanceRegistry,
};

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

struct ReportingExporter {
    inner: MetricExporter,
    stopped: std::sync::mpsc::Sender<()>,
}

impl PushMetricExporter for ReportingExporter {
    async fn export(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
        let result = self.inner.export(metrics).await;
        if let Err(error) = &result {
            log::warn!("OpenTelemetry metrics export failed: {error}");
        }
        result
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        let result = self.inner.shutdown_with_timeout(timeout);
        let _ = self.stopped.send(());
        result
    }

    fn temporality(&self) -> Temporality {
        self.inner.temporality()
    }
}

type ShutdownResult = std::result::Result<(), Arc<OTelSdkError>>;

/// Owns SDK construction and shutdown off the application's Tokio executor.
#[derive(Default)]
pub(crate) struct MetricsRuntime {
    worker: RwLock<Option<tokio::task::JoinHandle<ShutdownResult>>>,
    stop: Option<std::sync::mpsc::Sender<()>>,
    terminal: Option<ShutdownResult>,
    started: bool,
    join_timeout: Duration,
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
        let cleanup_timeout = timeout * 4 + Duration::from_secs(5);
        self.join_timeout = cleanup_timeout + Duration::from_secs(10);
        let instances = registry.metrics_reader();
        let (stop, stopping) = std::sync::mpsc::channel();
        let (ready, initialized) = oneshot::channel();
        self.stop = Some(stop);
        spawn_owned_blocking_worker(&self.worker, move || {
            let (stopped, exporter_stopped) = std::sync::mpsc::channel();
            let provider = (|| -> Result<SdkMeterProvider> {
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
                let reader = PeriodicReader::builder(ReportingExporter {
                    inner: exporter,
                    stopped,
                })
                    .with_interval(interval)
                    .build();
                let provider = SdkMeterProvider::builder()
                    .with_reader(reader)
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
                Ok(provider)
            })();
            let provider = match provider {
                Ok(provider) => provider,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return Ok(());
                }
            };
            if ready.send(Ok(())).is_ok() {
                // Disconnect is also a shutdown request, including cancellation.
                let _ = stopping.recv();
            }
            let result = provider
                .shutdown_with_timeout(cleanup_timeout)
                .map_err(Arc::new);
            // The SDK reader currently has its own fixed shutdown wait. Retain
            // our owner until the serial exporter has actually finished too.
            exporter_stopped.recv_timeout(cleanup_timeout).map_err(|error| {
                Arc::new(OTelSdkError::InternalFailure(format!(
                    "Metrics exporter shutdown was not confirmed: {error}"
                )))
            })?;
            result
        })
        .await?;
        self.started = true;
        initialized
            .await
            .context("Metrics initialization worker exited")??;
        Ok(())
    }

    pub(crate) async fn shutdown(&mut self) -> Result<()> {
        self.stop.take();
        if let Some(result) = &self.terminal {
            return result.clone().map_err(anyhow::Error::new);
        }
        match join_owned_worker_gracefully(&mut *self.worker.write().await, self.join_timeout)
            .await?
        {
            WorkerCompletion::Completed(result) => {
                self.terminal = Some(result.clone());
                result.map_err(anyhow::Error::new)
            }
            WorkerCompletion::Absent if !self.started => Ok(()),
            _ => anyhow::bail!("Metrics worker disappeared before confirmed shutdown"),
        }
    }
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
