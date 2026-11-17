// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

use anyhow::{Context, Result};
use drasi_lib::{DrasiLib, DrasiLibBuilder};
use drasi_server::{config::telemetry::TelemetryConfig, DrasiServerBuilder};
use drasi_source_application::{ApplicationSource, ApplicationSourceConfig};
use opentelemetry_proto::tonic::{
    collector::{metrics::v1::ExportMetricsServiceRequest, trace::v1::ExportTraceServiceRequest},
    common::v1::{any_value::Value, KeyValue},
};
use prost::Message;
use std::time::Duration;
use tokio::time::{timeout, Instant};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

const WAIT: Duration = Duration::from_secs(8);
const CASE: &str = "DRASI_OBS05_TRACE_CASE";
const FLAVOR: &str = "DRASI_OBS05_TRACE_FLAVOR";

#[path = "observability_tracing/native.rs"]
mod native;

fn child(case: &str) -> Result<()> {
    for flavor in ["current_thread", "multi_thread"] {
        let mut command = std::process::Command::new(std::env::current_exe()?);
        let log = match case {
            "unsampled"
            | "native-export"
            | "native-unsampled-prometheus"
            | "native-unsampled-otlp" => "info",
            "native-fallback" => "warn",
            _ => "off",
        };
        command
            .args([
                "--exact",
                "trace_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CASE, case)
            .env(FLAVOR, flavor)
            .env("RUST_LOG", log);
        if case == "hostile" {
            command
                .env(
                    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                    "http://127.0.0.1:1/wrong",
                )
                .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:1/wrong")
                .env(
                    "OTEL_EXPORTER_OTLP_TRACES_HEADERS",
                    "authorization=hostile,x-secret=hostile",
                )
                .env("OTEL_EXPORTER_OTLP_HEADERS", "authorization=hostile")
                .env("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "http/json")
                .env("OTEL_EXPORTER_OTLP_TRACES_TIMEOUT", "1")
                .env("OTEL_SERVICE_NAME", "hostile")
                .env(
                    "OTEL_RESOURCE_ATTRIBUTES",
                    "service.instance.id=hostile,secret=hostile",
                )
                .env("OTEL_TRACES_SAMPLER", "always_off")
                .env("OTEL_BSP_MAX_QUEUE_SIZE", "1");
        }
        let output = bounded_output(command, case, flavor)?;
        assert!(
            output.status.success(),
            "{case}/{flavor}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if case == "unsampled" {
            assert_eq!(
                String::from_utf8_lossy(&output.stdout)
                    .matches("OBS05 independent log proof")
                    .count(),
                1
            );
        }
        if case == "native-fallback" {
            let console = String::from_utf8_lossy(&output.stdout);
            assert!(
                console.contains("has no shared telemetry extension"),
                "missing Host SDK capability diagnostic: {console}"
            );
        }
    }
    Ok(())
}

/// A child that fails before its owned cleanup can otherwise wait forever while
/// dropping its runtime; report it instead of hanging the parent.
fn bounded_output(
    mut command: std::process::Command,
    case: &str,
    flavor: &str,
) -> Result<std::process::Output> {
    let directory = tempfile::tempdir()?;
    let (stdout, stderr) = (directory.path().join("out"), directory.path().join("err"));
    let mut process = command
        .stdout(std::fs::File::create(&stdout)?)
        .stderr(std::fs::File::create(&stderr)?)
        .spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = process.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            process.kill()?;
            process.wait()?;
            anyhow::bail!(
                "{case}/{flavor} timed out:\n{}{}",
                std::fs::read_to_string(&stdout).unwrap_or_default(),
                std::fs::read_to_string(&stderr).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(std::process::Output {
        status,
        stdout: std::fs::read(&stdout)?,
        stderr: std::fs::read(&stderr)?,
    })
}

#[test]
#[ignore = "requires DRASI_OBS05_FIXTURES_DIR with matched native fixtures"]
fn native_plugins_export_once_with_host_assigned_scope() -> Result<()> {
    child("native-export")
}
#[test]
#[ignore = "requires DRASI_OBS05_FIXTURES_DIR with matched native fixtures"]
fn native_plugin_retirement_fences_retained_callbacks() -> Result<()> {
    child("native-retire")
}
#[test]
#[ignore = "requires DRASI_OBS05_FIXTURES_DIR with matched native fixtures"]
fn native_capability_fallback_keeps_host_spans_and_reports_limits() -> Result<()> {
    child("native-fallback")
}
#[test]
#[ignore = "requires DRASI_OBS05_FIXTURES_DIR with matched native fixtures"]
fn native_unsampled_logs_and_metrics_survive_both_backends() -> Result<()> {
    child("native-unsampled-prometheus")?;
    child("native-unsampled-otlp")
}
#[test]
#[ignore = "requires DRASI_OBS05_FIXTURES_DIR with matched native fixtures"]
fn native_http_and_grpc_parents_survive_source_polling() -> Result<()> {
    native::native_ingress()
}

#[test]
fn configured_trace_and_metric_requests_share_resource() -> Result<()> {
    child("active")
}
#[test]
fn configured_trace_never_run_and_failed_build_send_no_requests() -> Result<()> {
    child("never-run")?;
    child("failed")
}
#[test]
fn configured_trace_bind_failure_rolls_back_all_collection() -> Result<()> {
    child("bind-failure")
}
#[test]
fn configured_trace_transport_ignores_hostile_environment() -> Result<()> {
    child("hostile")
}
#[test]
fn configured_unsampled_traces_keep_metrics_and_logs() -> Result<()> {
    child("unsampled")
}
#[test]
fn configured_off_and_empty_destinations_do_not_export() -> Result<()> {
    child("off")?;
    child("empty")
}

fn source(id: &str) -> DrasiLibBuilder {
    DrasiLib::builder()
        .with_id(id)
        .with_source_factory("input", true, |id| {
            let (source, _) = ApplicationSource::new(
                id.as_str(),
                ApplicationSourceConfig {
                    properties: Default::default(),
                    durability: None,
                },
            )?;
            Ok(source)
        })
}

fn attribute(values: &[KeyValue], key: &str) -> Option<String> {
    values
        .iter()
        .find(|value| value.key == key)
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match &value.value {
            Some(Value::StringValue(value)) => Some(value.clone()),
            _ => None,
        })
}

#[test]
#[ignore = "isolated global-installation subprocess entry point"]
fn trace_child() -> Result<()> {
    let mut runtime = if std::env::var(FLAVOR)? == "current_thread" {
        tokio::runtime::Builder::new_current_thread()
    } else {
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        builder.worker_threads(2);
        builder
    };
    runtime
        .enable_all()
        .build()?
        .block_on(scenario(&std::env::var(CASE)?))
}

async fn scenario(case: &str) -> Result<()> {
    if case.starts_with("native-") {
        return native::scenario(case).await;
    }
    let receiver = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&receiver)
        .await;
    let inactive = matches!(case, "off" | "empty");
    let config: TelemetryConfig = serde_json::from_value(serde_json::json!({
        "profile": if case == "off" { "off" } else { "basic" },
        "tracing": {
            "endpoint": if case == "empty" { String::new() } else { format!("{}/custom/traces", receiver.uri()) },
            "protocol": "http/protobuf", "serviceName": "obs05-public",
            "sampleRatio": if case == "unsampled" { 0.0 } else { 1.0 }
        },
        "metrics": {"backend": if inactive { "none" } else { "otlp" }, "otlp": {
            "endpoint": format!("{}/custom/metrics", receiver.uri()),
            "protocol": "http/protobuf", "exportInterval": 1, "exportTimeout": 1
        }}
    }))?;
    let mut builder = DrasiServerBuilder::new()
        .with_id("empty")
        .disable_ui()
        .add_instance_builder(source("first"))
        .add_instance_builder(source(if case == "failed" { "first" } else { "second" }))
        .with_telemetry(config);
    if matches!(case, "never-run" | "failed") {
        let preparation = builder.prepare();
        let cleanup = preparation.cleanup_handle();
        let result = preparation.finish().await;
        if case == "failed" {
            assert!(result.is_err());
        } else {
            let server = result?;
            tokio::time::sleep(Duration::from_millis(1100)).await;
            assert!(receiver.received_requests().await.unwrap().is_empty());
            server.shutdown().await?;
        }
        assert_eq!(
            cleanup.status().phase,
            drasi_server::server::CleanupPhase::Complete
        );
        assert!(receiver.received_requests().await.unwrap().is_empty());
        return Ok(());
    }
    if case == "bind-failure" {
        let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
        builder = builder.with_host_port("127.0.0.1", occupied.local_addr()?.port());
        let server = builder.build().await?;
        let cleanup = server.cleanup_handle();
        assert!(server.run().await.is_err());
        assert!(cleanup.wait_until(Instant::now() + WAIT).await.is_err());
        assert_eq!(
            cleanup.status().phase,
            drasi_server::server::CleanupPhase::Complete
        );
        assert!(receiver.received_requests().await.unwrap().is_empty());
        return Ok(());
    }
    let built = builder.build_with_owned_handles().await?;
    for id in ["first", "second"] {
        timeout(
            WAIT,
            built.servers[id].component_by_name("input")?.wait_started(),
        )
        .await??;
        assert_eq!(
            built.servers[id].telemetry().context_adapter().is_some(),
            !inactive
        );
    }
    if inactive {
        built.shutdown().await?;
        assert!(receiver.received_requests().await.unwrap().is_empty());
        return Ok(());
    }
    log::info!("OBS05 independent log proof");
    timeout(WAIT, async {
        loop {
            let requests = receiver.received_requests().await.unwrap();
            let metrics = requests
                .iter()
                .any(|request| request.url.path() == "/custom/metrics");
            let traces = requests
                .iter()
                .any(|request| request.url.path() == "/custom/traces");
            if metrics && (traces || case == "unsampled") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    built.shutdown().await?;
    let requests = receiver.received_requests().await.unwrap();
    let metrics = requests
        .iter()
        .find(|request| request.url.path() == "/custom/metrics")
        .context("metrics request")?;
    let metrics = ExportMetricsServiceRequest::decode(metrics.body.as_slice())?;
    let metric_resource = metrics.resource_metrics[0]
        .resource
        .as_ref()
        .context("metric resource")?;
    assert_eq!(
        attribute(&metric_resource.attributes, "service.name").as_deref(),
        Some("obs05-public")
    );
    let instance = attribute(&metric_resource.attributes, "service.instance.id")
        .context("service instance")?;
    assert_ne!(instance, "hostile");
    let traces: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == "/custom/traces")
        .collect();
    if case == "unsampled" {
        assert!(traces.is_empty());
        return Ok(());
    }
    let mut identities = std::collections::BTreeSet::new();
    for request in traces {
        assert_eq!(request.headers["content-type"], "application/x-protobuf");
        assert!(!request.headers.contains_key("authorization"));
        assert!(!request.headers.contains_key("x-secret"));
        let trace = ExportTraceServiceRequest::decode(request.body.as_slice())?;
        for resource in trace.resource_spans {
            let resource_attributes = &resource.resource.context("trace resource")?.attributes;
            assert_eq!(
                attribute(resource_attributes, "service.instance.id").as_deref(),
                Some(instance.as_str())
            );
            for scope in resource.scope_spans {
                for span in scope.spans {
                    if let Some(id) = attribute(&span.attributes, "instance_id") {
                        identities.insert(id);
                    }
                    assert!(!span.trace_id.iter().all(|byte| *byte == 0));
                }
            }
        }
    }
    assert!(identities.contains("first") && identities.contains("second"));
    Ok(())
}
