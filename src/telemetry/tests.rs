// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::*;
use axum::{extract::State, routing::post, Json, Router};
use drasi_lib::{DrasiLib, Query};
use drasi_source_application::{
    ApplicationSource, ApplicationSourceConfig, ApplicationSourceHandle, PropertyMapBuilder,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{mpsc, Notify};

#[derive(Default)]
pub(super) struct ShutdownProbe {
    pub(super) sdk: AtomicUsize,
    pub(super) exporter: AtomicUsize,
    pub(super) periodic_exports: AtomicUsize,
    pub(super) preparation: std::sync::Mutex<Option<Arc<PreparationGate>>>,
}

#[derive(Default)]
pub(super) struct PreparationGate {
    pub(super) entered: AtomicBool,
    pub(super) released: std::sync::Mutex<bool>,
    pub(super) changed: std::sync::Condvar,
}

impl ShutdownProbe {
    pub(super) fn pause_preparation(&self) {
        let gate = self.preparation.lock().unwrap().clone();
        if let Some(gate) = gate {
            let mut released = gate.released.lock().unwrap();
            gate.entered.store(true, Ordering::SeqCst);
            while !*released {
                released = gate.changed.wait(released).unwrap();
            }
        }
    }
}

struct CollectorState {
    received: mpsc::UnboundedSender<Value>,
    stalled: AtomicBool,
    release: Notify,
}

async fn collect(State(state): State<Arc<CollectorState>>, Json(body): Json<Value>) -> Json<Value> {
    state.received.send(body).expect("test collector receiver");
    while state.stalled.load(Ordering::Acquire) {
        let released = state.release.notified();
        if !state.stalled.load(Ordering::Acquire) {
            break;
        }
        released.await;
    }
    Json(json!({}))
}

pub(super) async fn instance(id: &str) -> Result<(Arc<DrasiLib>, ApplicationSourceHandle)> {
    let input = Arc::new(std::sync::OnceLock::new());
    let assigned_input = input.clone();
    let core = Arc::new(
        crate::query_test_support::builder()
            .with_id(id)
            .with_source_factory("input", true, move |id| {
                let (source, handle) = ApplicationSource::new(
                    id.as_str(),
                    ApplicationSourceConfig {
                        properties: Default::default(),
                        durability: None,
                    },
                )?;
                assigned_input
                    .set(handle)
                    .map_err(|_| anyhow::anyhow!("source constructor ran twice"))?;
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
            .await?,
    );
    core.start().await?;
    let query = core.component_by_name("items")?;
    tokio::time::timeout(Duration::from_secs(5), query.wait_started()).await??;
    core.get_query_results_by_id(query.id()).await?;
    let input = input
        .get()
        .context("source constructor did not publish its input handle")?
        .clone();
    Ok((core, input))
}

fn attribute<'a>(attributes: &'a Value, name: &str) -> Option<&'a Value> {
    attributes
        .as_array()?
        .iter()
        .find_map(|attribute| (attribute["key"] == name).then_some(&attribute["value"]))
}

fn points(body: &Value) -> Vec<&Value> {
    body["resourceMetrics"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|resource| resource["scopeMetrics"].as_array().into_iter().flatten())
        .flat_map(|scope| scope["metrics"].as_array().into_iter().flatten())
        .filter(|metric| metric["name"] == "drasi.query.progress.sequence")
        .flat_map(|metric| {
            metric["gauge"]["dataPoints"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .collect()
}

fn sequence(body: &Value, instance: &str, stage: &str) -> Option<u64> {
    points(body).into_iter().find_map(|point| {
        let attributes = &point["attributes"];
        if attribute(attributes, "drasi.instance.id")?["stringValue"] != instance
            || attribute(attributes, "drasi.progress.stage")?["stringValue"] != stage
        {
            return None;
        }
        point["asInt"].as_str()?.parse().ok()
    })
}

async fn wait_capture(
    receiver: &mut mpsc::UnboundedReceiver<Value>,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let body = receiver.recv().await.context("metrics collector closed")?;
            if predicate(&body) {
                return Ok(body);
            }
        }
    })
    .await
    .context("required OTLP progress was not exported")?
}

async fn exported_progress_and_owned_shutdown() -> Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}/v1/metrics", listener.local_addr()?);
    let (received, mut receiver) = mpsc::unbounded_channel();
    let collector = Arc::new(CollectorState {
        received,
        stalled: AtomicBool::new(false),
        release: Notify::new(),
    });
    let app = Router::new()
        .route("/v1/metrics", post(collect))
        .with_state(collector.clone());
    let (stop, stopping) = oneshot::channel();
    let mut server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopping.await;
            })
            .await
    });
    let (first, input) = instance("first").await?;
    let (second, second_input) = instance("second").await?;
    let registry = InstanceRegistry::new();
    registry
        .add("first".into(), first.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    registry
        .add("second".into(), second.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let mut metrics = MetricsRuntime::default();
    metrics
        .start(
            MetricsConfig {
                endpoint: ConfigValue::Static(endpoint),
                export_interval_ms: 100,
                export_timeout_ms: 1_000,
            },
            &registry,
        )
        .await?;

    input
        .send_node_insert(
            "one",
            vec!["Item"],
            PropertyMapBuilder::new().with_integer("value", 7).build(),
        )
        .await?;
    input
        .send_node_insert("ignored", vec!["Other"], PropertyMapBuilder::new().build())
        .await?;
    let body = wait_capture(&mut receiver, |body| {
        sequence(body, "first", "input.committed") == Some(2)
            && sequence(body, "first", "output.produced") == Some(1)
            && sequence(body, "second", "output.produced") == Some(0)
    })
    .await?;
    let resource = &body["resourceMetrics"][0]["resource"]["attributes"];
    assert_eq!(
        attribute(resource, "service.name").unwrap()["stringValue"],
        "drasi-server"
    );
    assert!(
        attribute(resource, "service.instance.id").unwrap()["stringValue"]
            .as_str()
            .is_some()
    );
    assert_eq!(
        attribute(resource, "process.pid").unwrap()["intValue"].as_str(),
        Some(std::process::id().to_string().as_str())
    );
    for point in points(&body) {
        let attributes = &point["attributes"];
        assert_eq!(
            attribute(attributes, "drasi.progress.persistent").unwrap()["boolValue"],
            false
        );
        for key in [
            "drasi.component.generation",
            "drasi.component.operation",
            "drasi.query.output.generation",
            "drasi.query.config_hash",
        ] {
            assert!(attribute(attributes, key).unwrap()["stringValue"]
                .as_str()
                .is_some());
        }
    }
    registry
        .remove("first")
        .await
        .context("registered first instance")?;
    let body = wait_capture(&mut receiver, |body| {
        sequence(body, "first", "output.produced").is_none()
            && sequence(body, "second", "output.produced") == Some(0)
    })
    .await?;
    assert!(points(&body)
        .iter()
        .all(
            |point| attribute(&point["attributes"], "drasi.instance.id").unwrap()["stringValue"]
                == "second"
        ));
    first.shutdown().await?;

    collector.stalled.store(true, Ordering::Release);
    while receiver.try_recv().is_ok() {}
    wait_capture(&mut receiver, |_| true).await?;
    second_input
        .send_node_insert(
            "during-export",
            vec!["Item"],
            PropertyMapBuilder::new().with_integer("value", 99).build(),
        )
        .await?;
    let query_id = second.component_by_name("items")?.id().clone();
    tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            if second.get_query_results_by_id(&query_id).await? == vec![json!({"value": 99})] {
                break Result::<()>::Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    assert!(tokio::time::timeout(Duration::from_millis(20), async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        metrics.request_shutdown(deadline)?;
        metrics.wait_shutdown_until(deadline).await
    },)
    .await
    .is_err());
    assert!(metrics.worker.read().await.is_some());
    assert_eq!(metrics.shutdown_probe.sdk.load(Ordering::SeqCst), 0);
    assert_eq!(metrics.shutdown_probe.exporter.load(Ordering::SeqCst), 0);
    let expired = metrics
        .wait_shutdown_until(tokio::time::Instant::now())
        .await
        .expect_err("stalled actual export must retain its join");
    assert!(expired.downcast_ref::<MetricsCleanupError>().is_some());
    assert!(metrics.worker.read().await.is_some());
    assert_eq!(second.get_query_progress_metrics()?.len(), 1);
    collector.stalled.store(false, Ordering::Release);
    collector.release.notify_waiters();
    tokio::time::timeout(Duration::from_secs(5), metrics.shutdown()).await??;
    assert!(metrics.worker.read().await.is_none());
    assert_eq!(metrics.shutdown_probe.sdk.load(Ordering::SeqCst), 1);
    assert_eq!(metrics.shutdown_probe.exporter.load(Ordering::SeqCst), 1);
    while receiver.try_recv().is_ok() {}
    metrics.shutdown().await?;
    assert_eq!(metrics.shutdown_probe.sdk.load(Ordering::SeqCst), 1);
    assert_eq!(metrics.shutdown_probe.exporter.load(Ordering::SeqCst), 1);
    assert!(
        receiver.try_recv().is_err(),
        "a repeated join must not collect/export again"
    );
    second.shutdown().await?;
    stop.send(())
        .map_err(|_| anyhow::anyhow!("collector already stopped"))?;
    tokio::time::timeout(Duration::from_secs(5), &mut server).await???;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn expired_metrics_phase_skips_fresh_collection_but_joins_cleanup() -> Result<()> {
    let collector = wiremock::MockServer::start().await;
    let registry = InstanceRegistry::new();
    let (core, _) = instance("expired-phase").await?;
    registry
        .add("expired-phase".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let mut metrics = MetricsRuntime::default();
    metrics
        .start(
            MetricsConfig {
                endpoint: ConfigValue::Static(format!("{}/v1/metrics", collector.uri())),
                export_interval_ms: 60_000,
                export_timeout_ms: 100,
            },
            &registry,
        )
        .await?;
    metrics.request_shutdown(tokio::time::Instant::now())?;
    let result = metrics
        .wait_shutdown_until(tokio::time::Instant::now() + Duration::from_secs(5))
        .await;
    let terminal = result.expect_err("an expired collection is not flush success");
    assert!(
        terminal.chain().any(|cause| {
            cause
                .downcast_ref::<drasi_lib::error::OperationFailures>()
                .is_some_and(|failures| {
                    failures.failures().iter().any(|failure| {
                        matches!(
                            failure.downcast_ref::<MetricsCleanupError>(),
                            Some(MetricsCleanupError::PhaseExpired),
                        )
                    })
                })
        }),
        "lost typed phase expiry: {terminal:#}"
    );
    assert!(metrics.is_joined().await);
    assert!(collector
        .received_requests()
        .await
        .context("requests")?
        .is_empty());
    assert!(metrics
        .wait_shutdown_until(tokio::time::Instant::now())
        .await
        .is_err());
    core.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn otlp_progress_and_cancelled_shutdown_on_current_thread() -> Result<()> {
    exported_progress_and_owned_shutdown().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn otlp_progress_and_cancelled_shutdown_on_multiple_threads() -> Result<()> {
    exported_progress_and_owned_shutdown().await
}

#[test]
fn metrics_configuration_is_opt_in_validated_and_round_trips() -> Result<()> {
    let mut config = crate::DrasiServerConfig::default();
    assert!(config.metrics.is_none());
    config.metrics = Some(MetricsConfig {
        endpoint: ConfigValue::Static("http://localhost:4318/v1/metrics".into()),
        export_interval_ms: 1_000,
        export_timeout_ms: 500,
    });
    config.validate()?;
    let yaml = serde_yaml::to_string(&config)?;
    let restored: crate::DrasiServerConfig = serde_yaml::from_str(&yaml)?;
    assert_eq!(restored.metrics.as_ref().unwrap().export_interval_ms, 1_000);
    for endpoint in [
        "file:///tmp/metrics",
        "http://user:password@localhost/v1/metrics",
        "http://localhost/v1/metrics?token=secret",
        "http://localhost/#fragment",
    ] {
        config.metrics.as_mut().unwrap().endpoint = ConfigValue::Static(endpoint.into());
        assert!(config.validate().is_err());
    }
    Ok(())
}
