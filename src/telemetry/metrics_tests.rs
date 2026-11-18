// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::process::ProcessMetrics;
use super::prometheus::{fixtures, scrape_router, PrometheusRuntime};
use super::recorders::prometheus_recorder;
use crate::server::{CleanupPhase, ServerCleanupHandle};
use axum::{
    http::{header::CONTENT_TYPE, StatusCode},
    response::Response,
};
use drasi_lib::{
    computation::v1::{ComponentId, ComponentRole, ComponentSemanticKind},
    observability::{
        MetricKind, MetricRegistrationContext, MetricUnit, PluginMetricDefinition,
        RegisteredMetric, TelemetryError, TelemetryProfile, TelemetryScope, TelemetrySettings,
    },
};
use metrics_process::collector::Metrics;
use std::sync::{Arc, Barrier};
use std::time::Duration;
use tokio::time::{timeout, Instant};
use tower::ServiceExt;

mod trace_transport {
    use crate::{api::mappings::DtoMapper, config::telemetry::TelemetryConfig};
    use opentelemetry_http::HttpClient;
    use opentelemetry_proto::tonic::collector::trace::v1::{
        ExportTracePartialSuccess, ExportTraceServiceResponse,
    };
    use prost::Message;
    use std::time::Duration;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    async fn send_response(
        response: Vec<u8>,
        deadline: Option<tokio::time::Instant>,
    ) -> anyhow::Result<(super::super::transport::TraceExportResponse, Vec<u8>)> {
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/custom/traces"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(response))
            .mount(&receiver)
            .await;
        let config: TelemetryConfig = serde_json::from_value(serde_json::json!({
            "tracing": {
                "endpoint": format!("{}/custom/traces", receiver.uri()),
                "protocol": "http/protobuf"
            }
        }))?;
        let destination = config
            .resolve(&DtoMapper::new(), 8080, true)?
            .tracing
            .unwrap();
        tokio::task::spawn_blocking(move || {
            let client = crate::telemetry::CoreHttpClient::new(
                destination,
                Duration::from_secs(2),
                crate::telemetry::OtlpSignal::Traces,
            )?;
            client.allow_until(deadline)?;
            let request = opentelemetry_http::Request::builder()
                .uri("http://unused.invalid/ignored")
                .body(bytes::Bytes::new())?;
            let response = futures::executor::block_on(client.send_bytes(request))
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            Ok((client.take_trace_response()?, response.into_body().to_vec()))
        })
        .await?
    }

    #[tokio::test]
    async fn trace_transport_rejects_malformed_nonempty_success() {
        assert!(send_response(vec![0xff], None).await.is_err());
    }

    #[tokio::test]
    async fn trace_transport_rejects_negative_rejection_count() {
        // ExportTraceServiceResponse.partial_success.rejected_spans = -1.
        let response = vec![
            0x0a, 0x0b, 0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
        ];
        assert!(send_response(response, None).await.is_err());
    }

    #[tokio::test]
    async fn trace_transport_observes_partial_rejection_without_echoing_text() {
        let body = ExportTraceServiceResponse {
            partial_success: Some(ExportTracePartialSuccess {
                rejected_spans: 2,
                error_message: "collector-response-secret".into(),
            }),
        }
        .encode_to_vec();
        let (response, sdk_body) = send_response(body, None).await.unwrap();
        assert_eq!(response.rejected_spans, 2);
        assert!(response.has_warning);
        assert!(!format!("{response:?}").contains("collector-response-secret"));
        assert!(
            sdk_body.is_empty(),
            "collector text must not reach SDK diagnostics"
        );
        let (response, _) = send_response(Vec::new(), None).await.unwrap();
        assert_eq!(response.rejected_spans, 0);
        assert!(!response.has_warning);
    }

    #[tokio::test]
    async fn trace_transport_preserves_resolved_destination_and_deadline() {
        assert!(send_response(Vec::new(), None).await.is_ok());
        assert!(send_response(Vec::new(), Some(tokio::time::Instant::now()))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn trace_transport_never_follows_redirects() -> anyhow::Result<()> {
        let target = MockServer::start().await;
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("{}/custom/traces", target.uri())),
            )
            .mount(&receiver)
            .await;
        let config: TelemetryConfig = serde_json::from_value(serde_json::json!({
            "tracing": {
                "endpoint": format!("{}/custom/traces", receiver.uri()),
                "protocol": "http/protobuf"
            }
        }))?;
        let destination = config
            .resolve(&DtoMapper::new(), 8080, true)?
            .tracing
            .unwrap();
        let status = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let client = crate::telemetry::CoreHttpClient::new(
                destination,
                Duration::from_secs(2),
                crate::telemetry::OtlpSignal::Traces,
            )?;
            client.allow_until(None)?;
            let request = opentelemetry_http::Request::builder()
                .uri("http://unused.invalid/ignored")
                .body(bytes::Bytes::new())?;
            Ok(futures::executor::block_on(client.send_bytes(request))
                .map_err(|error| anyhow::anyhow!("{error}"))?
                .status())
        })
        .await??;
        assert_eq!(status, 307);
        assert!(target.received_requests().await.unwrap().is_empty());
        Ok(())
    }
}

mod otlp {
    use super::*;
    use crate::{
        api::mappings::DtoMapper,
        config::telemetry::TelemetryConfig,
        instance_registry::InstanceRegistry,
        telemetry::{
            process::SnapshotSource, recorders::telemetry_resource, MetricsMode, MetricsRuntime,
        },
    };
    use opentelemetry_proto::tonic::{
        collector::metrics::v1::ExportMetricsServiceRequest,
        common::v1::{any_value::Value, KeyValue},
        metrics::v1::{metric::Data, number_data_point::Value as Number, Metric},
    };
    use prost::Message;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn mode(endpoint: &str, progress_enabled: bool) -> MetricsMode {
        configured_mode(endpoint, progress_enabled, None)
    }

    fn mode_with_process(endpoint: &str, source: SnapshotSource) -> MetricsMode {
        configured_mode(endpoint, true, Some(source))
    }

    fn configured_mode(
        endpoint: &str,
        progress_enabled: bool,
        process: Option<SnapshotSource>,
    ) -> MetricsMode {
        let config: TelemetryConfig = serde_json::from_value(serde_json::json!({
            "filters":{"exclude":if progress_enabled { Vec::<&str>::new() } else { vec!["drasi.query.progress."] }},
            "metrics":{"backend":"otlp","otlp":{
                "endpoint":endpoint,"protocol":"http/protobuf"
            }}
        })).unwrap();
        let effective = config.resolve(&DtoMapper::new(), 8080, true).unwrap();
        let progress_enabled = effective
            .policy()
            .allows_metric("drasi.query.progress.sequence");
        let process = process.map(|source| ProcessMetrics::with_source(effective.policy(), source));
        MetricsMode::CoreOtlp {
            destination: effective.metrics_otlp.unwrap(),
            export_interval: Duration::from_secs(1),
            export_timeout: Duration::from_secs(1),
            resource: telemetry_resource("wire-test"),
            progress_enabled,
            process,
        }
    }

    async fn collector() -> MockServer {
        let collector = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/custom/metrics"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::<u8>::new()))
            .mount(&collector)
            .await;
        collector
    }

    async fn capture(collector: &MockServer) -> ExportMetricsServiceRequest {
        timeout(Duration::from_secs(4), async {
            loop {
                let requests = collector.received_requests().await.unwrap();
                if let Some(request) = requests.first() {
                    assert_eq!(request.url.path(), "/custom/metrics");
                    assert_eq!(request.headers["content-type"], "application/x-protobuf");
                    return ExportMetricsServiceRequest::decode(request.body.as_slice()).unwrap();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }

    fn metric<'a>(body: &'a ExportMetricsServiceRequest, scope: &str, name: &str) -> &'a Metric {
        let matches: Vec<_> = body
            .resource_metrics
            .iter()
            .flat_map(|resource| &resource.scope_metrics)
            .filter(|metrics| metrics.scope.as_ref().unwrap().name == scope)
            .flat_map(|scope| &scope.metrics)
            .filter(|metric| metric.name == name)
            .collect();
        assert_eq!(matches.len(), 1, "expected exactly one {scope}/{name}");
        matches[0]
    }

    fn attribute<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a Value> {
        attributes
            .iter()
            .find(|attribute| attribute.key == key)
            .and_then(|attribute| attribute.value.as_ref())
            .and_then(|value| value.value.as_ref())
    }

    fn decode(request: &wiremock::Request) -> ExportMetricsServiceRequest {
        ExportMetricsServiceRequest::decode(request.body.as_slice()).unwrap()
    }

    fn has_metric(body: &ExportMetricsServiceRequest, name: &str) -> bool {
        body.resource_metrics
            .iter()
            .flat_map(|resource| &resource.scope_metrics)
            .flat_map(|scope| &scope.metrics)
            .any(|metric| metric.name == name)
    }

    fn process_cpu(body: &ExportMetricsServiceRequest) -> f64 {
        let Data::Sum(sum) = metric(body, "drasi.server.process", "process_cpu_seconds_total")
            .data
            .as_ref()
            .unwrap()
        else {
            panic!("process CPU must be a sum")
        };
        assert_eq!(sum.data_points.len(), 1);
        match sum.data_points[0].value {
            Some(Number::AsDouble(value)) => value,
            ref other => panic!("process CPU must be a double, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn core_wire_exports_process_metrics_once_per_collection_with_shared_resource() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let collector = collector().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let source: SnapshotSource = Arc::new(move || {
            let call = counted.fetch_add(1, Ordering::SeqCst) + 1;
            Metrics {
                cpu_seconds_total: Some(0.5 * call as f64),
                resident_memory_bytes: Some(4096),
                open_fds: Some(7),
                threads: Some(3),
                ..Default::default()
            }
        });
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode_with_process(&format!("{}/custom/metrics", collector.uri()), source),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .expect("Core registration context");
        register(
            &context,
            "events",
            MetricKind::Counter,
            MetricUnit::One,
            &[],
        )
        .unwrap()
        .counter()
        .unwrap()
        .increment(1);
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(collector.received_requests().await.unwrap().is_empty());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "preparation must not sample"
        );
        runtime.activate().unwrap();
        let body = capture(&collector).await;
        let cpu = metric(&body, "drasi.server.process", "process_cpu_seconds_total");
        assert_eq!(cpu.unit, "s");
        let Data::Sum(sum) = cpu.data.as_ref().unwrap() else {
            panic!("process CPU must be a sum")
        };
        assert!(sum.is_monotonic);
        assert_eq!(sum.aggregation_temporality, 2);
        assert_eq!(sum.data_points.len(), 1);
        assert!(sum.data_points[0].attributes.is_empty());
        assert_eq!(sum.data_points[0].value, Some(Number::AsDouble(0.5)));
        for (name, unit, value) in [
            ("process_resident_memory_bytes", "By", 4096.0),
            ("process_open_fds", "{file_descriptor}", 7.0),
            ("process_threads", "{thread}", 3.0),
        ] {
            let exported = metric(&body, "drasi.server.process", name);
            assert_eq!(exported.unit, unit, "{name}");
            let Data::Gauge(gauge) = exported.data.as_ref().unwrap() else {
                panic!("{name} must be a gauge")
            };
            assert_eq!(gauge.data_points.len(), 1, "{name}");
            assert!(gauge.data_points[0].attributes.is_empty(), "{name}");
            assert_eq!(gauge.data_points[0].value, Some(Number::AsDouble(value)));
        }
        for absent in [
            "process_virtual_memory_bytes",
            "process_max_fds",
            "process_start_time_seconds",
        ] {
            assert!(
                !has_metric(&body, absent),
                "{absent} exported without a reading"
            );
        }
        assert_eq!(
            body.resource_metrics.len(),
            1,
            "process metrics must share the provider resource"
        );
        let scopes: Vec<_> = body.resource_metrics[0]
            .scope_metrics
            .iter()
            .map(|scope| scope.scope.as_ref().unwrap().name.as_str())
            .collect();
        assert!(
            scopes.contains(&"drasi.core") && scopes.contains(&"drasi.server.process"),
            "{scopes:?}"
        );
        let resource = body.resource_metrics[0].resource.as_ref().unwrap();
        assert_eq!(
            attribute(&resource.attributes, "service.name"),
            Some(&Value::StringValue("wire-test".into()))
        );
        runtime.shutdown().await.unwrap();
        let requests = collector.received_requests().await.unwrap();
        assert!(requests.len() >= 2, "the final collection must export");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            requests.len(),
            "exactly one process snapshot per collection"
        );
        assert_eq!(
            process_cpu(&decode(requests.last().unwrap())),
            0.5 * requests.len() as f64
        );
    }

    #[tokio::test]
    async fn process_reading_missing_in_a_cycle_is_not_exported() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let collector = collector().await;
        let calls = Arc::new(AtomicUsize::new(0));
        // One snapshot per collection: open fds only in the first, CPU only in
        // the first two, and resident memory in every one.
        let source: SnapshotSource = Arc::new(move || {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            Metrics {
                cpu_seconds_total: (call < 2).then_some(1.0),
                resident_memory_bytes: Some(4096),
                open_fds: (call == 0).then_some(7),
                ..Default::default()
            }
        });
        let mut runtime = MetricsRuntime::default();
        runtime
            .prepare(
                mode_with_process(&format!("{}/custom/metrics", collector.uri()), source),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap();
        runtime.activate().unwrap();
        let requests = timeout(Duration::from_secs(10), async {
            loop {
                let requests = collector.received_requests().await.unwrap();
                if requests.len() >= 3 {
                    return requests;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let (first, second, third) = (
            decode(&requests[0]),
            decode(&requests[1]),
            decode(&requests[2]),
        );
        let Data::Gauge(gauge) = metric(&first, "drasi.server.process", "process_open_fds")
            .data
            .as_ref()
            .unwrap()
        else {
            panic!("open fds must be a gauge")
        };
        assert_eq!(gauge.data_points[0].value, Some(Number::AsDouble(7.0)));
        assert_eq!(process_cpu(&second), 1.0);
        assert!(
            !has_metric(&second, "process_open_fds"),
            "a missing reading must not re-export the previous value"
        );
        assert!(has_metric(&third, "process_resident_memory_bytes"));
        assert!(
            !has_metric(&third, "process_cpu_seconds_total"),
            "a missing CPU reading must not re-export the previous cumulative value"
        );
        runtime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn core_wire_is_cumulative_preserves_units_bounds_and_activation_barrier() {
        let collector = collector().await;
        let registry = InstanceRegistry::new();
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &registry,
            )
            .await
            .unwrap()
            .expect("Core registration context");
        let counter = register(
            &context,
            "events",
            MetricKind::Counter,
            MetricUnit::One,
            &[],
        )
        .unwrap()
        .counter()
        .unwrap();
        counter.absolute(100);
        drop(counter);
        register(
            &context,
            "events",
            MetricKind::Counter,
            MetricUnit::One,
            &[],
        )
        .unwrap()
        .counter()
        .unwrap()
        .absolute(110);
        register(&context, "depth", MetricKind::Gauge, MetricUnit::One, &[])
            .unwrap()
            .gauge()
            .unwrap()
            .set(7.0);
        for (name, unit) in [("latency", MetricUnit::Seconds), ("count", MetricUnit::One)] {
            let histogram = register(&context, name, MetricKind::Histogram, unit, &[])
                .unwrap()
                .histogram()
                .unwrap();
            histogram.record(0.000003);
            histogram.record(2.0);
        }
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(collector.received_requests().await.unwrap().is_empty());
        runtime.activate().unwrap();
        assert!(
            runtime.activate().is_err(),
            "activation must not create another loop"
        );
        let body = capture(&collector).await;
        let Data::Sum(sum) = metric(&body, "drasi.core", "drasi.plugin.probe.events")
            .data
            .as_ref()
            .unwrap()
        else {
            panic!("counter must be a sum")
        };
        assert_eq!(sum.aggregation_temporality, 2);
        assert!(sum.is_monotonic);
        assert!(matches!(sum.data_points[0].value, Some(Number::AsInt(110))));
        assert_eq!(
            attribute(&sum.data_points[0].attributes, "graph_id"),
            Some(&Value::StringValue("graph".into()))
        );
        assert_eq!(
            attribute(&sum.data_points[0].attributes, "instance_id"),
            Some(&Value::StringValue("instance".into()))
        );
        let Data::Gauge(gauge) = metric(&body, "drasi.core", "drasi.plugin.probe.depth")
            .data
            .as_ref()
            .unwrap()
        else {
            panic!("depth must be a gauge")
        };
        assert!(matches!(
            gauge.data_points[0].value,
            Some(Number::AsDouble(7.0))
        ));
        for (name, unit) in [("latency", "s"), ("count", "1")] {
            let metric = metric(&body, "drasi.core", &format!("drasi.plugin.probe.{name}"));
            assert_eq!(metric.unit, unit);
            let Data::Histogram(histogram) = metric.data.as_ref().unwrap() else {
                panic!("must be histogram")
            };
            assert_eq!(histogram.aggregation_temporality, 2);
            let point = &histogram.data_points[0];
            assert_eq!(
                point.explicit_bounds,
                [
                    0.000001, 0.000005, 0.00001, 0.00005, 0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05,
                    0.1, 0.5, 1.0, 5.0, 10.0, 100.0, 1000.0
                ]
            );
            assert_eq!(point.count, 2);
            assert_eq!(point.sum, Some(2.000003));
        }
        let resource = body.resource_metrics[0].resource.as_ref().unwrap();
        assert_eq!(
            attribute(&resource.attributes, "service.name"),
            Some(&Value::StringValue("wire-test".into()))
        );
        assert!(attribute(&resource.attributes, "service.instance.id").is_some());
        runtime.shutdown().await.unwrap();
        assert_eq!(
            runtime
                .shutdown_probe
                .sdk
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            runtime
                .shutdown_probe
                .exporter
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[tokio::test]
    async fn core_prepared_but_never_activated_cleanup_sends_nothing() {
        let collector = collector().await;
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        register(
            &context,
            "pending",
            MetricKind::Counter,
            MetricUnit::One,
            &[],
        )
        .unwrap()
        .counter()
        .unwrap()
        .increment(1);
        runtime.shutdown().await.unwrap();
        assert!(runtime.is_joined().await);
        assert!(collector.received_requests().await.unwrap().is_empty());
        assert!(runtime.activate().is_err());
    }

    #[tokio::test]
    async fn core_progress_keeps_legacy_labels_and_can_be_completely_excluded() {
        for enabled in [true, false] {
            let collector = collector().await;
            let (core, input) = crate::telemetry::tests::instance("wire-progress")
                .await
                .unwrap();
            let registry = InstanceRegistry::new();
            registry
                .add("wire-progress".into(), core.clone())
                .await
                .unwrap();
            input
                .send_node_insert(
                    "one",
                    vec!["Item"],
                    drasi_source_application::PropertyMapBuilder::new()
                        .with_integer("value", 7)
                        .build(),
                )
                .await
                .unwrap();
            input
                .send_node_insert(
                    "ignored",
                    vec!["Other"],
                    drasi_source_application::PropertyMapBuilder::new().build(),
                )
                .await
                .unwrap();
            let mut runtime = MetricsRuntime::default();
            let context = runtime
                .prepare(
                    mode(&format!("{}/custom/metrics", collector.uri()), enabled),
                    &registry,
                )
                .await
                .unwrap()
                .unwrap();
            register(
                &context,
                "visible",
                MetricKind::Counter,
                MetricUnit::One,
                &[],
            )
            .unwrap()
            .counter()
            .unwrap()
            .increment(1);
            runtime.activate().unwrap();
            let body = capture(&collector).await;
            if enabled {
                let Data::Gauge(gauge) =
                    metric(&body, "drasi.progress", "drasi.query.progress.sequence")
                        .data
                        .as_ref()
                        .unwrap()
                else {
                    panic!("progress must remain a gauge")
                };
                assert_eq!(gauge.data_points.len(), 2);
                for point in &gauge.data_points {
                    assert_eq!(
                        attribute(&point.attributes, "drasi.instance.id"),
                        Some(&Value::StringValue("wire-progress".into()))
                    );
                    assert_eq!(
                        attribute(&point.attributes, "drasi.progress.persistent"),
                        Some(&Value::BoolValue(false))
                    );
                    for key in [
                        "drasi.query.id",
                        "drasi.component.generation",
                        "drasi.component.operation",
                        "drasi.query.output.generation",
                        "drasi.query.config_hash",
                    ] {
                        assert!(matches!(
                            attribute(&point.attributes, key),
                            Some(Value::StringValue(_))
                        ));
                    }
                    match attribute(&point.attributes, "drasi.progress.stage") {
                        Some(Value::StringValue(stage)) if stage == "input.committed" => {
                            assert_eq!(point.value, Some(Number::AsInt(2)));
                            assert_eq!(
                                attribute(&point.attributes, "drasi.input.kind"),
                                Some(&Value::StringValue("source".into()))
                            );
                            assert!(attribute(&point.attributes, "drasi.input.id").is_some());
                        }
                        Some(Value::StringValue(stage)) if stage == "output.produced" => {
                            assert_eq!(point.value, Some(Number::AsInt(1)))
                        }
                        _ => panic!("unexpected progress stage"),
                    }
                }
            } else {
                assert!(!body
                    .resource_metrics
                    .iter()
                    .flat_map(|r| &r.scope_metrics)
                    .flat_map(|s| &s.metrics)
                    .any(|m| m.name == "drasi.query.progress.sequence"));
            }
            runtime.shutdown().await.unwrap();
            core.shutdown().await.unwrap();
        }
    }

    #[test]
    fn core_otlp_ignores_hostile_exporter_and_resource_environment() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "telemetry::metrics_tests::otlp::hostile_environment_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env(
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "http://127.0.0.1:1/wrong-base",
            )
            .env(
                "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
                "http://127.0.0.1:1/wrong-metrics",
            )
            .env("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json")
            .env("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL", "http/json")
            .env(
                "OTEL_EXPORTER_OTLP_HEADERS",
                "authorization=hostile-global,x-global=unwanted",
            )
            .env(
                "OTEL_EXPORTER_OTLP_METRICS_HEADERS",
                "authorization=hostile-signal,x-signal=unwanted,content-type=text/plain",
            )
            .env("OTEL_EXPORTER_OTLP_TIMEOUT", "1")
            .env("OTEL_EXPORTER_OTLP_METRICS_TIMEOUT", "1")
            .env(
                "OTEL_RESOURCE_ATTRIBUTES",
                "service.name=hostile,secret=unwanted",
            )
            .env("OTEL_SERVICE_NAME", "hostile-service")
            .env("OTEL_METRIC_EXPORT_INTERVAL", "1")
            .env("OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE", "delta")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    #[ignore = "isolated hostile-environment subprocess entry point"]
    async fn hostile_environment_child() {
        let collector = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/custom/metrics"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(100)))
            .mount(&collector)
            .await;
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        register(&context, "clean", MetricKind::Counter, MetricUnit::One, &[])
            .unwrap()
            .counter()
            .unwrap()
            .increment(1);
        runtime.activate().unwrap();
        let first = capture(&collector).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        runtime.shutdown().await.unwrap();
        let requests = collector.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "one periodic and one final export");
        for request in &requests {
            for name in ["authorization", "x-global", "x-signal"] {
                assert!(
                    !request.headers.contains_key(name),
                    "environment header leaked: {name}"
                );
            }
            assert_eq!(request.headers["content-type"], "application/x-protobuf");
            let body = ExportMetricsServiceRequest::decode(request.body.as_slice()).unwrap();
            let resource = body.resource_metrics[0].resource.as_ref().unwrap();
            assert_eq!(
                attribute(&resource.attributes, "service.name"),
                Some(&Value::StringValue("wire-test".into()))
            );
            assert!(attribute(&resource.attributes, "secret").is_none());
            assert_eq!(
                attribute(&resource.attributes, "service.instance.id"),
                attribute(
                    &first.resource_metrics[0]
                        .resource
                        .as_ref()
                        .unwrap()
                        .attributes,
                    "service.instance.id"
                )
            );
            let Data::Sum(sum) = metric(&body, "drasi.core", "drasi.plugin.probe.clean")
                .data
                .as_ref()
                .unwrap()
            else {
                panic!("expected cumulative sum")
            };
            assert_eq!(sum.aggregation_temporality, 2);
            assert_eq!(sum.data_points[0].value, Some(Number::AsInt(1)));
        }
    }

    fn tls_certificate(
        issuer: Option<(
            &openssl::x509::X509,
            &openssl::pkey::PKey<openssl::pkey::Private>,
        )>,
    ) -> anyhow::Result<(
        openssl::x509::X509,
        openssl::pkey::PKey<openssl::pkey::Private>,
    )> {
        use openssl::{
            asn1::Asn1Time,
            hash::MessageDigest,
            pkey::PKey,
            rsa::Rsa,
            x509::{
                extension::{BasicConstraints, KeyUsage, SubjectAlternativeName},
                X509NameBuilder, X509,
            },
        };
        let key = PKey::from_rsa(Rsa::generate(2048)?)?;
        let mut name = X509NameBuilder::new()?;
        name.append_entry_by_text(
            "CN",
            if issuer.is_some() {
                "localhost"
            } else {
                "test CA"
            },
        )?;
        let name = name.build();
        let mut cert = X509::builder()?;
        cert.set_version(2)?;
        let serial = openssl::bn::BigNum::from_u32(if issuer.is_some() { 2 } else { 1 })?
            .to_asn1_integer()?;
        cert.set_serial_number(&serial)?;
        cert.set_subject_name(&name)?;
        cert.set_issuer_name(issuer.map_or(name.as_ref(), |(cert, _)| cert.subject_name()))?;
        cert.set_pubkey(&key)?;
        cert.set_not_before(Asn1Time::days_from_now(0)?.as_ref())?;
        cert.set_not_after(Asn1Time::days_from_now(1)?.as_ref())?;
        if issuer.is_none() {
            cert.append_extension(BasicConstraints::new().critical().ca().build()?)?;
            cert.append_extension(KeyUsage::new().key_cert_sign().crl_sign().build()?)?;
        } else {
            cert.append_extension(BasicConstraints::new().critical().build()?)?;
            cert.append_extension(
                KeyUsage::new()
                    .digital_signature()
                    .key_encipherment()
                    .build()?,
            )?;
            let san = SubjectAlternativeName::new()
                .ip("127.0.0.1")
                .build(&cert.x509v3_context(issuer.map(|(cert, _)| cert.as_ref()), None))?;
            cert.append_extension(san)?;
        }
        cert.sign(issuer.map_or(&key, |(_, key)| key), MessageDigest::sha256())?;
        Ok((cert.build(), key))
    }

    #[tokio::test]
    async fn core_tls_verifies_peer_and_delivers_only_configured_auth_without_leaks(
    ) -> anyhow::Result<()> {
        use openssl::ssl::{SslAcceptor, SslMethod};
        use opentelemetry_http::HttpClient;
        use std::io::{Read, Write};

        #[derive(Clone)]
        struct CapturedLogs(Arc<std::sync::Mutex<Vec<u8>>>);
        impl Write for CapturedLogs {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        const AUTH: &str = "Bearer test-only-sensitive-auth";
        const PAYLOAD: &[u8] = b"test-only-sensitive-input";
        let (root, root_key) = tls_certificate(None)?;
        let (leaf, key) = tls_certificate(Some((&root, &root_key)))?;
        for (trusted, signal) in [
            (false, crate::telemetry::OtlpSignal::Metrics),
            (true, crate::telemetry::OtlpSignal::Metrics),
            (false, crate::telemetry::OtlpSignal::Traces),
            (true, crate::telemetry::OtlpSignal::Traces),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let endpoint = format!("https://{}/custom/metrics", listener.local_addr()?);
            let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
            acceptor.set_certificate(&leaf)?;
            acceptor.set_private_key(&key)?;
            let acceptor = acceptor.build();
            let receiver =
                tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Vec<u8>>> {
                    let until = std::time::Instant::now() + Duration::from_secs(5);
                    let socket = loop {
                        match listener.accept() {
                            Ok((socket, _)) => break socket,
                            Err(error)
                                if error.kind() == std::io::ErrorKind::WouldBlock
                                    && std::time::Instant::now() < until =>
                            {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            Err(error) => return Err(error.into()),
                        }
                    };
                    socket.set_nonblocking(false)?;
                    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
                    socket.set_write_timeout(Some(Duration::from_secs(3)))?;
                    let mut stream = match acceptor.accept(socket) {
                        Ok(stream) => stream,
                        Err(error) => {
                            anyhow::ensure!(
                                !trusted,
                                "Trusted fixture TLS handshake failed: {error}"
                            );
                            return Ok(None);
                        }
                    };
                    let mut received = Vec::new();
                    loop {
                        let mut chunk = [0; 4096];
                        let read = stream.read(&mut chunk)?;
                        anyhow::ensure!(read != 0, "TLS request ended before its body");
                        received.extend_from_slice(&chunk[..read]);
                        anyhow::ensure!(received.len() < 65_536, "unexpectedly large test request");
                        if let Some(end) =
                            received.windows(4).position(|bytes| bytes == b"\r\n\r\n")
                        {
                            let headers =
                                String::from_utf8_lossy(&received[..end]).to_ascii_lowercase();
                            let length: usize = headers
                                .lines()
                                .find_map(|line| line.strip_prefix("content-length:"))
                                .ok_or_else(|| anyhow::anyhow!("missing request length"))?
                                .trim()
                                .parse()?;
                            if received.len() >= end + 4 + length {
                                break;
                            }
                        }
                    }
                    stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )?;
                    Ok(Some(received))
                });
            let config: TelemetryConfig = serde_json::from_value(serde_json::json!({
                "metrics": {"backend": "otlp", "otlp": {
                    "endpoint": endpoint, "protocol": "http/protobuf",
                    "headers": {"authorization": "${OBS04_TLS_TEST_AUTH}"}
                }}
            }))?;
            let serialized = serde_yaml::to_string(&config)?;
            assert!(serialized.contains("OBS04_TLS_TEST_AUTH"));
            assert!(!serialized.contains(AUTH));
            let mapper = DtoMapper::with_overrides(std::collections::HashMap::from([(
                "OBS04_TLS_TEST_AUTH".into(),
                AUTH.into(),
            )]));
            let destination = config.resolve(&mapper, 8080, true)?.metrics_otlp.unwrap();
            let root_der = root.to_der()?;
            let client_result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let logs = CapturedLogs(Arc::new(std::sync::Mutex::new(Vec::new())));
                let writer = logs.clone();
                let subscriber = tracing_subscriber::fmt()
                    .without_time()
                    .with_ansi(false)
                    .with_max_level(tracing::Level::TRACE)
                    .with_writer(move || writer.clone())
                    .finish();
                let _scope = tracing::subscriber::set_default(subscriber);
                let mut client = crate::telemetry::CoreHttpClient::new(
                    destination,
                    Duration::from_secs(2),
                    signal,
                )?;
                if trusted {
                    // Test-only trust anchor: no custom-CA production configuration.
                    client.client = reqwest_otel::blocking::Client::builder()
                        .tls_certs_only([reqwest_otel::Certificate::from_der(&root_der)?])
                        .redirect(reqwest_otel::redirect::Policy::none())
                        .timeout(Duration::from_secs(2))
                        .build()?;
                }
                client.allow_until(None)?;
                let request = opentelemetry_http::Request::builder()
                    .uri("http://unused.invalid/ignored")
                    .header("x-unconfigured", "must-not-reach-receiver")
                    .body(bytes::Bytes::from_static(PAYLOAD))?;
                let result = futures::executor::block_on(client.send_bytes(request));
                assert!(!format!("{client:?}").contains("/custom/metrics"));
                let diagnostics = format!(
                    "{client:?} {:?} {}",
                    result.as_ref().err(),
                    String::from_utf8_lossy(&logs.0.lock().unwrap())
                );
                assert!(!diagnostics.contains(AUTH));
                assert!(!diagnostics.contains(std::str::from_utf8(PAYLOAD)?));
                if trusted {
                    assert_eq!(
                        result.map_err(|error| anyhow::anyhow!("{error}"))?.status(),
                        200
                    );
                } else {
                    assert!(
                        result.is_err(),
                        "production client accepted an untrusted CA"
                    );
                }
                Ok(())
            })
            .await?;
            let received = receiver.await??;
            client_result?;
            if trusted {
                let request = String::from_utf8(received.expect("trusted TLS request"))?;
                assert!(request.starts_with("POST /custom/metrics HTTP/1.1\r\n"));
                assert!(request
                    .to_ascii_lowercase()
                    .contains("content-type: application/x-protobuf\r\n"));
                assert!(request.contains(&format!("authorization: {AUTH}\r\n")));
                assert!(request.ends_with(std::str::from_utf8(PAYLOAD)?));
                assert!(!request.contains("x-unconfigured"));
            } else {
                assert!(
                    received.is_none(),
                    "untrusted TLS peer received credentials or body"
                );
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn core_transport_never_follows_redirects() {
        let redirected = collector().await;
        let collector = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("{}/custom/metrics", redirected.uri())),
            )
            .mount(&collector)
            .await;
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        register(
            &context,
            "redirect",
            MetricKind::Counter,
            MetricUnit::One,
            &[],
        )
        .unwrap()
        .counter()
        .unwrap()
        .increment(1);
        runtime.activate().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        runtime.request_shutdown(deadline).unwrap();
        assert!(runtime.wait_shutdown_until(deadline).await.is_err());
        assert!(runtime.is_joined().await);
        assert_eq!(collector.received_requests().await.unwrap().len(), 1);
        assert!(redirected.received_requests().await.unwrap().is_empty());
    }

    async fn stalled_periodic_export_retains_worker() {
        let collector = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3)))
            .mount(&collector)
            .await;
        let (core, input) = crate::telemetry::tests::instance("stalled-core")
            .await
            .unwrap();
        let registry = InstanceRegistry::new();
        registry
            .add("stalled-core".into(), core.clone())
            .await
            .unwrap();
        let mut runtime = MetricsRuntime::default();
        runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), true),
                &registry,
            )
            .await
            .unwrap();
        runtime.activate().unwrap();
        capture(&collector).await;
        input
            .send_node_insert(
                "during-export",
                vec!["Item"],
                drasi_source_application::PropertyMapBuilder::new()
                    .with_integer("value", 99)
                    .build(),
            )
            .await
            .unwrap();
        timeout(Duration::from_millis(500), async {
            let query = core.component_by_name("items").unwrap();
            loop {
                if core.get_query_results_by_id(query.id()).await.unwrap()
                    == vec![serde_json::json!({"value":99})]
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("query/heartbeat blocked by transport");
        let deadline = Instant::now() + Duration::from_millis(80);
        runtime.request_shutdown(deadline).unwrap();
        assert!(timeout(
            Duration::from_millis(10),
            runtime.wait_shutdown_until(deadline)
        )
        .await
        .is_err());
        assert!(!runtime.is_joined().await);
        assert!(runtime.wait_shutdown_until(Instant::now()).await.is_err());
        assert!(!runtime.is_joined().await);
        assert!(
            timeout(Duration::from_secs(2), runtime.wait_for_completion())
                .await
                .unwrap()
                .is_err()
        );
        assert!(runtime.is_joined().await);
        assert_eq!(
            collector.received_requests().await.unwrap().len(),
            1,
            "no fresh request after phase expiry"
        );
        assert_eq!(
            runtime
                .shutdown_probe
                .sdk
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            runtime
                .shutdown_probe
                .exporter
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert!(
            runtime.wait_for_completion().await.is_err(),
            "joining must retain phase failure"
        );
        core.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn core_stall_keeps_current_thread_responsive_and_cancelled_wait_owned() {
        stalled_periodic_export_retains_worker().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn core_stall_keeps_multiple_threads_responsive_and_cancelled_wait_owned() {
        stalled_periodic_export_retains_worker().await;
    }

    #[tokio::test]
    async fn core_final_request_is_capped_by_remaining_phase_not_a_fresh_timeout() {
        let collector = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3)))
            .mount(&collector)
            .await;
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        register(&context, "final", MetricKind::Counter, MetricUnit::One, &[])
            .unwrap()
            .counter()
            .unwrap()
            .increment(1);
        runtime.activate().unwrap();
        let start = Instant::now();
        runtime
            .request_shutdown(start + Duration::from_millis(80))
            .unwrap();
        let result = timeout(Duration::from_millis(600), runtime.wait_for_completion())
            .await
            .expect("final HTTP request exceeded remaining allowance");
        assert!(result.is_err());
        assert!(runtime.is_joined().await);
        assert_eq!(collector.received_requests().await.unwrap().len(), 1);
        assert!(runtime.wait_for_completion().await.is_err());
    }

    #[test]
    fn core_export_warnings_suppress_repetition_and_report_recovery_once() {
        let mut warnings = crate::telemetry::ExportWarnings::default();
        let now = Instant::now();
        assert_eq!(warnings.failed(now), Some(0));
        assert_eq!(warnings.failed(now + Duration::from_secs(1)), None);
        assert_eq!(warnings.failed(now + Duration::from_secs(29)), None);
        assert_eq!(warnings.failed(now + Duration::from_secs(30)), Some(2));
        assert_eq!(warnings.failed(now + Duration::from_secs(31)), None);
        assert_eq!(warnings.recovered(), Some(1));
        assert_eq!(warnings.recovered(), None);
        assert_eq!(warnings.failed(now + Duration::from_secs(32)), None);
        assert_eq!(warnings.recovered(), Some(1));
        assert_eq!(warnings.recovered(), None);
        assert_eq!(warnings.failed(now + Duration::from_secs(34)), None);
        assert_eq!(warnings.failed(now + Duration::from_secs(59)), None);
        assert_eq!(warnings.failed(now + Duration::from_secs(60)), Some(2));
        assert_eq!(warnings.recovered(), Some(0));
        assert_eq!(warnings.recovered(), None);
    }

    #[tokio::test]
    async fn core_preparation_failure_is_retained_and_sends_no_http() {
        let collector = collector().await;
        let mut mode = mode(&format!("{}/custom/metrics", collector.uri()), true);
        let MetricsMode::CoreOtlp {
            export_interval, ..
        } = &mut mode
        else {
            unreachable!()
        };
        *export_interval = Duration::ZERO;
        let mut runtime = MetricsRuntime::default();
        assert!(runtime
            .prepare(mode, &InstanceRegistry::new())
            .await
            .is_err());
        assert!(
            runtime.wait_for_completion().await.is_err(),
            "initialization error must remain in the owned terminal outcome"
        );
        assert!(runtime.wait_for_completion().await.is_err());
        assert!(runtime.is_joined().await);
        assert!(collector.received_requests().await.unwrap().is_empty());
        assert!(runtime.activate().is_err());
    }

    #[tokio::test]
    async fn core_cancelled_preparation_keeps_worker_and_closes_without_export() {
        use std::sync::atomic::Ordering;
        let collector = collector().await;
        let mut runtime = MetricsRuntime::default();
        let gate = Arc::new(crate::telemetry::tests::PreparationGate::default());
        *runtime.shutdown_probe.preparation.lock().unwrap() = Some(gate.clone());
        let registry = InstanceRegistry::new();
        let mut preparing = Box::pin(runtime.prepare(
            mode(&format!("{}/custom/metrics", collector.uri()), true),
            &registry,
        ));
        assert!(timeout(Duration::from_millis(50), &mut preparing)
            .await
            .is_err());
        assert!(
            gate.entered.load(Ordering::SeqCst),
            "preparation never reached the retained worker"
        );
        drop(preparing);
        assert!(!runtime.is_joined().await);
        runtime
            .request_shutdown(Instant::now() + Duration::from_secs(2))
            .unwrap();
        *gate.released.lock().unwrap() = true;
        gate.changed.notify_all();
        timeout(Duration::from_secs(2), runtime.wait_for_completion())
            .await
            .unwrap()
            .unwrap();
        assert!(runtime.is_joined().await);
        assert!(collector.received_requests().await.unwrap().is_empty());
        assert_eq!(runtime.shutdown_probe.sdk.load(Ordering::SeqCst), 1);
        assert_eq!(runtime.shutdown_probe.exporter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn core_connection_refusal_recovers_without_queued_snapshots_or_permanent_failure() {
        use std::sync::atomic::Ordering;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("http://{address}/custom/metrics"), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        let counter = register(
            &context,
            "recover",
            MetricKind::Counter,
            MetricUnit::One,
            &[],
        )
        .unwrap()
        .counter()
        .unwrap();
        counter.increment(1);
        runtime.activate().unwrap();
        eventually(|| {
            runtime
                .shutdown_probe
                .periodic_exports
                .load(Ordering::SeqCst)
                == 1
        })
        .await;
        let listener = std::net::TcpListener::bind(address).unwrap();
        let collector = MockServer::builder().listener(listener).start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&collector)
            .await;
        counter.increment(4);
        let body = capture(&collector).await;
        let Data::Sum(sum) = metric(&body, "drasi.core", "drasi.plugin.probe.recover")
            .data
            .as_ref()
            .unwrap()
        else {
            panic!("expected recovered cumulative sum")
        };
        assert_eq!(sum.data_points[0].value, Some(Number::AsInt(5)));
        assert_eq!(collector.received_requests().await.unwrap().len(), 1);
        runtime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn core_slow_export_skips_missed_ticks_instead_of_catching_up() {
        use std::sync::atomic::Ordering;
        let collector = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(1400)))
            .mount(&collector)
            .await;
        let mut mode = mode(&format!("{}/custom/metrics", collector.uri()), false);
        let MetricsMode::CoreOtlp { export_timeout, .. } = &mut mode else {
            unreachable!()
        };
        *export_timeout = Duration::from_secs(3);
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(mode, &InstanceRegistry::new())
            .await
            .unwrap()
            .unwrap();
        register(&context, "slow", MetricKind::Counter, MetricUnit::One, &[])
            .unwrap()
            .counter()
            .unwrap()
            .increment(1);
        runtime.activate().unwrap();
        capture(&collector).await;
        timeout(Duration::from_secs(2), async {
            while runtime
                .shutdown_probe
                .periodic_exports
                .load(Ordering::SeqCst)
                == 0
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            collector.received_requests().await.unwrap().len(),
            1,
            "missed ticks triggered a catch-up request"
        );
        runtime.request_shutdown(Instant::now()).unwrap();
        assert!(runtime.wait_for_completion().await.is_err());
        assert_eq!(collector.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn core_streams_preserve_more_than_the_sdk_default_cardinality() {
        let collector = collector().await;
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        for value in 0..2001 {
            register(
                &context,
                "many",
                MetricKind::Counter,
                MetricUnit::One,
                &[("series", &value.to_string())],
            )
            .unwrap()
            .counter()
            .unwrap()
            .increment(1);
        }
        assert_eq!(
            context.registrations().counts().unwrap(),
            (1, 2001),
            "known producer work did not finish before activation"
        );
        runtime.activate().unwrap();
        let body = capture(&collector).await;
        let Data::Sum(sum) = metric(&body, "drasi.core", "drasi.plugin.probe.many")
            .data
            .as_ref()
            .unwrap()
        else {
            panic!("expected all accepted series")
        };
        let total: i64 = sum
            .data_points
            .iter()
            .map(|point| match point.value {
                Some(Number::AsInt(value)) => value,
                _ => panic!("counter is not an integer"),
            })
            .sum();
        let present: std::collections::BTreeSet<_> = sum
            .data_points
            .iter()
            .filter_map(|point| match attribute(&point.attributes, "series") {
                Some(Value::StringValue(value)) => Some(value.as_str()),
                _ => None,
            })
            .collect();
        let missing: Vec<_> = (0..2001)
            .filter(|value| !present.contains(value.to_string().as_str()))
            .take(15)
            .collect();
        let coalesced: Vec<_> = sum
            .data_points
            .iter()
            .filter(|point| matches!(point.value, Some(Number::AsInt(value)) if value > 1))
            .take(15)
            .map(|point| (attribute(&point.attributes, "series"), &point.value))
            .collect();
        assert_eq!(sum.data_points.len(), 2001, "completed Core registrations: {:?}; sum={total}; missing={missing:?}; coalesced={coalesced:?}; overflow={:?}",
            context.registrations().counts().unwrap(),
            sum.data_points.iter().find(|point| attribute(&point.attributes, "otel.metric.overflow").is_some()));
        assert!(sum.data_points.iter().all(|point| {
            attribute(&point.attributes, "otel.metric.overflow").is_none()
                && point.value == Some(Number::AsInt(1))
        }));
        runtime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn core_distinct_small_series_must_not_alias_in_adapter_cache() {
        let collector = collector().await;
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        for value in ["1", "11"] {
            register(
                &context,
                "alias",
                MetricKind::Counter,
                MetricUnit::One,
                &[("series", value)],
            )
            .unwrap()
            .counter()
            .unwrap()
            .increment(1);
        }
        assert_eq!(context.registrations().counts().unwrap(), (1, 2));
        let first = metrics::Key::from_parts("drasi.plugin.probe.alias", &[("series", "1")]);
        let second = metrics::Key::from_parts("drasi.plugin.probe.alias", &[("series", "11")]);
        runtime.activate().unwrap();
        let body = capture(&collector).await;
        let Data::Sum(sum) = metric(&body, "drasi.core", "drasi.plugin.probe.alias")
            .data
            .as_ref()
            .unwrap()
        else {
            panic!("expected counter series")
        };
        assert_eq!(
            sum.data_points.len(),
            2,
            "metrics Key eq={}, hashes=({}, {}); OTel attribute eq={}; points={:?}",
            first == second,
            first.get_hash(),
            second.get_hash(),
            opentelemetry::KeyValue::new("series", "1")
                == opentelemetry::KeyValue::new("series", "11"),
            sum.data_points
        );
        runtime.shutdown().await.unwrap();
    }

    async fn colliding_metric(kind: MetricKind) -> Metric {
        let collector = collector().await;
        let mut runtime = MetricsRuntime::default();
        let context = runtime
            .prepare(
                mode(&format!("{}/custom/metrics", collector.uri()), false),
                &InstanceRegistry::new(),
            )
            .await
            .unwrap()
            .unwrap();
        for (series, value) in [("1", 3.0), ("11", 7.0)] {
            let metric = register(
                &context,
                "collision",
                kind,
                MetricUnit::One,
                &[("series", series)],
            )
            .unwrap();
            match kind {
                MetricKind::Gauge => metric.gauge().unwrap().set(value),
                MetricKind::Histogram => metric.histogram().unwrap().record(value),
                _ => unreachable!("fixture covers the gauge and histogram handle caches"),
            }
        }
        runtime.activate().unwrap();
        let body = capture(&collector).await;
        let metric = metric(&body, "drasi.core", "drasi.plugin.probe.collision").clone();
        runtime.shutdown().await.unwrap();
        metric
    }

    #[tokio::test]
    async fn core_colliding_gauge_series_keep_their_own_values() {
        let metric = colliding_metric(MetricKind::Gauge).await;
        let Data::Gauge(gauge) = metric.data.unwrap() else {
            panic!("expected gauge")
        };
        assert_eq!(gauge.data_points.len(), 2);
        for (series, value) in [("1", 3.0), ("11", 7.0)] {
            let point = gauge
                .data_points
                .iter()
                .find(|point| {
                    attribute(&point.attributes, "series")
                        == Some(&Value::StringValue(series.into()))
                })
                .expect("distinct accepted gauge series missing");
            assert_eq!(point.value, Some(Number::AsDouble(value)));
        }
    }

    #[tokio::test]
    async fn core_colliding_histogram_series_keep_their_own_observations() {
        let metric = colliding_metric(MetricKind::Histogram).await;
        let Data::Histogram(histogram) = metric.data.unwrap() else {
            panic!("expected histogram")
        };
        assert_eq!(histogram.data_points.len(), 2);
        for (series, value) in [("1", 3.0), ("11", 7.0)] {
            let point = histogram
                .data_points
                .iter()
                .find(|point| {
                    attribute(&point.attributes, "series")
                        == Some(&Value::StringValue(series.into()))
                })
                .expect("distinct accepted histogram series missing");
            assert_eq!(point.count, 1);
            assert_eq!(point.sum, Some(value));
        }
    }
}

async fn eventually(mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !condition() && std::time::Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert!(
        condition(),
        "real retained work did not reach the expected state"
    );
}

async fn upkeep_eventually(
    runtime: &PrometheusRuntime,
    stage: &str,
    mut condition: impl FnMut() -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !condition() && std::time::Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert!(condition(), "{stage}: {}", fixtures::upkeep_status(runtime));
}

async fn scrape(runtime: &Arc<PrometheusRuntime>) -> Response {
    scrape_router("/probe/metrics", runtime.clone())
        .unwrap()
        .oneshot(
            axum::http::Request::builder()
                .uri("/probe/metrics")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn close(owner: &ServerCleanupHandle) {
    timeout(Duration::from_secs(2), owner.shutdown())
        .await
        .unwrap()
        .unwrap();
}

fn register(
    context: &MetricRegistrationContext,
    leaf: &str,
    kind: MetricKind,
    unit: MetricUnit,
    labels: &[(&str, &str)],
) -> Result<RegisteredMetric, TelemetryError> {
    let scope = TelemetryScope::graph("instance", "graph")
        .unwrap()
        .component(
            ComponentId::try_new("064e948e-04b4-40d2-8dcd-32740a66e91f").unwrap(),
            ComponentRole::Source,
            ComponentSemanticKind::Source,
        );
    context.plugin(
        &TelemetryProfile::Debug.resolve(),
        &scope,
        "probe",
        PluginMetricDefinition {
            leaf,
            kind,
            unit,
            description: "Test metric",
        },
        labels
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into())),
    )
}

#[test]
fn normalized_names_reject_alias_before_backend_registration() {
    let (context, handle) = prometheus_recorder().unwrap();
    register(&context, "a-b", MetricKind::Counter, MetricUnit::One, &[])
        .unwrap()
        .counter()
        .unwrap()
        .increment(3);
    let before = context.registrations().counts().unwrap();
    assert!(matches!(
        register(&context, "a_b", MetricKind::Counter, MetricUnit::One, &[]),
        Err(TelemetryError::Conflict(_))
    ));
    assert_eq!(context.registrations().counts().unwrap(), before);
    register(&context, "a-b", MetricKind::Counter, MetricUnit::One, &[])
        .unwrap()
        .counter()
        .unwrap()
        .increment(2);
    let text = handle.render();
    assert!(text.contains("drasi_plugin_probe_a_b{"));
    assert!(text
        .lines()
        .any(|line| line.starts_with("drasi_plugin_probe_a_b{") && line.ends_with(" 5")));
}

#[test]
fn histogram_generated_names_are_reserved_in_both_registration_orders() {
    for suffix in ["sum", "count", "bucket"] {
        for histogram_first in [true, false] {
            let (context, handle) = prometheus_recorder().unwrap();
            let generated = format!("latency_{suffix}");
            let (first, kind, second, second_kind) = if histogram_first {
                (
                    "latency",
                    MetricKind::Histogram,
                    generated.as_str(),
                    MetricKind::Counter,
                )
            } else {
                (
                    generated.as_str(),
                    MetricKind::Counter,
                    "latency",
                    MetricKind::Histogram,
                )
            };
            register(&context, first, kind, MetricUnit::One, &[]).unwrap();
            let before = context.registrations().counts().unwrap();
            assert!(
                matches!(
                    register(&context, second, second_kind, MetricUnit::One, &[]),
                    Err(TelemetryError::Conflict(_))
                ),
                "histogram_first={histogram_first}, suffix={suffix}"
            );
            assert_eq!(context.registrations().counts().unwrap(), before);
            // A rejected histogram must not reserve its other generated names.
            if !histogram_first {
                register(
                    &context,
                    "latency-free",
                    MetricKind::Counter,
                    MetricUnit::One,
                    &[],
                )
                .unwrap();
                let other = if suffix == "sum" {
                    "latency_count"
                } else {
                    "latency_sum"
                };
                register(&context, other, MetricKind::Counter, MetricUnit::One, &[]).unwrap();
                assert!(!handle
                    .render()
                    .contains("# TYPE drasi_plugin_probe_latency histogram"));
            }
        }
    }
}

#[test]
fn normalized_label_aliases_and_histogram_le_are_transactional_rejections() {
    for (kind, labels) in [
        (
            MetricKind::Counter,
            vec![("foo-bar", "one"), ("foo_bar", "two")],
        ),
        (MetricKind::Histogram, vec![("le", "custom")]),
    ] {
        let (context, handle) = prometheus_recorder().unwrap();
        assert!(matches!(
            register(&context, "sample", kind, MetricUnit::One, &labels),
            Err(TelemetryError::Invalid(_))
        ));
        assert_eq!(context.registrations().counts().unwrap(), (0, 0));
        assert!(handle.render().is_empty(), "rejection reached the backend");
        register(&context, "sample", kind, MetricUnit::One, &[]).unwrap();
        register(&context, "sample", kind, MetricUnit::One, &[]).unwrap();
        assert_eq!(context.registrations().counts().unwrap(), (1, 1));
    }
}

#[test]
fn concurrent_normalized_collisions_accept_exactly_one_definition() {
    let (context, _) = prometheus_recorder().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let jobs: Vec<_> = ["a-b", "a_b"]
        .into_iter()
        .map(|name| {
            let context = context.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                register(&context, name, MetricKind::Counter, MetricUnit::One, &[])
            })
        })
        .collect();
    let results: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(TelemetryError::Conflict(_))))
            .count(),
        1
    );
    assert_eq!(context.registrations().counts().unwrap(), (1, 1));
}

#[test]
fn histogram_export_preserves_seconds_counts_and_all_approved_bounds() {
    let (context, handle) = prometheus_recorder().unwrap();
    for (name, unit) in [("seconds", MetricUnit::Seconds), ("count", MetricUnit::One)] {
        let histogram = register(&context, name, MetricKind::Histogram, unit, &[])
            .unwrap()
            .histogram()
            .unwrap();
        histogram.record(0.000003);
        histogram.record(2.0);
    }
    let text = handle.render();
    for name in ["seconds", "count"] {
        let prefix = format!("drasi_plugin_probe_{name}_bucket{{");
        let buckets: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with(&prefix))
            .map(|line| {
                line.split("le=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(
            buckets,
            [
                "0.000001", "0.000005", "0.00001", "0.00005", "0.0001", "0.0005", "0.001", "0.005",
                "0.01", "0.05", "0.1", "0.5", "1", "5", "10", "100", "1000", "+Inf"
            ]
        );
        assert!(text.lines().any(|line| line
            .starts_with(&format!("drasi_plugin_probe_{name}_sum{{"))
            && line.ends_with(" 2.000003")));
        assert!(text.lines().any(|line| line
            .starts_with(&format!("drasi_plugin_probe_{name}_count{{"))
            && line.ends_with(" 2")));
    }
}

#[tokio::test]
async fn empty_scrape_is_valid_prometheus_text() {
    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    let response = scrape(&runtime).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[CONTENT_TYPE],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    assert!(axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap()
        .is_empty());
    close(&owner).await;
    assert!(runtime.is_joined().await);
}

fn process(metrics: Metrics) -> Arc<ProcessMetrics> {
    let policy = TelemetrySettings::new(TelemetryProfile::Basic)
        .with_metrics(true)
        .resolve()
        .unwrap();
    // `Metrics` is not `Clone`; its fields are all `Copy`.
    let Metrics {
        cpu_seconds_total,
        open_fds,
        max_fds,
        virtual_memory_bytes,
        virtual_memory_max_bytes,
        resident_memory_bytes,
        start_time_seconds,
        threads,
    } = metrics;
    ProcessMetrics::with_source(
        &policy,
        Arc::new(move || Metrics {
            cpu_seconds_total,
            open_fds,
            max_fds,
            virtual_memory_bytes,
            virtual_memory_max_bytes,
            resident_memory_bytes,
            start_time_seconds,
            threads,
        }),
    )
}

async fn scrape_text(runtime: &Arc<PrometheusRuntime>) -> String {
    let response = scrape(runtime).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[CONTENT_TYPE],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

fn metric_name(line: &str) -> &str {
    let sample = line
        .strip_prefix("# HELP ")
        .or_else(|| line.strip_prefix("# TYPE "))
        .unwrap_or(line);
    sample.split(['{', ' ']).next().unwrap_or_default()
}

#[tokio::test]
async fn process_lines_form_valid_text_after_empty_render() {
    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare_with_process(
        &owner,
        handle,
        Some(process(Metrics {
            cpu_seconds_total: Some(1.25),
            resident_memory_bytes: Some(4096),
            ..Default::default()
        })),
    )
    .await
    .unwrap();
    assert_eq!(
        scrape_text(&runtime).await,
        "# HELP process_cpu_seconds_total Total user and system CPU time spent in seconds.\n\
         # TYPE process_cpu_seconds_total counter\n\
         process_cpu_seconds_total 1.25\n\
         # HELP process_resident_memory_bytes Resident memory size in bytes.\n\
         # TYPE process_resident_memory_bytes gauge\n\
         process_resident_memory_bytes 4096\n"
    );
    close(&owner).await;
    assert!(runtime.is_joined().await);
}

#[tokio::test]
async fn process_families_follow_drasi_metrics_without_collision() {
    let owner = ServerCleanupHandle::bootstrap();
    let (context, handle) = prometheus_recorder().unwrap();
    register(
        &context,
        "cpu_seconds_total",
        MetricKind::Counter,
        MetricUnit::One,
        &[],
    )
    .unwrap()
    .counter()
    .unwrap()
    .increment(2);
    let runtime = PrometheusRuntime::prepare_with_process(
        &owner,
        handle,
        Some(process(Metrics {
            cpu_seconds_total: Some(1.25),
            resident_memory_bytes: Some(4096),
            ..Default::default()
        })),
    )
    .await
    .unwrap();
    let text = scrape_text(&runtime).await;
    assert!(
        text.contains("# TYPE drasi_plugin_probe_cpu_seconds_total counter"),
        "{text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| *line == "# TYPE process_cpu_seconds_total counter")
            .count(),
        1,
        "{text}"
    );
    let types: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("# TYPE "))
        .collect();
    let unique: std::collections::HashSet<_> = types.iter().collect();
    assert_eq!(unique.len(), types.len(), "{text}");
    let lines: Vec<_> = text.lines().collect();
    let last_drasi = lines
        .iter()
        .rposition(|line| metric_name(line).starts_with("drasi_"))
        .expect("drasi family rendered");
    let first_process = lines
        .iter()
        .position(|line| metric_name(line).starts_with("process_"))
        .expect("process family rendered");
    assert!(last_drasi < first_process, "{text}");
    close(&owner).await;
    assert!(runtime.is_joined().await);
}

#[tokio::test]
async fn failed_process_reading_keeps_scrape_successful() {
    let owner = ServerCleanupHandle::bootstrap();
    let (context, handle) = prometheus_recorder().unwrap();
    register(&context, "depth", MetricKind::Gauge, MetricUnit::One, &[])
        .unwrap()
        .gauge()
        .unwrap()
        .set(3.0);
    let runtime =
        PrometheusRuntime::prepare_with_process(&owner, handle, Some(process(Metrics::default())))
            .await
            .unwrap();
    let text = scrape_text(&runtime).await;
    assert!(
        text.lines()
            .any(|line| line.starts_with("drasi_plugin_probe_depth{") && line.ends_with(" 3")),
        "{text}"
    );
    assert!(!text.contains("process_"), "{text}");
    close(&owner).await;
    assert!(runtime.is_joined().await);
}

#[tokio::test]
async fn four_physical_renders_keep_slots_when_waiter_is_cancelled() {
    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    let mut stall = fixtures::BackendStall::acquire(&runtime).await;
    let mut waiters = Vec::new();
    for _ in 0..4 {
        let runtime = runtime.clone();
        waiters.push(tokio::spawn(async move { scrape(&runtime).await }));
    }
    eventually(|| fixtures::render_count(&runtime) == 4).await;
    let overload = timeout(Duration::from_millis(100), scrape(&runtime))
        .await
        .expect("overload must reject without queuing");
    assert_eq!(overload.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(overload.into_body(), 4096)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"],
        "TELEMETRY_SCRAPE_UNAVAILABLE"
    );
    let cancelled = waiters.pop().unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    assert_eq!(
        scrape(&runtime).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(fixtures::render_count(&runtime), 4);
    stall.release();
    for waiter in waiters {
        assert_eq!(
            timeout(Duration::from_secs(2), waiter)
                .await
                .unwrap()
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    eventually(|| fixtures::upkeep_counts(&runtime).2 == 0).await;
    let response = timeout(Duration::from_secs(2), scrape(&runtime))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    close(&owner).await;
    assert!(runtime.is_joined().await);
    stall.finish();
}

#[tokio::test]
async fn five_second_wait_timeout_does_not_release_the_worker() {
    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    let mut stall = fixtures::BackendStall::acquire(&runtime).await;
    let started = Instant::now();
    let waiter = {
        let runtime = runtime.clone();
        tokio::spawn(async move { scrape(&runtime).await })
    };
    eventually(|| fixtures::render_count(&runtime) == 1).await;
    let response = timeout(Duration::from_secs(6), waiter)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(started.elapsed() >= Duration::from_secs(5));
    assert_eq!(fixtures::render_count(&runtime), 1);
    let deadline = Instant::now() + Duration::from_millis(30);
    runtime.request_shutdown(deadline).unwrap();
    assert!(runtime.wait_shutdown_until(deadline).await.is_err());
    assert!(!runtime.is_joined().await);
    assert_eq!(
        scrape(&runtime).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    stall.release();
    timeout(Duration::from_secs(2), runtime.wait_for_completion())
        .await
        .unwrap()
        .unwrap();
    assert!(runtime.is_joined().await);
    close(&owner).await;
    stall.finish();
}

#[tokio::test]
async fn failed_render_returns_typed_500_and_remains_a_cleanup_failure() {
    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    fixtures::poison_backend(&runtime);
    let response = scrape(&runtime).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "TELEMETRY_RENDER_FAILED");
    assert!(!String::from_utf8_lossy(&body).contains("poison"));
    assert!(timeout(Duration::from_secs(2), owner.shutdown())
        .await
        .unwrap()
        .is_err());
    assert_eq!(owner.status().phase, CleanupPhase::Complete);
    assert!(runtime.is_joined().await);
    assert!(runtime.wait_for_completion().await.is_err());
}

fn inspection_router(runtime: Option<Arc<PrometheusRuntime>>) -> axum::Router {
    use crate::api::v1::telemetry_handlers::{telemetry_router, TelemetryInspection};
    let config: crate::config::telemetry::TelemetryConfig =
        serde_json::from_value(serde_json::json!({
            "inspection": {"enabled": true},
            "metrics": {"backend": "prometheus"}
        }))
        .unwrap();
    let effective = config
        .resolve(&crate::api::mappings::DtoMapper::new(), 8080, true)
        .unwrap();
    telemetry_router(TelemetryInspection::new(&effective, runtime))
}

async fn inspect_metrics(router: &axum::Router) -> Response {
    router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(crate::api::v1::telemetry_handlers::TELEMETRY_METRICS_PATH)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Status, error code and raw body of an error response.
async fn error_parts(response: Response) -> (StatusCode, String, String) {
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body).into_owned();
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let code = json["code"].as_str().unwrap_or_default().to_owned();
    (status, code, body)
}

#[tokio::test]
async fn inspection_metrics_route_status_matrix() {
    let (status, code, _) = error_parts(inspect_metrics(&inspection_router(None)).await).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code, "TELEMETRY_METRICS_UNAVAILABLE");

    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    let response = inspect_metrics(&inspection_router(Some(runtime.clone()))).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[CONTENT_TYPE],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    close(&owner).await;
    assert!(runtime.is_joined().await);

    // A fresh runtime has no finished render awaiting reaping, so the four
    // stalled renders deterministically fill admission.
    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    let router = inspection_router(Some(runtime.clone()));
    let mut stall = fixtures::BackendStall::acquire(&runtime).await;
    let mut waiters = Vec::new();
    for _ in 0..4 {
        let runtime = runtime.clone();
        waiters.push(tokio::spawn(async move { scrape(&runtime).await }));
    }
    eventually(|| fixtures::render_count(&runtime) == 4).await;
    let overloaded = timeout(Duration::from_millis(100), inspect_metrics(&router))
        .await
        .expect("inspection must reject without queuing");
    let (status, code, _) = error_parts(overloaded).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code, "TELEMETRY_METRICS_UNAVAILABLE");
    let (status, code, _) = error_parts(scrape(&runtime).await).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code, "TELEMETRY_SCRAPE_UNAVAILABLE");
    assert_eq!(fixtures::render_count(&runtime), 4);
    stall.release();
    for waiter in waiters {
        assert_eq!(
            timeout(Duration::from_secs(2), waiter)
                .await
                .unwrap()
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    close(&owner).await;
    assert!(runtime.is_joined().await);
    stall.finish();

    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    fixtures::poison_backend(&runtime);
    let (status, code, body) =
        error_parts(inspect_metrics(&inspection_router(Some(runtime.clone()))).await).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(code, "TELEMETRY_INSPECTION_FAILED");
    assert!(!body.contains("poison"), "{body}");
    assert!(timeout(Duration::from_secs(2), owner.shutdown())
        .await
        .unwrap()
        .is_err());
    assert!(runtime.is_joined().await);
}

#[tokio::test(start_paused = true)]
async fn upkeep_runs_without_scrapers_and_cleanup_joins_actual_work() {
    let owner = ServerCleanupHandle::bootstrap();
    let (context, handle) = prometheus_recorder().unwrap();
    let histogram = register(
        &context,
        "load",
        MetricKind::Histogram,
        MetricUnit::One,
        &[],
    )
    .unwrap()
    .histogram()
    .unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    let mut stall = fixtures::BackendStall::acquire(&runtime).await;
    for _ in 0..20_000 {
        histogram.record(0.25);
    }
    tokio::time::advance(Duration::from_secs(5)).await;
    upkeep_eventually(&runtime, "first upkeep submission", || {
        fixtures::upkeep_counts(&runtime).0 == 1
    })
    .await;
    tokio::time::advance(Duration::from_secs(30)).await;
    assert_eq!(fixtures::upkeep_counts(&runtime), (1, 0, 1, 1));
    owner.request_shutdown().unwrap();
    let wait = {
        let owner = owner.clone();
        tokio::spawn(async move {
            owner
                .wait_until(Instant::now() + Duration::from_millis(50))
                .await
        })
    };
    eventually(|| !wait.is_finished() && owner.status().phase == CleanupPhase::DrainingCollection)
        .await;
    tokio::time::advance(Duration::from_millis(100)).await;
    eventually(|| wait.is_finished()).await;
    assert!(wait.await.unwrap().is_err());
    assert!(!runtime.is_joined().await);
    assert_ne!(owner.status().phase, CleanupPhase::Complete);
    stall.release();
    upkeep_eventually(&runtime, "released upkeep completion", || {
        fixtures::upkeep_counts(&runtime).1 == 1
    })
    .await;
    upkeep_eventually(&runtime, "owner completion", || {
        owner.status().phase == CleanupPhase::Complete
    })
    .await;
    assert!(runtime.is_joined().await);
    assert!(
        owner
            .wait_until(Instant::now() + Duration::from_secs(1))
            .await
            .is_err(),
        "elapsed caller wait is historical"
    );
    stall.finish();
}

#[tokio::test(start_paused = true)]
async fn sustained_histograms_are_upkept_without_scrapes_or_sample_loss() {
    let owner = ServerCleanupHandle::bootstrap();
    let (context, handle) = prometheus_recorder().unwrap();
    let histogram = register(
        &context,
        "sustained",
        MetricKind::Histogram,
        MetricUnit::One,
        &[],
    )
    .unwrap()
    .histogram()
    .unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    for cycle in 1..=6 {
        for _ in 0..20_000 {
            histogram.record(0.25);
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        upkeep_eventually(&runtime, &format!("sustained cycle {cycle}"), || {
            fixtures::upkeep_counts(&runtime).1 >= cycle
        })
        .await;
    }
    assert_eq!(fixtures::upkeep_counts(&runtime).3, 1);
    assert_eq!(fixtures::render_count(&runtime), 0);
    // Resume real time for blocking-pool completion; paused-time auto-advance
    // must not race the HTTP response timeout.
    tokio::time::resume();
    let response = scrape(&runtime).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let text = std::str::from_utf8(&body).unwrap();
    assert!(text.lines().any(
        |line| line.starts_with("drasi_plugin_probe_sustained_count{") && line.ends_with(" 120000")
    ));
    assert!(text
        .lines()
        .any(|line| line.starts_with("drasi_plugin_probe_sustained_sum{")
            && line.ends_with(" 30000")));
    close(&owner).await;
    assert!(runtime.is_joined().await);
}

#[tokio::test]
async fn prometheus_and_legacy_metrics_cannot_be_retained_together() {
    for legacy_first in [true, false] {
        let owner = ServerCleanupHandle::bootstrap();
        let (_, handle) = prometheus_recorder().unwrap();
        let legacy = Arc::new(tokio::sync::Mutex::new(super::MetricsRuntime::default()));
        if legacy_first {
            owner.retain_metrics(legacy).unwrap();
            assert!(PrometheusRuntime::prepare(&owner, handle).await.is_err());
        } else {
            let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
            assert!(owner.retain_metrics(legacy).is_err());
            close(&owner).await;
            assert!(runtime.is_joined().await);
        }
        close(&owner).await;
    }
}

#[tokio::test(start_paused = true)]
async fn review_quiescing_rejects_scrapes_but_maintains_histograms_until_collection() {
    let owner = ServerCleanupHandle::bootstrap();
    let preparation = owner.admit().unwrap();
    let (context, handle) = prometheus_recorder().unwrap();
    let histogram = register(
        &context,
        "quiescing",
        MetricKind::Histogram,
        MetricUnit::One,
        &[],
    )
    .unwrap()
    .histogram()
    .unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();

    owner.request_shutdown().unwrap();
    let overall_deadline = owner.shutdown_deadline().unwrap();
    assert_eq!(owner.status().phase, CleanupPhase::Quiescing);
    assert_eq!(
        scrape(&runtime).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    for cycle in 1..=5 {
        for _ in 0..1_000 {
            histogram.record(0.25);
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        upkeep_eventually(&runtime, &format!("quiescing upkeep cycle {cycle}"), || {
            fixtures::upkeep_counts(&runtime).1 >= cycle
        })
        .await;
        assert_eq!(owner.status().phase, CleanupPhase::Quiescing);
        assert_eq!(owner.shutdown_deadline(), Some(overall_deadline));
        assert_eq!(
            scrape(&runtime).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
    assert!(fixtures::collection_deadline(&runtime).is_none());
    tokio::time::advance(Duration::from_secs(4)).await;
    assert_eq!(overall_deadline - Instant::now(), Duration::from_secs(1));
    drop(preparation);
    tokio::time::resume();
    close(&owner).await;
    assert!(runtime.is_joined().await);
    assert_eq!(
        fixtures::collection_deadline(&runtime),
        Some(overall_deadline)
    );
    assert_eq!(fixtures::upkeep_counts(&runtime).3, 1);
}

#[tokio::test(start_paused = true)]
async fn review_upkeep_failure_is_recorded_before_shutdown_and_retains_its_typed_cause() {
    let owner = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&owner, handle).await.unwrap();
    fixtures::poison_backend(&runtime);
    tokio::time::advance(Duration::from_secs(5)).await;
    upkeep_eventually(&runtime, "failed upkeep controller exit", || {
        fixtures::controller_finished(&runtime)
    })
    .await;

    assert!(
        owner.admission_is_open(),
        "failure must be observed before owner shutdown"
    );
    let failure = fixtures::failure(&runtime)
        .expect("actual upkeep failure must be recorded before shutdown");
    assert!(fixtures::has_backend_failure(&failure));
    assert!(fixtures::collection_deadline(&runtime).is_none());
    assert!(
        !runtime.is_joined().await,
        "the failed controller join remains owned"
    );
    let response = scrape(&runtime).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "TELEMETRY_SCRAPE_UNAVAILABLE");
    assert!(!String::from_utf8_lossy(&body).contains("poison"));

    tokio::time::resume();
    assert!(timeout(Duration::from_secs(2), owner.shutdown())
        .await
        .unwrap()
        .is_err());
    assert_eq!(owner.status().phase, CleanupPhase::Complete);
    assert!(runtime.is_joined().await);
    assert!(Arc::ptr_eq(&failure, &fixtures::failure(&runtime).unwrap()));
    let error = runtime.wait_for_completion().await.unwrap_err();
    assert!(fixtures::has_backend_failure(&error));
}
