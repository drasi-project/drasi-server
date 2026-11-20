// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::Result;
use drasi_lib::observability::{ResolvedTelemetry, TELEMETRY_TARGET};
use opentelemetry::{
    global::{BoxedSpan, BoxedTracer},
    trace::{noop::NoopTracer, SpanBuilder, Tracer},
};
use opentelemetry_sdk::trace::SdkTracer;
use std::sync::{Arc, OnceLock};
use tracing_subscriber::{filter::filter_fn, Layer, Registry};

struct ConfiguredTracer {
    tracer: BoxedTracer,
    policy: ResolvedTelemetry,
}

#[derive(Clone)]
pub(crate) struct StartupTraceSlot(Arc<OnceLock<ConfiguredTracer>>);

impl StartupTraceSlot {
    pub(crate) fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    pub(crate) fn layer(&self) -> impl Layer<Registry> + Send + Sync {
        let slot = self.clone();
        tracing_opentelemetry::layer()
            .with_tracer(StartupTracer(self.clone()))
            .with_filter(filter_fn(move |metadata| {
                metadata.is_span()
                    && metadata.target() == TELEMETRY_TARGET
                    && slot
                        .0
                        .get()
                        .is_some_and(|configured| configured.policy.allows_span(metadata.name()))
            }))
    }

    pub(crate) fn install(&self, tracer: SdkTracer, policy: ResolvedTelemetry) -> Result<()> {
        anyhow::ensure!(
            self.0
                .set(ConfiguredTracer {
                    tracer: BoxedTracer::new(Box::new(tracer)),
                    policy
                })
                .is_ok(),
            "Standalone tracing has already been configured"
        );
        tracing::callsite::rebuild_interest_cache();
        Ok(())
    }
}

#[derive(Clone)]
struct StartupTracer(StartupTraceSlot);

impl Tracer for StartupTracer {
    type Span = BoxedSpan;

    fn build_with_context(
        &self,
        builder: SpanBuilder,
        parent: &opentelemetry::Context,
    ) -> Self::Span {
        match self.0 .0.get() {
            Some(configured) => configured.tracer.build_with_context(builder, parent),
            None => {
                BoxedTracer::new(Box::new(NoopTracer::new())).build_with_context(builder, parent)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drasi_lib::observability::{
        ExecutionContext, OpenTelemetryContextAdapter, RootSampling, TelemetryProfile,
        TelemetrySampler, TelemetrySettings, TraceContextAdapter, TraceContextV1,
    };
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::{InMemorySpanExporter, Sampler, SdkTracerProvider};
    use tracing_subscriber::{prelude::*, EnvFilter};

    fn operation() -> tracing::Span {
        tracing::info_span!(target: "drasi::telemetry", "source.publish")
    }

    #[test]
    fn startup_trace_slot_preserves_adapter_downcast_and_rebuilds_callsites() -> anyhow::Result<()>
    {
        let slot = StartupTraceSlot::new();
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .with_sampler(TelemetrySampler(Sampler::AlwaysOn))
            .build();
        let subscriber = tracing_subscriber::registry().with(slot.layer());
        let _scope = tracing::subscriber::set_default(subscriber);
        assert!(OpenTelemetryContextAdapter
            .span_context(&operation())
            .is_empty());
        slot.install(
            provider.tracer("startup"),
            TelemetrySettings::new(TelemetryProfile::Basic)
                .with_metrics(false)
                .resolve()?,
        )?;
        for sampled in [true, false] {
            let parent = ExecutionContext::new(TraceContextV1::new(
                [1; 16],
                [2; 8],
                u8::from(sampled),
                Some("vendor=value".into()),
            )?);
            let span = operation();
            OpenTelemetryContextAdapter.prepare_operation(
                &span,
                &parent,
                &[],
                RootSampling::Never,
            )?;
            let child = OpenTelemetryContextAdapter.span_context(&span);
            let child = child.trace().unwrap();
            assert_eq!(child.trace_id(), [1; 16]);
            assert_ne!(child.span_id(), [0; 8]);
            assert_eq!(child.sampled(), sampled);
            assert_eq!(child.trace_state(), Some("vendor=value"));
            {
                let _entered = span.enter();
                assert_eq!(
                    OpenTelemetryContextAdapter.current_context().trace(),
                    Some(child)
                );
            }
            assert!(OpenTelemetryContextAdapter.current_context().is_empty());
        }
        provider.force_flush()?;
        let spans = exporter.get_finished_spans()?;
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].parent_span_id.to_bytes(), [2; 8]);
        provider.shutdown()?;
        Ok(())
    }

    #[test]
    fn startup_trace_filter_is_independent_of_logging() -> anyhow::Result<()> {
        use std::{
            io::Write,
            sync::{Arc, Mutex},
        };
        #[derive(Clone)]
        struct Writer(Arc<Mutex<Vec<u8>>>);
        impl Write for Writer {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        for (logs, sampled, expected_spans) in [("off", true, 2), ("info", false, 0)] {
            let slot = StartupTraceSlot::new();
            let exporter = InMemorySpanExporter::default();
            let provider = SdkTracerProvider::builder()
                .with_simple_exporter(exporter.clone())
                .with_sampler(TelemetrySampler(if sampled {
                    Sampler::AlwaysOn
                } else {
                    Sampler::AlwaysOff
                }))
                .build();
            let writer = Writer(Default::default());
            let output = writer.0.clone();
            let subscriber = tracing_subscriber::registry().with(slot.layer()).with(
                tracing_subscriber::fmt::layer()
                    .without_time()
                    .with_ansi(false)
                    .with_writer(move || writer.clone())
                    .with_filter(EnvFilter::new(logs)),
            );
            let _scope = tracing::subscriber::set_default(subscriber);
            slot.install(
                provider.tracer("filter"),
                TelemetrySettings::new(TelemetryProfile::Debug)
                    .with_metrics(false)
                    .resolve()?,
            )?;
            drop(operation());
            drop(tracing::debug_span!(target: "drasi::telemetry", "query.evaluate"));
            drop(tracing::info_span!(target: "http", "request"));
            drop(tracing::info_span!(target: "opentelemetry", "export"));
            tracing::info!("ordinary-log-independent");
            provider.force_flush()?;
            assert_eq!(exporter.get_finished_spans()?.len(), expected_spans);
            let text = String::from_utf8(output.lock().unwrap().clone())?;
            assert_eq!(
                text.matches("ordinary-log-independent").count(),
                usize::from(logs == "info")
            );
            provider.shutdown()?;
        }
        Ok(())
    }

    #[test]
    fn startup_trace_slot_rejects_second_installation() -> anyhow::Result<()> {
        let slot = StartupTraceSlot::new();
        let provider = SdkTracerProvider::builder().build();
        let policy = TelemetrySettings::new(TelemetryProfile::Basic)
            .with_metrics(false)
            .resolve()?;
        slot.install(provider.tracer("one"), policy.clone())?;
        assert!(slot.install(provider.tracer("two"), policy).is_err());
        provider.shutdown()?;
        Ok(())
    }
}
