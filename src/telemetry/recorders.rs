// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! One guarded recorder shared by all producers; no process-global recorder.
//!
//! Canonical reservations live as long as accepted definitions, never samples.
//! Core bounds definitions/series and input bytes. Buckets and upkeep aggregate
//! samples, but do not promise a hard RSS bound under sustained producer load.

use drasi_lib::observability::{
    MetricKind, MetricRegistration, MetricRegistrationContext, MetricRegistrationGuard,
    RegistrationLimits, TelemetryError,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use metrics_opentelemetry::{OpenTelemetryMetrics, OpenTelemetryRecorder};
use opentelemetry::{metrics::MeterProvider, KeyValue};
use opentelemetry_sdk::{
    metrics::{Aggregation, Instrument, InstrumentKind, SdkMeterProvider, Stream},
    Resource,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

pub(crate) const HISTOGRAM_BOUNDS: &[f64] = &[
    0.000001, 0.000005, 0.00001, 0.00005, 0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0,
    5.0, 10.0, 100.0, 1000.0,
];

// Task 4 supplies this once to the retained runtime, not once per graph/export.
pub fn metrics_resource(service_name: &str) -> Resource {
    Resource::builder_empty()
        .with_attributes([
            KeyValue::new("service.name", service_name.to_owned()),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("service.instance.id", uuid::Uuid::new_v4().to_string()),
            KeyValue::new("process.pid", i64::from(std::process::id())),
        ])
        .build()
}

pub fn core_metric_view(
    limits: RegistrationLimits,
) -> anyhow::Result<impl Fn(&Instrument) -> Option<Stream> + Send + Sync> {
    let stream = move |histogram| {
        let builder = Stream::builder().with_cardinality_limit(limits.series);
        let builder = if histogram {
            builder.with_aggregation(Aggregation::ExplicitBucketHistogram {
                boundaries: HISTOGRAM_BOUNDS.to_vec(),
                record_min_max: true,
            })
        } else {
            builder
        };
        builder.build()
    };
    // Stream is not Clone in SDK 0.33.1. Validate both immutable templates
    // before instrument creation; rebuilding cannot introduce a new error.
    stream(false).map_err(|error| anyhow::anyhow!("Invalid Core metric stream: {error}"))?;
    stream(true).map_err(|error| anyhow::anyhow!("Invalid Core histogram stream: {error}"))?;
    Ok(move |instrument: &Instrument| {
        if instrument.scope().name() != "drasi.core" {
            return None;
        }
        Some(
            match stream(instrument.kind() == InstrumentKind::Histogram) {
                Ok(stream) => stream,
                Err(_) => {
                    unreachable!("immutable Core stream templates were validated at preparation")
                }
            },
        )
    })
}

pub fn otlp_recorder(provider: &SdkMeterProvider) -> anyhow::Result<MetricRegistrationContext> {
    let recorder =
        OpenTelemetryRecorder::new(OpenTelemetryMetrics::new(provider.meter("drasi.core")));
    Ok(MetricRegistrationContext::new(
        Arc::new(recorder),
        RegistrationLimits::default(),
    )?)
}

#[derive(Debug, Default)]
struct Reservations {
    definitions: BTreeSet<String>,
    names: BTreeMap<String, String>,
}

/// Called only at Core's shared, transactional acceptance boundary.
#[derive(Debug, Default)]
struct PrometheusGuard(Mutex<Reservations>);

impl MetricRegistrationGuard for PrometheusGuard {
    fn validate_and_commit(&self, registration: &MetricRegistration) -> Result<(), TelemetryError> {
        let histogram = registration.kind() == MetricKind::Histogram;
        let mut labels = BTreeSet::new();
        for (key, _) in registration.labels() {
            let canonical = normalize(key, false);
            if (histogram && canonical == "le") || !labels.insert(canonical) {
                return Err(TelemetryError::Invalid(
                    "Prometheus label keys collide or use the reserved histogram label".into(),
                ));
            }
        }
        let family = normalize(registration.name(), true);
        let mut names = vec![family.clone()];
        if histogram {
            names.extend(["sum", "count", "bucket"].map(|suffix| format!("{family}_{suffix}")));
        }
        let mut state = self.0.lock().map_err(|_| TelemetryError::Poisoned)?;
        for name in &names {
            if state
                .names
                .get(name)
                .is_some_and(|owner| owner != registration.name())
            {
                return Err(TelemetryError::Conflict(registration.name().into()));
            }
        }
        if !state.definitions.contains(registration.name())
            && state.definitions.len() >= RegistrationLimits::default().definitions
        {
            return Err(TelemetryError::Capacity("Prometheus definitions"));
        }
        // All fallible validation precedes this commit, including generated names.
        state.definitions.insert(registration.name().into());
        for name in names {
            state.names.insert(name, registration.name().into());
        }
        Ok(())
    }
}

// Matches metrics-exporter-prometheus 0.17.2's metric-name/label-key sanitizers.
fn normalize(value: &str, metric: bool) -> String {
    value
        .chars()
        .enumerate()
        .map(|(index, ch)| {
            if ch.is_ascii_alphabetic()
                || ch == '_'
                || (metric && ch == ':')
                || (index > 0 && ch.is_ascii_digit())
            {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn prometheus_recorder() -> anyhow::Result<(MetricRegistrationContext, PrometheusHandle)>
{
    let recorder = PrometheusBuilder::new()
        .set_buckets(HISTOGRAM_BOUNDS)?
        .set_enable_unit_suffix(false)
        .build_recorder();
    let handle = recorder.handle();
    let context = MetricRegistrationContext::new_with_guard(
        Arc::new(recorder),
        RegistrationLimits::default(),
        Arc::new(PrometheusGuard::default()),
    )?;
    Ok((context, handle))
}
