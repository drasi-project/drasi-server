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
    config::telemetry::TelemetryConfig, server::CleanupPhase, DrasiServer, DrasiServerBuilder,
    TelemetryBindings,
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
const SCRAPE_PATH: &str = "/observability/custom";

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
fn prometheus_shared_custom_path_serves_real_metrics_concurrently() -> Result<()> {
    child("prometheus-shared")
}

#[test]
fn prometheus_separate_listener_exposes_only_scrapes() -> Result<()> {
    child("prometheus-separate")
}

#[test]
fn prometheus_off_and_none_create_no_route_or_separate_listener() -> Result<()> {
    child("prometheus-off")?;
    child("prometheus-none")
}

#[test]
fn prometheus_empty_configuration_starts_healthy() -> Result<()> {
    child("prometheus-empty")
}

#[test]
fn prometheus_api_disabled_owned_result_serves_before_return_and_owns_shutdown() -> Result<()> {
    child("prometheus-owned")
}

#[test]
fn prometheus_shared_route_requires_enabled_api_before_factories() -> Result<()> {
    child("prometheus-no-api")
}

#[test]
fn prometheus_owned_result_requires_separate_listener_before_factories() -> Result<()> {
    child("prometheus-owned-shared")
}

#[test]
fn prometheus_management_bind_failure_rolls_back_both_listeners() -> Result<()> {
    child("prometheus-management-busy")
}

#[test]
fn prometheus_scrape_bind_failure_rolls_back_both_listeners() -> Result<()> {
    child("prometheus-scrape-busy")
}

#[test]
fn prometheus_owned_result_retains_cleanup_on_bind_failure() -> Result<()> {
    child("prometheus-owned-busy")
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
    namespace: &'static str,
    leaf: &'static str,
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
                    self.namespace,
                    PluginMetricDefinition {
                        leaf: self.leaf,
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
    if case.starts_with("prometheus-") {
        prometheus_scenario(case).await?;
        assert_global_untouched(&globals);
        return Ok(());
    }
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
        namespace: "task4",
        leaf: "created",
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

fn prometheus_config(profile: &str, backend: &str, port: Option<u16>) -> Result<TelemetryConfig> {
    Ok(serde_json::from_value(serde_json::json!({
        "profile": profile,
        "metrics": {
            "backend": backend,
            "prometheus": { "path": SCRAPE_PATH, "port": port }
        }
    }))?)
}

fn prometheus_builder(observed: Arc<Mutex<Vec<String>>>) -> Result<DrasiServerBuilder> {
    let factory = Arc::new(FirstFactory {
        inner: query_plugin::factory(),
        observed,
        namespace: "test",
        leaf: "count",
    });
    Ok(DrasiServerBuilder::new()
        .with_id("first")
        .with_query_factory(factory.clone())
        .with_simple_query("probe", "MATCH (n) RETURN n", vec![])?
        .add_instance_builder(
            DrasiLib::builder()
                .with_id("second")
                .with_query_factory(factory)
                .with_query(Query::cypher("probe").query("MATCH (n) RETURN n").build()?),
        )
        .disable_ui())
}

async fn reserved_port() -> Result<(tokio::net::TcpListener, u16)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()?)
}

async fn ready(client: &reqwest::Client, port: u16, path: &str) -> Result<reqwest::Response> {
    timeout(WAIT, async {
        loop {
            if let Ok(response) = client
                .get(format!("http://127.0.0.1:{port}{path}"))
                .send()
                .await
            {
                return response;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("listener did not become responsive")
}

async fn assert_scrape(
    client: &reqwest::Client,
    port: u16,
    expected_instances: &[&str],
) -> Result<()> {
    let response = client
        .get(format!("http://127.0.0.1:{port}{SCRAPE_PATH}"))
        .send()
        .await?;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    let text = response.text().await?;
    assert!(
        text.contains("# TYPE drasi_plugin_test_count counter"),
        "{text}"
    );
    let samples = text
        .lines()
        .filter(|line| line.starts_with("drasi_plugin_test_count{"))
        .collect::<Vec<_>>();
    assert_eq!(samples.len(), expected_instances.len(), "{text}");
    for instance in expected_instances {
        let label = format!("instance_id=\"{instance}\"");
        let sample = samples
            .iter()
            .find(|sample| sample.contains(&label))
            .with_context(|| format!("missing {label}: {text}"))?;
        assert!(sample.contains("graph_id=\""), "{sample}");
        assert_eq!(sample.rsplit_once(' ').context("sample value")?.1, "1");
    }
    Ok(())
}

async fn assert_closed(port: u16) -> Result<()> {
    assert!(
        timeout(WAIT, tokio::net::TcpStream::connect(("127.0.0.1", port)))
            .await?
            .is_err(),
        "listener {port} remained open"
    );
    let rebound = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    drop(rebound);
    Ok(())
}

async fn prometheus_scenario(case: &str) -> Result<()> {
    let client = client()?;
    let observed = Arc::new(Mutex::new(Vec::new()));
    if matches!(case, "prometheus-no-api" | "prometheus-owned-shared") {
        let builder = prometheus_builder(observed.clone())?.with_telemetry(prometheus_config(
            "basic",
            "prometheus",
            None,
        )?);
        let (cleanup, error) = if case == "prometheus-no-api" {
            let preparation = builder.prepare();
            let cleanup = preparation.cleanup_handle();
            let error = preparation
                .finish()
                .await
                .err()
                .context("shared scrape accepted with API disabled")?;
            (cleanup, error)
        } else {
            let preparation = builder.enable_api().prepare_with_owned_handles();
            let cleanup = preparation.cleanup_handle();
            let error = preparation
                .finish()
                .await
                .err()
                .context("owned result accepted an unservable shared route")?;
            (cleanup, error)
        };
        let error = format!("{error:#}");
        assert!(
            if case == "prometheus-no-api" {
                error.contains("management listener")
            } else {
                error.contains("separate listener")
            },
            "wrong compatibility error: {error}"
        );
        assert!(
            observed.lock().unwrap().is_empty(),
            "rejection ran factories"
        );
        assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
        assert_eq!(cleanup.status().pending_workers, 0);
        return Ok(());
    }

    let (management_reservation, management_port) = reserved_port().await?;
    let (scrape_reservation, scrape_port) = reserved_port().await?;
    let disabled = matches!(case, "prometheus-off" | "prometheus-none");
    let shared = case == "prometheus-shared";
    let config = prometheus_config(
        if case == "prometheus-off" {
            "off"
        } else {
            "basic"
        },
        if case == "prometheus-none" {
            "none"
        } else {
            "prometheus"
        },
        (!shared).then_some(scrape_port),
    )?;

    if matches!(case, "prometheus-owned" | "prometheus-owned-busy") {
        let _scrape_reservation = if case == "prometheus-owned" {
            drop(scrape_reservation);
            None
        } else {
            Some(scrape_reservation)
        };
        let preparation = prometheus_builder(observed.clone())?
            .with_telemetry(config)
            .prepare_with_owned_handles();
        let cleanup = preparation.cleanup_handle();
        if case == "prometheus-owned-busy" {
            let error = preparation
                .finish()
                .await
                .err()
                .context("owned result returned despite occupied scrape port")?;
            assert!(
                !format!("{error:#}").contains("cannot activate"),
                "test stopped at the capability gate: {error:#}"
            );
            assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
            assert_eq!(cleanup.status().pending_workers, 0);
        } else {
            let built = preparation.finish().await?;
            for core in built.servers.values() {
                timeout(WAIT, core.component_by_name("probe")?.wait_created()).await??;
            }
            // No readiness polling: the owned result must already serve.
            assert_scrape(&client, scrape_port, &["first", "second"]).await?;
            assert_eq!(
                client
                    .get(format!("http://127.0.0.1:{scrape_port}/health"))
                    .send()
                    .await?
                    .status(),
                reqwest::StatusCode::NOT_FOUND
            );
            built.shutdown().await?;
            assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
            assert_closed(scrape_port).await?;
        }
        return Ok(());
    }

    // Holding both reservations during construction catches premature serving
    // by build(); only run() may bind a normal Server's sockets.
    let directory = tempfile::Builder::new()
        .prefix("obs04-task5-")
        .tempdir_in(".")?;
    let server = if case == "prometheus-empty" {
        let path = directory.path().join("server.yaml");
        std::fs::write(
            &path,
            serde_yaml::to_string(&serde_json::json!({
                "host": "127.0.0.1",
                "persistConfig": false,
                "verifyPlugins": false,
                "autoInstallPlugins": false,
                "instances": [],
                "telemetry": config
            }))?,
        )?;
        DrasiServer::new(
            path,
            management_port,
            directory.path().join("plugins"),
            true,
            false,
        )
        .await?
    } else {
        let builder = if disabled {
            DrasiServerBuilder::new().with_id("disabled").disable_ui()
        } else {
            prometheus_builder(observed.clone())?
        };
        builder
            .with_host_port("127.0.0.1", management_port)
            .with_telemetry(config)
            .build()
            .await?
    };
    let cleanup = server.cleanup_handle();
    if case == "prometheus-management-busy" {
        drop(scrape_reservation);
        let result = timeout(WAIT, server.run()).await?;
        assert!(result.is_err(), "occupied management port was accepted");
        assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
        assert_eq!(cleanup.status().pending_workers, 0);
        assert!(!cleanup.status().retry_required);
        assert_closed(scrape_port).await?;
        drop(management_reservation);
        assert_closed(management_port).await?;
        return Ok(());
    }
    if case == "prometheus-scrape-busy" {
        drop(management_reservation);
        let result = timeout(WAIT, server.run()).await?;
        assert!(result.is_err(), "occupied scrape port was accepted");
        assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
        assert_eq!(cleanup.status().pending_workers, 0);
        assert!(!cleanup.status().retry_required);
        assert_closed(management_port).await?;
        drop(scrape_reservation);
        assert_closed(scrape_port).await?;
        return Ok(());
    }
    drop(management_reservation);
    drop(scrape_reservation);
    assert_closed(management_port).await?;
    assert_closed(scrape_port).await?;
    let running = tokio::spawn(server.run());
    let health = ready(&client, management_port, "/health").await?;
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    if disabled {
        assert_eq!(
            client
                .get(format!("http://127.0.0.1:{management_port}{SCRAPE_PATH}"))
                .send()
                .await?
                .status(),
            reqwest::StatusCode::NOT_FOUND
        );
        assert_closed(scrape_port).await?;
    } else {
        let port = if shared { management_port } else { scrape_port };
        assert_eq!(
            ready(&client, port, SCRAPE_PATH).await?.status(),
            reqwest::StatusCode::OK
        );
        if case == "prometheus-empty" {
            let instances: serde_json::Value = client
                .get(format!(
                    "http://127.0.0.1:{management_port}/api/v1/instances"
                ))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            assert_eq!(instances["data"], serde_json::json!([]));
        } else {
            timeout(WAIT, async {
                while observed.lock().unwrap().len() != 2 {
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            assert_scrape(&client, port, &["first", "second"]).await?;
            let expected = ["first", "second"];
            let reads = (0..4).map(|_| assert_scrape(&client, port, &expected));
            futures_util::future::try_join_all(reads).await?;
        }
        assert_eq!(
            client
                .get(format!("http://127.0.0.1:{port}/metrics"))
                .send()
                .await?
                .status(),
            reqwest::StatusCode::NOT_FOUND
        );
        if !shared {
            for path in ["/health", "/api/versions", "/api/v1/instances", "/ui/"] {
                assert_eq!(
                    client
                        .get(format!("http://127.0.0.1:{scrape_port}{path}"))
                        .send()
                        .await?
                        .status(),
                    reqwest::StatusCode::NOT_FOUND,
                    "separate listener exposed {path}"
                );
            }
            assert_eq!(
                client
                    .get(format!("http://127.0.0.1:{management_port}{SCRAPE_PATH}"))
                    .send()
                    .await?
                    .status(),
                reqwest::StatusCode::NOT_FOUND
            );
        }
    }
    cleanup.wait_until(Instant::now() + WAIT).await?;
    timeout(WAIT, running).await???;
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert_eq!(cleanup.status().pending_workers, 0);
    assert_closed(management_port).await?;
    assert_closed(scrape_port).await?;
    Ok(())
}
