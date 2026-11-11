// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_lib::{
    computation::v1::{ComponentId, ComponentRole, ComponentSemanticKind, ComputationGraph},
    observability::*,
};
use metrics::{Gauge, Recorder};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_opentelemetry::{OpenTelemetryMetrics, OpenTelemetryRecorder};
use opentelemetry::{metrics::MeterProvider, trace::TracerProvider, KeyValue};
use opentelemetry_sdk::{
    metrics::{
        data::{AggregatedMetrics, Metric, MetricData, ResourceMetrics},
        InMemoryMetricExporterBuilder, PeriodicReader, SdkMeterProvider, Temporality,
    },
    trace::{BatchSpanProcessor, InMemorySpanExporter, Sampler, SdkTracerProvider},
    Resource,
};
use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;

const COMPONENT: &str = "00000000-0000-4000-8000-000000000234";
const BOUNDS: &[f64] = &[0.000001, 0.00001, 0.001, 1.0, 5.0];
const HISTOGRAM: &str = "drasi.plugin.compat.duration";

fn resource() -> Resource {
    Resource::builder_empty()
        .with_attributes([
            KeyValue::new("service.name", "drasi-server"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("service.instance.id", "compatibility-test"),
        ])
        .build()
}

fn scope() -> Result<TelemetryScope> {
    Ok(TelemetryScope::graph("compatibility", "graph")?.component(
        ComponentId::try_new(COMPONENT)?,
        ComponentRole::Transformer,
        ComponentSemanticKind::Query,
    ))
}

async fn completed_span_fidelity() -> Result<()> {
    let exporter = InMemorySpanExporter::default();
    let processor = OpenTelemetrySpanProcessor::new(
        BatchSpanProcessor::builder(exporter.clone()).build(),
        Sampler::AlwaysOn,
    );
    let provider = SdkTracerProvider::builder()
        .with_resource(resource())
        .with_sampler(processor.sampler())
        .with_span_processor(processor.clone())
        .build();
    let collector = NativeSpanCollector::new(Arc::new(processor), Default::default())?;
    let graph = ComputationGraph::builder("graph")
        .with_telemetry(
            TelemetryOptions::new(
                TelemetrySettings::new(TelemetryProfile::Debug).with_metrics(false),
            )
            .with_instance_id("compatibility")
            .with_native_span_collector(collector.clone()),
        )
        .build()?;
    let binding = graph
        .telemetry()
        .plugin_binding(scope()?, 1, "compat", None)?;
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("compatibility")));
    let host =
        tracing::subscriber::with_default(subscriber, || tracing::info_span!("host.compatibility"));
    let adapter = OpenTelemetryContextAdapter;
    adapter.prepare_operation(
        &host,
        &ExecutionContext::default(),
        &[],
        RootSampling::Always,
    )?;
    let parent = adapter
        .span_context(&host)
        .trace()
        .context("host span must have portable context")?
        .clone();
    let original = CompletedSpan {
        name: "plugin.compatibility".into(),
        target: "compatibility::plugin".into(),
        context: TraceContextV1::new(
            parent.trace_id(),
            [23; 8],
            parent.trace_flags(),
            Some("compat=preserved".into()),
        )?,
        parent_span_id: Some(parent.span_id()),
        links: vec![TelemetrySpanLink {
            context: TraceContextV1::new([41; 16], [42; 8], 1, None)?,
            attributes: vec![(
                "link.kind".into(),
                TelemetryAttribute::String("input".into()),
            )],
        }],
        start_time_unix_ns: 1_700_000_000_000_000_001,
        end_time_unix_ns: 1_700_000_000_000_000_999,
        status: TelemetrySpanStatus::Error,
        attributes: vec![("decoded".into(), TelemetryAttribute::I64(7))],
        events: vec![TelemetrySpanEvent {
            name: "decode".into(),
            time_unix_ns: 1_700_000_000_000_000_500,
            attributes: vec![("valid".into(), TelemetryAttribute::Bool(true))],
        }],
    };
    assert_eq!(
        binding.submit_span(&original),
        TelemetrySubmission::Accepted
    );
    drop(host);
    collector.flush(Duration::from_secs(3)).await?;
    provider.force_flush()?;
    let spans = exporter.get_finished_spans()?;
    assert_eq!(
        spans.len(),
        2,
        "host and completed plugin spans export once"
    );
    let native = spans
        .iter()
        .find(|span| span.name == original.name)
        .context("completed plugin span was not exported")?;
    assert_eq!(
        native.span_context.trace_id().to_bytes(),
        original.context.trace_id()
    );
    assert_eq!(
        native.span_context.span_id().to_bytes(),
        original.context.span_id()
    );
    assert_eq!(
        native.span_context.trace_flags().to_u8(),
        original.context.trace_flags()
    );
    assert_eq!(
        native.span_context.trace_state().header(),
        "compat=preserved"
    );
    assert_eq!(native.parent_span_id.to_bytes(), parent.span_id());
    assert_eq!(
        native.start_time,
        UNIX_EPOCH + Duration::from_nanos(original.start_time_unix_ns)
    );
    assert_eq!(
        native.end_time,
        UNIX_EPOCH + Duration::from_nanos(original.end_time_unix_ns)
    );
    assert!(matches!(
        native.status,
        opentelemetry::trace::Status::Error { .. }
    ));
    assert_eq!(native.links.links.len(), 1);
    assert_eq!(
        native.links.links[0].span_context.trace_id().to_bytes(),
        [41; 16]
    );
    assert_eq!(
        native.links.links[0].span_context.span_id().to_bytes(),
        [42; 8]
    );
    assert_eq!(
        native.links.links[0].attributes,
        vec![KeyValue::new("link.kind", "input")]
    );
    assert_eq!(native.events.events.len(), 1);
    assert_eq!(native.events.events[0].name, "decode");
    assert_eq!(
        native.events.events[0].timestamp,
        UNIX_EPOCH + Duration::from_nanos(original.events[0].time_unix_ns)
    );
    assert_eq!(
        native.events.events[0].attributes,
        vec![KeyValue::new("valid", true)]
    );
    for attribute in [
        KeyValue::new("decoded", 7_i64),
        KeyValue::new("instance_id", "compatibility"),
        KeyValue::new("component_id", COMPONENT),
    ] {
        assert!(native.attributes.contains(&attribute));
    }
    assert_eq!(native.instrumentation_scope.name(), original.target);
    drop(binding);
    drop(graph);
    collector.shutdown(Duration::from_secs(3)).await?;
    // The collector borrows the processor: it must not shut down the provider.
    let tracer = provider.tracer("still-owned-by-caller");
    drop(opentelemetry::trace::Tracer::start(
        &tracer,
        "after-collector-shutdown",
    ));
    provider.force_flush()?;
    assert_eq!(exporter.get_finished_spans()?.len(), 3);
    provider.shutdown()?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn host_and_completed_spans_preserve_fidelity_on_current_thread() -> Result<()> {
    completed_span_fidelity().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_and_completed_spans_preserve_fidelity_on_multiple_threads() -> Result<()> {
    completed_span_fidelity().await
}

fn metric_workload(recorder: Arc<dyn Recorder + Send + Sync>) -> Result<(ComputationGraph, Gauge)> {
    let context = MetricRegistrationContext::new(recorder, Default::default())?;
    let settings = TelemetrySettings::new(TelemetryProfile::Debug)
        .with_tracing(false)
        .with_sampling(RootSampling::Never)
        .exclude_metrics("excluded");
    let policy = settings.resolve()?;
    let graph = ComputationGraph::builder("graph")
        .with_telemetry(
            TelemetryOptions::new(settings)
                .with_instance_id("compatibility")
                .with_metric_context(context.clone()),
        )
        .build()?;
    let raw = context
        .plugin(
            &policy,
            &scope()?,
            "compat",
            PluginMetricDefinition {
                leaf: "raw",
                kind: MetricKind::Counter,
                unit: MetricUnit::One,
                description: "Raw facade counter",
            },
            [],
        )?
        .counter()?;
    raw.increment(3);
    raw.absolute(10);
    raw.increment(2);
    raw.absolute(8);
    raw.absolute(10);

    let old = graph
        .telemetry()
        .plugin_binding(scope()?, 1, "compat", None)?;
    let old_counter = old
        .metrics()
        .counter("replaced", MetricUnit::One, "Replacement total")?;
    let old_gauge = old
        .metrics()
        .gauge("queued", MetricUnit::One, "Queued work")?;
    old_counter.absolute(100);
    old_gauge.set(99.0);
    let next = graph
        .telemetry()
        .plugin_binding(scope()?, 2, "compat", None)?;
    let counter = next
        .metrics()
        .counter("replaced", MetricUnit::One, "Replacement total")?;
    counter.absolute(10);
    counter.absolute(10);
    counter.absolute(8);
    old_counter.increment(1_000);
    let gauge = next
        .metrics()
        .gauge("queued", MetricUnit::One, "Queued work")?;
    gauge.set(10.0);
    gauge.increment(3.0);
    gauge.decrement(4.0);
    old_gauge.set(1_000.0);
    let histogram =
        next.metrics()
            .histogram("duration", MetricUnit::Seconds, "Duration in seconds")?;
    for seconds in [0.000001, 0.0005, 2.0] {
        histogram.record(seconds);
    }
    next.metrics()
        .counter("excluded", MetricUnit::One, "Excluded")?
        .increment(1);
    Ok((graph, gauge))
}

fn metric<'a>(exports: &'a [ResourceMetrics], name: &str) -> Result<&'a Metric> {
    let values = exports
        .iter()
        .flat_map(ResourceMetrics::scope_metrics)
        .flat_map(|scope| scope.metrics())
        .filter(|metric| metric.name() == name)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        values.len() == 1,
        "expected exactly one metric '{name}', got {}",
        values.len()
    );
    Ok(values[0])
}

#[test]
fn otel_recorder_preserves_facade_semantics_units_and_retirement() -> Result<()> {
    let exporter = InMemoryMetricExporterBuilder::new()
        .with_temporality(Temporality::Cumulative)
        .build();
    let provider = SdkMeterProvider::builder()
        .with_resource(resource())
        .with_reader(
            PeriodicReader::builder(exporter.clone())
                .with_interval(Duration::from_secs(3_600))
                .build(),
        )
        .build();
    let recorder =
        OpenTelemetryRecorder::new(OpenTelemetryMetrics::new(provider.meter("compatibility")));
    recorder.set_histogram_bounds([HISTOGRAM].into_iter(), BOUNDS);
    let (graph, gauge) = metric_workload(Arc::new(recorder))?;
    provider.force_flush()?;
    let exports = exporter.get_finished_metrics()?;
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].resource(), &resource());
    for (name, expected) in [("raw", 12), ("replaced", 110)] {
        let value = metric(&exports, &format!("drasi.plugin.compat.{name}"))?;
        assert_eq!(value.unit(), "1");
        let AggregatedMetrics::U64(MetricData::Sum(sum)) = value.data() else {
            anyhow::bail!("counter did not export as a u64 sum");
        };
        assert!(sum.is_monotonic());
        assert_eq!(sum.temporality(), Temporality::Cumulative);
        let points = sum.data_points().collect::<Vec<_>>();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].value(), expected);
        assert!(points[0]
            .attributes()
            .any(|value| value == &KeyValue::new("component_id", COMPONENT)));
    }
    let AggregatedMetrics::F64(MetricData::Gauge(values)) =
        metric(&exports, "drasi.plugin.compat.queued")?.data()
    else {
        anyhow::bail!("gauge did not export as an f64 gauge");
    };
    assert_eq!(
        values
            .data_points()
            .map(|point| point.value())
            .collect::<Vec<_>>(),
        vec![9.0]
    );
    let histogram = metric(&exports, HISTOGRAM)?;
    assert_eq!(histogram.unit(), "s");
    let AggregatedMetrics::F64(MetricData::Histogram(values)) = histogram.data() else {
        anyhow::bail!("duration did not export as an f64 explicit histogram");
    };
    let points = values.data_points().collect::<Vec<_>>();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].count(), 3);
    assert!((points[0].sum() - 2.000501).abs() < 1e-12);
    assert_eq!(points[0].bounds().collect::<Vec<_>>(), BOUNDS);
    assert_eq!(
        points[0].bucket_counts().collect::<Vec<_>>(),
        vec![1, 0, 1, 0, 1, 0]
    );
    assert!(metric(&exports, "drasi.plugin.compat.excluded").is_err());
    drop(graph);
    gauge.set(999.0);
    exporter.reset();
    provider.force_flush()?;
    let retired = exporter.get_finished_metrics()?;
    let AggregatedMetrics::F64(MetricData::Gauge(values)) =
        metric(&retired, "drasi.plugin.compat.queued")?.data()
    else {
        anyhow::bail!("retired gauge changed type");
    };
    assert_eq!(
        values
            .data_points()
            .map(|point| point.value())
            .collect::<Vec<_>>(),
        vec![0.0]
    );
    provider.shutdown()?;
    Ok(())
}

fn prometheus_value(text: &str, name: &str) -> Result<f64> {
    prometheus_value_with_labels(text, name, &[])
}

fn prometheus_value_with_labels(text: &str, name: &str, labels: &[(&str, &str)]) -> Result<f64> {
    let values = text
        .lines()
        .filter(|line| line.starts_with(&format!("{name}{{")))
        .filter(|line| {
            labels
                .iter()
                .all(|(key, value)| line.contains(&format!("{key}=\"{value}\"")))
        })
        .map(|line| {
            line.rsplit_once(' ')
                .context("Prometheus sample has no value")?
                .1
                .parse::<f64>()
                .context("invalid Prometheus value")
        })
        .collect::<Result<Vec<_>>>()?;
    anyhow::ensure!(
        values.len() == 1,
        "expected one Prometheus sample for '{name}', got {}",
        values.len()
    );
    Ok(values[0])
}

#[test]
fn prometheus_recorder_preserves_the_same_facade_contract() -> Result<()> {
    let recorder = PrometheusBuilder::new()
        .set_buckets(BOUNDS)?
        .build_recorder();
    let handle = recorder.handle();
    let (graph, gauge) = metric_workload(Arc::new(recorder))?;
    let text = handle.render();
    assert_eq!(prometheus_value(&text, "drasi_plugin_compat_raw")?, 12.0);
    assert_eq!(
        prometheus_value(&text, "drasi_plugin_compat_replaced")?,
        110.0
    );
    assert_eq!(prometheus_value(&text, "drasi_plugin_compat_queued")?, 9.0);
    assert_eq!(
        prometheus_value(&text, "drasi_plugin_compat_duration_count")?,
        3.0
    );
    assert!(
        (prometheus_value(&text, "drasi_plugin_compat_duration_sum")? - 2.000501).abs() < 1e-12
    );
    assert!(!text.contains("drasi_plugin_compat_excluded"));
    for (bound, count) in [("0.000001", "1"), ("0.001", "2"), ("5", "3"), ("+Inf", "3")] {
        assert!(text.lines().any(|line| {
            line.starts_with("drasi_plugin_compat_duration_bucket{")
                && line.contains(&format!("le=\"{bound}\""))
                && line.ends_with(&format!(" {count}"))
        }));
    }
    drop(graph);
    gauge.set(999.0);
    assert_eq!(
        prometheus_value(&handle.render(), "drasi_plugin_compat_queued")?,
        0.0
    );
    Ok(())
}

async fn rebuilt_native_query_workload(
    telemetry: TelemetryOptions,
    logging: ComponentLogRuntime,
) -> Result<String> {
    use drasi_server::{dynamic_loading::load_plugins, PluginRegistry, Query};
    use drasi_source_application::{
        ApplicationSource, ApplicationSourceConfig, PropertyMapBuilder,
    };
    use std::{collections::HashSet, path::PathBuf};

    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/plugins");
    let names = [
        "drasi_source_mock",
        "drasi_reaction_log",
        "drasi_computation_transformer_query",
    ]
    .map(|name| {
        format!(
            "{}{name}{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        )
    });
    for name in &names {
        anyhow::ensure!(
            directory.join(name).is_file(),
            "rebuild the local plugin bundle: missing {name}"
        );
    }
    let allowed = HashSet::from(names);
    let mut registry = PluginRegistry::new();
    let loaded = load_plugins(&directory, &mut registry, None, Some(&allowed))?;
    assert_eq!(loaded.plugins_failed, 0, "{:?}", loaded.failures);
    assert_eq!(loaded.plugins_loaded, 3);
    assert!(loaded.source_descriptors >= 1);
    assert!(loaded.reaction_descriptors >= 1);
    let factory = registry
        .query_factory()?
        .context("rebuilt native query factory not registered")?;
    let handle = Arc::new(std::sync::OnceLock::new());
    let assigned = handle.clone();
    let core = drasi_lib::DrasiLib::builder()
        .with_id("plugin-baseline")
        .with_telemetry(telemetry.with_instance_id("plugin-baseline"))
        .with_component_logging(logging.clone())
        .with_query_factory(factory)
        .with_source_factory("input", true, move |id| {
            let (source, input) = ApplicationSource::new(
                id.as_str(),
                ApplicationSourceConfig {
                    properties: Default::default(),
                    durability: None,
                },
            )?;
            assigned
                .set(input)
                .map_err(|_| anyhow::anyhow!("source constructed twice"))?;
            Ok(source)
        })
        .with_query(
            Query::cypher("items")
                .query("MATCH (n:Item) RETURN n.value AS value")
                .from_input("input")
                .enable_bootstrap(false)
                .auto_start(true)
                .build()?,
        )
        .build()
        .await
        .context("build the native query workload")?;
    core.start()
        .await
        .context("start the native query workload")?;
    let query = core.component_by_name("items")?;
    tokio::time::timeout(Duration::from_secs(10), query.wait_started()).await??;
    handle
        .get()
        .context("source did not publish its input handle")?
        .send_node_insert(
            "one",
            vec!["Item"],
            PropertyMapBuilder::new().with_integer("value", 7).build(),
        )
        .await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if core.get_query_results_by_id(query.id()).await?
                == vec![serde_json::json!({"value": 7})]
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    let query_id = query.id().to_string();
    core.shutdown()
        .await
        .context("shut down the native query workload")?;
    Ok(query_id)
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires make build-local-test-plugins"]
async fn rebuilt_native_query_metrics_reach_otel_without_traces() -> Result<()> {
    let exporter = InMemoryMetricExporterBuilder::new()
        .with_temporality(Temporality::Cumulative)
        .build();
    let provider = SdkMeterProvider::builder()
        .with_resource(resource())
        .with_reader(
            PeriodicReader::builder(exporter.clone())
                .with_interval(Duration::from_secs(3_600))
                .build(),
        )
        .build();
    let recorder =
        OpenTelemetryRecorder::new(OpenTelemetryMetrics::new(provider.meter("native-query")));
    let logging = ComponentLogRuntime::new(LogPolicy::new("off")?)?;
    let subscriber = tracing_subscriber::registry().with(logging.layer());
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        rebuilt_native_query_workload(
            TelemetryOptions::new(
                TelemetrySettings::new(TelemetryProfile::Persistence)
                    .with_tracing(false)
                    .with_sampling(RootSampling::Never)
                    .require_native_query_observations(),
            )
            .with_metric_context(MetricRegistrationContext::new(
                Arc::new(recorder),
                Default::default(),
            )?),
            logging.clone(),
        )
        .with_subscriber(subscriber),
    )
    .await;
    logging.shutdown(Duration::from_secs(3)).await?;
    provider.force_flush()?;
    let exports = exporter.get_finished_metrics()?;
    provider.shutdown()?;
    let query_id = result??;
    for name in [
        "drasi.query.events_processed",
        "drasi.query.results_emitted",
    ] {
        let value = metric(&exports, name)?;
        let AggregatedMetrics::U64(MetricData::Sum(sum)) = value.data() else {
            anyhow::bail!("{name} did not export as a u64 sum");
        };
        assert_eq!(
            sum.data_points().map(|point| point.value()).sum::<u64>(),
            1,
            "{name} must count one input/result, not host and plugin copies"
        );
        let active = sum
            .data_points()
            .filter(|point| point.value() != 0)
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1, "{name}");
        for attribute in [
            KeyValue::new("instance_id", "plugin-baseline"),
            KeyValue::new("component_id", query_id.clone()),
            KeyValue::new("query_id", query_id.clone()),
        ] {
            assert!(active[0].attributes().any(|value| value == &attribute));
        }
        let dimension = if name == "drasi.query.events_processed" {
            KeyValue::new("phase", "steady")
        } else {
            KeyValue::new("change_kind", "add")
        };
        assert!(active[0].attributes().any(|value| value == &dimension));
    }
    // Evaluation/publication and delivery acknowledgement each commit once.
    for (name, expected) in [
        ("drasi.query.engine.duration", 1),
        ("drasi.query.commit.duration", 2),
    ] {
        let value = metric(&exports, name)?;
        assert_eq!(value.unit(), "s", "{name}");
        let AggregatedMetrics::F64(MetricData::Histogram(histogram)) = value.data() else {
            anyhow::bail!("{name} did not export as an f64 histogram");
        };
        assert_eq!(
            histogram
                .data_points()
                .map(|point| point.count())
                .sum::<u64>(),
            expected,
            "{name} must measure the actual work exactly once"
        );
        let active = histogram
            .data_points()
            .filter(|point| point.count() != 0)
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1, "{name}");
        assert!(active[0].sum().is_finite() && active[0].sum() >= 0.0);
        for attribute in [
            KeyValue::new("query_id", query_id.clone()),
            KeyValue::new("phase", "steady"),
        ] {
            assert!(active[0].attributes().any(|value| value == &attribute));
        }
        if name == "drasi.query.commit.duration" {
            assert!(active[0]
                .attributes()
                .any(|value| value == &KeyValue::new("outcome", "success")));
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires make build-local-test-plugins"]
async fn rebuilt_native_query_phases_reach_the_shared_trace_processor() -> Result<()> {
    let exporter = InMemorySpanExporter::default();
    let processor = OpenTelemetrySpanProcessor::new(
        BatchSpanProcessor::builder(exporter.clone()).build(),
        Sampler::AlwaysOn,
    );
    let provider = SdkTracerProvider::builder()
        .with_resource(resource())
        .with_sampler(processor.sampler())
        .with_span_processor(processor.clone())
        .build();
    let collector = NativeSpanCollector::new(Arc::new(processor), Default::default())?;
    let recorder = PrometheusBuilder::new().build_recorder();
    let metrics = recorder.handle();
    let logging = ComponentLogRuntime::new(LogPolicy::new("off")?)?;
    let subscriber = tracing_subscriber::registry()
        .with(logging.layer())
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("native-query")));
    // Managed reconciliation uses the process subscriber, as in standalone Server.
    tracing::subscriber::set_global_default(subscriber)?;
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        rebuilt_native_query_workload(
            TelemetryOptions::new(
                TelemetrySettings::new(TelemetryProfile::Debug).require_native_query_observations(),
            )
            .with_metric_context(MetricRegistrationContext::new(
                Arc::new(recorder),
                Default::default(),
            )?)
            .with_context_adapter(Arc::new(OpenTelemetryContextAdapter))
            .with_native_span_collector(collector.clone()),
            logging.clone(),
        ),
    )
    .await;
    logging.shutdown(Duration::from_secs(3)).await?;
    collector.shutdown(Duration::from_secs(3)).await?;
    provider.force_flush()?;
    let spans = exporter.get_finished_spans()?;
    provider.shutdown()?;
    let query_id = result??;
    let evaluation = spans
        .iter()
        .find(|span| span.name == "query.evaluate")
        .context("real native evaluator span was not exported")?;
    let confirmation = spans
        .iter()
        .find(|span| {
            span.name == "query.confirm_outputs"
                && span.parent_span_id == evaluation.parent_span_id
                && span.span_context.trace_id() == evaluation.span_context.trace_id()
        })
        .context("query delivery acknowledgement span was not exported")?;
    assert!(spans.iter().any(|span| {
        span.name == "query.process"
            && span.span_context.span_id() == evaluation.parent_span_id
            && span.span_context.trace_id() == evaluation.span_context.trace_id()
    }));
    for name in [
        "query.evaluate",
        "query.commit",
        "query.prepare_input",
        "query.prepare_output",
        "query.finalize",
    ] {
        let phases = spans
            .iter()
            .filter(|span| span.name == name)
            .collect::<Vec<_>>();
        let expected = if name == "query.commit" { 2 } else { 1 };
        assert_eq!(phases.len(), expected, "{name}");
        for phase in phases {
            if name == "query.commit" {
                assert!(
                    phase.parent_span_id == evaluation.parent_span_id
                        || phase.parent_span_id == confirmation.span_context.span_id(),
                    "{name} must belong to evaluation or delivery acknowledgement"
                );
            } else {
                assert_eq!(phase.parent_span_id, evaluation.parent_span_id, "{name}");
            }
            assert_eq!(
                phase.span_context.trace_id(),
                evaluation.span_context.trace_id(),
                "{name}"
            );
            assert!(phase.end_time >= phase.start_time, "{name}");
            assert!(phase
                .attributes
                .contains(&KeyValue::new("component_id", query_id.clone())));
        }
    }
    for parent in [
        evaluation.parent_span_id,
        confirmation.span_context.span_id(),
    ] {
        assert_eq!(
            spans
                .iter()
                .filter(|span| span.name == "query.commit" && span.parent_span_id == parent)
                .count(),
            1,
            "evaluation and acknowledgement must each attribute one real commit"
        );
    }
    assert!(!format!("{spans:?}").contains("MATCH (n:Item)"));
    let text = metrics.render();
    for (name, labels) in [
        ("drasi_query_events_processed", ("phase", "steady")),
        ("drasi_query_results_emitted", ("change_kind", "add")),
        ("drasi_query_engine_duration_count", ("phase", "steady")),
    ] {
        assert_eq!(
            prometheus_value_with_labels(&text, name, &[("query_id", query_id.as_str()), labels],)?,
            1.0,
            "{name}"
        );
    }
    assert_eq!(
        prometheus_value_with_labels(
            &text,
            "drasi_query_engine_duration_count",
            &[("query_id", query_id.as_str()), ("phase", "bootstrap")],
        )?,
        0.0
    );
    assert!(
        !text.contains("drasi_query_commit_duration"),
        "Debug spans must not enable Persistence-only commit histograms"
    );
    Ok(())
}
