// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use crate::instance_registry::InstanceRegistry;
use drasi_lib::computation::v1::SourceProgressKey;
use opentelemetry::{metrics::MeterProvider, KeyValue};
use opentelemetry_sdk::metrics::SdkMeterProvider;

pub(super) fn register_progress(provider: &SdkMeterProvider, registry: &InstanceRegistry) {
    let instances = registry.metrics_reader();
    provider
        .meter("drasi.progress")
        .u64_observable_gauge("drasi.query.progress.sequence")
        .with_description("Committed logical input or produced output sequence of a ready query")
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
                        KeyValue::new("drasi.query.config_hash", snapshot.config_hash.to_string()),
                        KeyValue::new("drasi.progress.persistent", snapshot.persistent),
                    ];
                    let mut output_attributes = attributes.to_vec();
                    output_attributes
                        .push(KeyValue::new("drasi.progress.stage", "output.produced"));
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
