// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::{
    prometheus::{fixtures, PrometheusRuntime},
    recorders::prometheus_recorder,
    MetricsRuntime,
};
use crate::server::{CleanupPhase, ServerCleanupHandle};
use axum::{
    http::{header::CONTENT_TYPE, StatusCode},
    response::{IntoResponse, Response},
};
use drasi_lib::{
    computation::v1::{ComponentId, ComponentRole, ComponentSemanticKind},
    observability::{
        MetricKind, MetricRegistrationContext, MetricUnit, PluginMetricDefinition, RegisteredMetric,
        TelemetryError, TelemetryProfile, TelemetryScope, TelemetrySettings,
    },
};
use std::sync::{Arc, Barrier};
use std::time::Duration;
use tokio::{sync::Mutex, time::Instant};

fn register(
    context: &MetricRegistrationContext,
    leaf: &str,
    kind: MetricKind,
    labels: &[(&str, &str)],
) -> Result<RegisteredMetric, TelemetryError> {
    let scope = TelemetryScope::graph("metrics-test", "graph")?.component(
        ComponentId::try_new("00000000-0000-0000-0000-000000000001").expect("valid fixture UUID"),
        ComponentRole::Source,
        ComponentSemanticKind::Source,
    );
    context.plugin(
        &TelemetrySettings::new(TelemetryProfile::Debug).resolve()?,
        &scope,
        "fixture",
        PluginMetricDefinition {
            leaf,
            kind,
            unit: MetricUnit::One,
            description: "Test metric",
        },
        labels.iter().map(|(key, value)| (key.to_string(), value.to_string())),
    )
}

#[test]
fn normalized_name_collisions_reject_before_backend_registration() {
    let (context, handle) = prometheus_recorder().unwrap();
    register(&context, "a-b", MetricKind::Counter, &[("route", "accepted")])
        .unwrap().counter().unwrap().increment(7);
    let before = context.registrations().counts().unwrap();
    assert!(matches!(
        register(&context, "a_b", MetricKind::Counter, &[("route", "rejected")]),
        Err(TelemetryError::Conflict(_))
    ));
    assert_eq!(context.registrations().counts().unwrap(), before);
    register(&context, "a-b", MetricKind::Counter, &[("route", "accepted")])
        .unwrap().counter().unwrap().increment(2);
    let rendered = handle.render();
    assert!(rendered.contains("route=\"accepted\""));
    assert!(!rendered.contains("rejected"));
    assert!(rendered.lines().any(|line| line.starts_with("drasi_plugin_fixture_a_b{") && line.ends_with(" 9")));
}

#[test]
fn histogram_generated_names_are_reserved_atomically_in_both_orders() {
    for suffix in ["sum", "count", "bucket"] {
        for histogram_first in [false, true] {
            let (context, _) = prometheus_recorder().unwrap();
            let generated = format!("latency_{suffix}");
            let (first, first_kind, second, second_kind) = if histogram_first {
                ("latency", MetricKind::Histogram, generated.as_str(), MetricKind::Counter)
            } else {
                (generated.as_str(), MetricKind::Counter, "latency", MetricKind::Histogram)
            };
            register(&context, first, first_kind, &[]).unwrap();
            assert!(matches!(register(&context, second, second_kind, &[]), Err(TelemetryError::Conflict(_))));
            assert_eq!(context.registrations().counts().unwrap(), (1, 1));
            register(&context, first, first_kind, &[]).unwrap();
            if !histogram_first {
                // Rejection must not leave the histogram's family or other
                // generated names partially reserved.
                register(&context, "latency", MetricKind::Counter, &[]).unwrap();
                let other = if suffix == "bucket" { "latency_count" } else { "latency_bucket" };
                register(&context, other, MetricKind::Gauge, &[]).unwrap();
            }
        }
    }
}

#[test]
fn invalid_normalized_labels_leave_no_guard_reservations() {
    for labels in [
        vec![("foo-bar", "one"), ("foo_bar", "two")],
        vec![("le", "not-a-bucket")],
    ] {
        let (context, handle) = prometheus_recorder().unwrap();
        assert!(matches!(
            register(&context, "latency", MetricKind::Histogram, &labels),
            Err(TelemetryError::Invalid(_))
        ));
        assert_eq!(context.registrations().counts().unwrap(), (0, 0));
        assert!(handle.render().is_empty(), "rejection reached the backend");
        register(&context, "latency", MetricKind::Histogram, &[])
            .unwrap().histogram().unwrap().record(0.5);
        register(&context, "latency", MetricKind::Histogram, &[]).unwrap();
        assert_eq!(context.registrations().counts().unwrap(), (1, 1));
        assert!(handle.render().contains("drasi_plugin_fixture_latency_count{"));
    }
}

#[test]
fn concurrent_normalized_collisions_accept_exactly_one_definition() {
    for names in [("a-b", "a_b"), ("latency", "latency_sum")] {
        let (context, handle) = prometheus_recorder().unwrap();
        let barrier = Barrier::new(2);
        std::thread::scope(|threads| {
            let attempts: Vec<_> = [names.0, names.1].into_iter().map(|name| {
                let context = &context;
                let barrier = &barrier;
                threads.spawn(move || {
                    barrier.wait();
                    let kind = if name == "latency" { MetricKind::Histogram } else { MetricKind::Counter };
                    register(context, name, kind, &[])
                })
            }).collect();
            let results: Vec<_> = attempts.into_iter().map(|attempt| attempt.join().unwrap()).collect();
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
            assert_eq!(results.iter().filter(|result| matches!(result, Err(TelemetryError::Conflict(_)))).count(), 1);
        });
        assert_eq!(context.registrations().counts().unwrap(), (1, 1));
        assert_eq!(handle.render().lines().filter(|line| line.starts_with("# TYPE ")).count(), 1);
    }
}

#[test]
fn second_valued_and_unitless_histograms_share_bounds_without_scaling_or_suffixes() {
    let (context, handle) = prometheus_recorder().unwrap();
    register(&context, "duration", MetricKind::Histogram, &[])
        .unwrap().histogram().unwrap().record(0.000007);
    let scope = TelemetryScope::graph("metrics-test", "graph").unwrap().component(
        ComponentId::try_new("00000000-0000-0000-0000-000000000001").unwrap(),
        ComponentRole::Source,
        ComponentSemanticKind::Source,
    );
    context.plugin(
        &TelemetrySettings::new(TelemetryProfile::Debug).resolve().unwrap(),
        &scope,
        "fixture",
        PluginMetricDefinition {
            leaf: "seconds",
            kind: MetricKind::Histogram,
            unit: MetricUnit::Seconds,
            description: "Seconds without scaling",
        },
        [],
    ).unwrap().histogram().unwrap().record(0.000007);
    let rendered = handle.render();
    for metric in ["drasi_plugin_fixture_duration", "drasi_plugin_fixture_seconds"] {
        let buckets: Vec<_> = rendered.lines()
            .filter(|line| line.starts_with(&format!("{metric}_bucket{{")))
            .map(|line| {
                let bound = line.split("le=\"").nth(1).unwrap().split('"').next().unwrap();
                let count = line.rsplit(' ').next().unwrap().parse::<u64>().unwrap();
                (bound, count)
            }).collect();
        assert_eq!(buckets, vec![
            ("0.000001", 0), ("0.000005", 0), ("0.00001", 1),
            ("0.00005", 1), ("0.0001", 1), ("0.0005", 1), ("0.001", 1),
            ("0.005", 1), ("0.01", 1), ("0.05", 1), ("0.1", 1),
            ("0.5", 1), ("1", 1), ("5", 1), ("10", 1), ("100", 1),
            ("1000", 1), ("+Inf", 1),
        ]);
        assert!(rendered.lines().any(|line| line.starts_with(&format!("{metric}_sum{{")) && line.ends_with(" 0.000007")));
    }
    assert!(!rendered.contains("seconds_seconds"));
}

async fn response(runtime: &PrometheusRuntime) -> Response {
    match runtime.scrape().await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

async fn body(response: Response) -> String {
    String::from_utf8(axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

async fn renders(runtime: &PrometheusRuntime, count: usize) {
    for _ in 0..100_000 {
        if fixtures::render_count(runtime) == count {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("render workers did not reach {count}");
}

#[tokio::test]
async fn four_slots_survive_waiter_cancellation_and_do_not_queue_a_fifth() {
    let cleanup = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&cleanup, handle).await.unwrap();
    let gate = fixtures::hold_backend(&runtime);
    let mut waiters = Vec::new();
    for count in 1..=4 {
        let scrape_runtime = runtime.clone();
        waiters.push(tokio::spawn(async move { response(&scrape_runtime).await }));
        renders(&runtime, count).await;
    }
    let overload = response(&runtime).await;
    assert_eq!(overload.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(body(overload).await.contains("TELEMETRY_SCRAPE_UNAVAILABLE"));
    let cancelled = waiters.remove(0);
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    assert_eq!(response(&runtime).await.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(fixtures::render_count(&runtime), 4);
    drop(gate);
    for waiter in waiters {
        assert_eq!(waiter.await.unwrap().status(), StatusCode::OK);
    }
    fixtures::join_render_workers(&runtime).await;
    assert_eq!(response(&runtime).await.status(), StatusCode::OK);
    cleanup.shutdown().await.unwrap();
    assert!(runtime.is_joined().await);
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
}

#[tokio::test(start_paused = true)]
async fn five_second_wait_timeout_keeps_the_actual_worker_owned() {
    let cleanup = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&cleanup, handle).await.unwrap();
    let gate = fixtures::hold_backend(&runtime);
    let scrape_runtime = runtime.clone();
    let waiter = tokio::spawn(async move { response(&scrape_runtime).await });
    renders(&runtime, 1).await;
    tokio::time::advance(Duration::from_secs(4)).await;
    assert!(!waiter.is_finished(), "scrape wait expired before five seconds");
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(waiter.await.unwrap().status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(fixtures::render_count(&runtime), 1);
    runtime.request_shutdown(Instant::now()).unwrap();
    assert!(runtime.wait_shutdown_until(Instant::now()).await.is_err());
    assert!(!runtime.is_joined().await);
    assert_eq!(response(&runtime).await.status(), StatusCode::SERVICE_UNAVAILABLE);
    drop(gate);
    runtime.wait_for_completion().await.unwrap();
    cleanup.shutdown().await.unwrap();
    assert!(runtime.is_joined().await);
}

#[tokio::test]
async fn failed_render_returns_500_instead_of_an_empty_success() {
    let cleanup = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&cleanup, handle).await.unwrap();
    fixtures::poison_backend(&runtime);
    let failed = response(&runtime).await;
    assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body(failed).await;
    assert!(body.contains("TELEMETRY_RENDER_FAILED"));
    assert!(!body.contains("poison"), "technical cause leaked to the public");
    assert!(cleanup.shutdown().await.is_err(), "cleanup hid the render failure");
    assert!(runtime.is_joined().await);
}

#[tokio::test]
async fn empty_recorder_has_valid_successful_exposition() {
    let cleanup = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&cleanup, handle).await.unwrap();
    let successful = response(&runtime).await;
    assert_eq!(successful.status(), StatusCode::OK);
    assert_eq!(successful.headers()[CONTENT_TYPE], "text/plain; version=0.0.4; charset=utf-8");
    assert!(body(successful).await.is_empty());
    cleanup.shutdown().await.unwrap();
    assert!(runtime.is_joined().await);
}

#[tokio::test(start_paused = true)]
async fn upkeep_runs_without_scrapers_and_cleanup_joins_its_actual_work() {
    let cleanup = ServerCleanupHandle::bootstrap();
    let (context, handle) = prometheus_recorder().unwrap();
    let histogram = register(&context, "duration", MetricKind::Histogram, &[]).unwrap().histogram().unwrap();
    let runtime = PrometheusRuntime::prepare(&cleanup, handle).await.unwrap();
    let gate = fixtures::hold_backend(&runtime);
    for _ in 0..10_000 {
        histogram.record(0.025);
    }
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let work = fixtures::wait_upkeep_worker(&runtime).await;
    assert_eq!(fixtures::render_count(&runtime), 0);
    for _ in 0..3 {
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(fixtures::wait_upkeep_worker(&runtime).await, work, "upkeep overlapped its stalled worker");
    }
    assert_eq!(fixtures::completed_upkeep(&runtime), 0);
    cleanup.request_shutdown().unwrap();
    let wait = cleanup.wait_until(Instant::now() + Duration::from_secs(1));
    tokio::pin!(wait);
    tokio::select! {
        result = &mut wait => panic!("upkeep still owns actual work: {result:?}"),
        _ = tokio::task::yield_now() => {}
    }
    tokio::time::advance(Duration::from_secs(2)).await;
    assert!(wait.await.is_err());
    assert!(!runtime.is_joined().await);
    drop(gate);
    // The caller's elapsed wait is historical, not permission to abort upkeep.
    assert!(cleanup.wait_for_completion().await.is_err());
    assert!(runtime.is_joined().await);
    assert_eq!(fixtures::completed_upkeep(&runtime), 1);
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
}

#[tokio::test(start_paused = true)]
async fn sustained_no_scraper_histograms_receive_periodic_upkeep() {
    let cleanup = ServerCleanupHandle::bootstrap();
    let (context, handle) = prometheus_recorder().unwrap();
    let histogram = register(&context, "duration", MetricKind::Histogram, &[])
        .unwrap().histogram().unwrap();
    let runtime = PrometheusRuntime::prepare(&cleanup, handle).await.unwrap();
    for batch in 1..=12 {
        for _ in 0..10_000 {
            histogram.record(0.025);
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        for _ in 0..100_000 {
            if fixtures::completed_upkeep(&runtime) >= batch {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(fixtures::completed_upkeep(&runtime), batch);
        assert_eq!(fixtures::render_count(&runtime), 0);
    }
    let rendered = body(response(&runtime).await).await;
    assert!(rendered.lines().any(|line| line.starts_with("drasi_plugin_fixture_duration_count{") && line.ends_with(" 120000")));
    cleanup.shutdown().await.unwrap();
}

#[tokio::test]
async fn cleanup_rejects_prometheus_and_otlp_owners_in_both_orders() {
    let cleanup = ServerCleanupHandle::bootstrap();
    cleanup.retain_metrics(Arc::new(Mutex::new(MetricsRuntime::default()))).unwrap();
    let (_, handle) = prometheus_recorder().unwrap();
    assert!(PrometheusRuntime::prepare(&cleanup, handle).await.is_err());
    cleanup.shutdown().await.unwrap();

    let cleanup = ServerCleanupHandle::bootstrap();
    let (_, handle) = prometheus_recorder().unwrap();
    let runtime = PrometheusRuntime::prepare(&cleanup, handle).await.unwrap();
    assert!(cleanup.retain_metrics(Arc::new(Mutex::new(MetricsRuntime::default()))).is_err());
    cleanup.shutdown().await.unwrap();
    assert!(runtime.is_joined().await);
}
