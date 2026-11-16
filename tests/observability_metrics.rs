// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::{
    computation::v1::*,
    observability::{
        ComponentLogRuntime, LogPolicy, MetricKind, MetricUnit, PluginMetricDefinition,
        TelemetryProfile, TelemetryScope, TelemetrySettings,
    },
    DrasiLib, Query,
};
use drasi_server::{
    config::telemetry::TelemetryConfig, server::CleanupPhase, DrasiServerBuilder, TelemetryBindings,
};
use opentelemetry_proto::tonic::{
    collector::metrics::v1::ExportMetricsServiceRequest, common::v1::any_value::Value,
    metrics::v1::metric::Data,
};
use prost::Message;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::{timeout, Instant};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

mod query_plugin;

const WAIT: Duration = Duration::from_secs(8);
const CASE: &str = "DRASI_OBS04_METRICS_CASE";
const FLAVOR: &str = "DRASI_OBS04_METRICS_FLAVOR";

fn child(case: &str) -> Result<()> {
    for flavor in ["current_thread", "multi_thread"] {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "metrics_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CASE, case)
            .env(FLAVOR, flavor)
            .env("RUST_LOG", "info")
            .output()?;
        assert!(
            output.status.success(),
            "{case}/{flavor}:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
fn configured_owned_handles_bind_first_factories_and_activate_before_return() -> Result<()> {
    child("owned")
}

#[test]
fn configured_build_accumulates_until_run_and_keeps_one_resource() -> Result<()> {
    child("run")
}

#[test]
fn configured_never_run_cleanup_sends_no_requests() -> Result<()> {
    child("never-run")
}

#[test]
fn configured_failed_build_joins_without_exporting() -> Result<()> {
    child("failed")
}

#[test]
fn configured_otlp_and_borrowed_bindings_reject_before_factories() -> Result<()> {
    child("exclusive")
}

#[test]
#[ignore = "isolated global-installation subprocess entry point"]
fn metrics_child() -> Result<()> {
    let mut runtime = match std::env::var(FLAVOR)?.as_str() {
        "current_thread" => tokio::runtime::Builder::new_current_thread(),
        "multi_thread" => {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.worker_threads(2);
            builder
        }
        flavor => anyhow::bail!("unknown runtime flavor: {flavor}"),
    };
    runtime
        .enable_all()
        .build()?
        .block_on(scenario(&std::env::var(CASE)?))
}

struct GlobalRecorder(Arc<Mutex<Vec<String>>>);

impl metrics::Recorder for GlobalRecorder {
    fn describe_counter(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    fn describe_gauge(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    fn describe_histogram(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    fn register_counter(&self, key: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Counter {
        self.0.lock().unwrap().push(key.name().into());
        metrics::Counter::noop()
    }
    fn register_gauge(&self, key: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Gauge {
        self.0.lock().unwrap().push(key.name().into());
        metrics::Gauge::noop()
    }
    fn register_histogram(
        &self,
        key: &metrics::Key,
        _: &metrics::Metadata<'_>,
    ) -> metrics::Histogram {
        self.0.lock().unwrap().push(key.name().into());
        metrics::Histogram::noop()
    }
}

struct FirstFactory {
    inner: Arc<dyn ComponentFactory>,
    observed: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ComponentFactory for FirstFactory {
    fn descriptor(&self) -> &FactoryDescriptor {
        self.inner.descriptor()
    }
    fn query_authoring(&self) -> Option<&dyn QueryAuthoring> {
        self.inner.query_authoring()
    }
    fn validate(&self, specification: &ComponentSpecification) -> Result<()> {
        self.inner.validate(specification)
    }
    fn record_schemas(&self) -> Vec<Arc<Schema>> {
        self.inner.record_schemas()
    }
    fn binds_inputs(&self) -> bool {
        self.inner.binds_inputs()
    }
    async fn create(
        &self,
        context: ConstructionContext,
    ) -> Result<ConstructedComponent, ComponentCreationError> {
        let observe = || -> Result<()> {
            let metrics = context
                .telemetry()
                .metric_context()
                .context("first factory missing metric context")?;
            let scope =
                TelemetryScope::graph(context.instance_id.clone(), context.graph_id.clone())?
                    .component(
                        context.component_id.clone(),
                        ComponentRole::Transformer,
                        ComponentSemanticKind::Query,
                    );
            metrics
                .plugin(
                    context.telemetry().policy(),
                    &scope,
                    "task4",
                    PluginMetricDefinition {
                        leaf: "created",
                        kind: MetricKind::Counter,
                        unit: MetricUnit::One,
                        description: "Factory constructions",
                    },
                    [],
                )?
                .counter()?
                .increment(1);
            self.observed
                .lock()
                .unwrap()
                .push(context.instance_id.to_string());
            Ok(())
        };
        observe().map_err(ComponentCreationError::terminal)?;
        self.inner.create(context).await
    }
}

async fn scenario(case: &str) -> Result<()> {
    let globals = Arc::new(Mutex::new(Vec::new()));
    metrics::set_global_recorder(GlobalRecorder(globals.clone()))
        .map_err(|_| anyhow::anyhow!("global test recorder already installed"))?;
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&collector)
        .await;
    let config: TelemetryConfig = serde_json::from_value(serde_json::json!({
        "profile": "basic",
        "tracing": { "serviceName": "task4-public" },
        "metrics": { "backend": "otlp", "otlp": {
            "endpoint": format!("{}/custom/metrics", collector.uri()),
            "protocol": "http/protobuf", "exportInterval": 1, "exportTimeout": 1
        }}
    }))?;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let factory = Arc::new(FirstFactory {
        inner: query_plugin::factory(),
        observed: observed.clone(),
    });
    let query = || Query::cypher("probe").query("MATCH (n) RETURN n").build();
    let builder = DrasiServerBuilder::new()
        .with_id("first")
        .with_query_factory(factory.clone())
        .with_simple_query("probe", "MATCH (n) RETURN n", vec![])?
        .add_instance_builder(
            DrasiLib::builder()
                .with_id(if case == "failed" { "first" } else { "second" })
                .with_query_factory(factory)
                .with_query(query()?),
        )
        .with_telemetry(config);
    if case == "exclusive" {
        let logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
        let preparation = builder
            .with_telemetry_bindings(TelemetryBindings::new(
                TelemetrySettings::new(TelemetryProfile::Off),
                logging.clone(),
            ))
            .prepare();
        let cleanup = preparation.cleanup_handle();
        let error = preparation
            .finish()
            .await
            .err()
            .context("configured and borrowed policies were accepted")?;
        assert!(
            format!("{error:#}").contains("mutually exclusive"),
            "{error:#}"
        );
        assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
        assert_eq!(cleanup.status().pending_workers, 0);
        assert!(
            observed.lock().unwrap().is_empty(),
            "rejection constructed a factory"
        );
        assert!(!logging.is_closed(), "rejection closed borrowed logging");
        assert!(collector.received_requests().await.unwrap().is_empty());
        logging.shutdown(WAIT).await?;
        assert_global_untouched(&globals);
        return Ok(());
    }
    if case == "failed" {
        let preparation = builder.prepare();
        let cleanup = preparation.cleanup_handle();
        let error = preparation
            .finish()
            .await
            .err()
            .context("duplicate Core IDs were accepted")?;
        assert!(
            format!("{error:#}").contains("Duplicate Core instance ID"),
            "{error:#}"
        );
        assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
        assert_eq!(cleanup.status().pending_workers, 0);
        assert!(!cleanup.status().retry_required);
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(
            collector.received_requests().await.unwrap().is_empty(),
            "failed preparation exported"
        );
        assert_global_untouched(&globals);
        return Ok(());
    }
    if case == "owned" {
        let built = builder.build_with_owned_handles().await?;
        for core in built.servers.values() {
            timeout(WAIT, core.component_by_name("probe")?.wait_created()).await??;
        }
        capture(&collector).await?;
        built.shutdown().await?;
    } else {
        let server = builder.build().await?;
        let cleanup = server.cleanup_handle();
        timeout(WAIT, async {
            while observed.lock().unwrap().len() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(
            collector.received_requests().await.unwrap().is_empty(),
            "build exported before run"
        );
        if case == "never-run" {
            server.shutdown().await?;
            assert!(
                collector.received_requests().await.unwrap().is_empty(),
                "rollback exported"
            );
            drop(server);
            assert_global_untouched(&globals);
            return Ok(());
        }
        let running = tokio::spawn(server.run());
        capture(&collector).await?;
        cleanup.wait_until(Instant::now() + WAIT).await?;
        timeout(WAIT, running).await???;
    }
    assert_eq!(
        observed
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["first".into(), "second".into()])
    );
    assert_eq!(
        observed.lock().unwrap().len(),
        2,
        "a Core was constructed twice"
    );
    let requests = collector.received_requests().await.unwrap();
    let mut identities = BTreeSet::new();
    let mut resources = BTreeSet::new();
    for request in requests {
        assert_eq!(request.url.path(), "/custom/metrics");
        assert_eq!(request.headers["content-type"], "application/x-protobuf");
        let body = ExportMetricsServiceRequest::decode(request.body.as_slice())?;
        for resource in body.resource_metrics {
            let attributes = resource.resource.context("missing resource")?.attributes;
            assert!(attributes.iter().any(|a| a.key == "service.name"
                && a.value.as_ref().and_then(|v| v.value.as_ref())
                    == Some(&Value::StringValue("task4-public".into()))));
            let identity = attributes
                .into_iter()
                .find(|a| a.key == "service.instance.id")
                .context("missing service instance")?
                .value
                .context("missing identity value")?
                .value;
            resources.insert(format!("{identity:?}"));
            for scope in resource.scope_metrics {
                for metric in scope.metrics {
                    if metric.name == "drasi.plugin.task4.created" {
                        let Data::Sum(sum) = metric.data.context("counter data")? else {
                            anyhow::bail!("counter was not a sum")
                        };
                        for point in sum.data_points {
                            let id = point
                                .attributes
                                .iter()
                                .find(|a| a.key == "instance_id")
                                .context("missing instance")?;
                            if let Some(Value::StringValue(id)) =
                                id.value.as_ref().and_then(|v| v.value.as_ref())
                            {
                                identities.insert(id.clone());
                            }
                            assert!(point.attributes.iter().any(|a| a.key == "graph_id"));
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        identities,
        BTreeSet::from(["first".into(), "second".into()])
    );
    assert_eq!(resources.len(), 1);
    assert_global_untouched(&globals);
    Ok(())
}

fn assert_global_untouched(globals: &Mutex<Vec<String>>) {
    metrics::counter!("task4.global.sentinel").increment(1);
    assert_eq!(
        *globals.lock().unwrap(),
        ["task4.global.sentinel"],
        "Server installed or used global recorder"
    );
}

async fn capture(collector: &MockServer) -> Result<()> {
    timeout(WAIT, async {
        while collector.received_requests().await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}
