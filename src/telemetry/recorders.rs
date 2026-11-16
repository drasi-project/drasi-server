// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! One guarded recorder/context, with fixed histogram aggregation and no globals.
//!
//! Reservations grow only with accepted definitions, not observations or label
//! values. Core owns the definition/series limits; this guard only indexes their
//! exported names. Periodic upkeep drains samples but is not a hard RSS bound.

use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, Mutex}};
use drasi_lib::observability::{
    MetricKind, MetricRegistration, MetricRegistrationContext, MetricRegistrationGuard,
    RegistrationLimits, TelemetryError,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

pub(super) const HISTOGRAM_BOUNDS: &[f64] = &[
    0.000001, 0.000005, 0.00001, 0.00005, 0.0001, 0.0005, 0.001, 0.005, 0.01,
    0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 100.0, 1000.0,
];

#[derive(Debug, Default)]
struct PrometheusGuard(Mutex<BTreeMap<String, String>>);

impl MetricRegistrationGuard for PrometheusGuard {
    fn validate_and_commit(&self, registration: &MetricRegistration) -> Result<(), TelemetryError> {
        let histogram = registration.kind() == MetricKind::Histogram;
        let mut labels = BTreeSet::new();
        for (key, _) in registration.labels() {
            let normalized = normalize(key, false);
            if (histogram && normalized == "le") || !labels.insert(normalized) {
                return Err(TelemetryError::Invalid(
                    "Prometheus label collision or reserved histogram label".into(),
                ));
            }
        }
        let family = normalize(registration.name(), true);
        let mut names = vec![family.clone()];
        if histogram {
            names.extend(["bucket", "sum", "count"].map(|suffix| format!("{family}_{suffix}")));
        }
        let mut owners = self.0.lock().map_err(|_| TelemetryError::Poisoned)?;
        for name in &names {
            if owners.get(name).is_some_and(|owner| owner != registration.name()) {
                return Err(TelemetryError::Conflict(registration.name().into()));
            }
        }
        for name in names {
            owners.entry(name).or_insert_with(|| registration.name().into());
        }
        Ok(())
    }
}

// Exact metric-name/label-key rules from metrics-exporter-prometheus 0.17.2.
// In particular, an invalid leading character is replaced, not prefixed.
fn normalize(value: &str, metric: bool) -> String {
    value.chars().enumerate().map(|(index, ch)| {
        if ch.is_ascii_alphabetic() || ch == '_' || (metric && ch == ':')
            || (index > 0 && ch.is_ascii_digit())
        {
            ch
        } else {
            '_'
        }
    }).collect()
}

/// Construct without installing globals, enabling idle eviction or starting work.
/// Units are retained without scaling or suffixes, including unitless histograms.
pub(super) fn prometheus_recorder() -> anyhow::Result<(MetricRegistrationContext, PrometheusHandle)> {
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
