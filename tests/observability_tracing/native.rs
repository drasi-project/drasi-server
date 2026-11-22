// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Opt-in real cdylib qualification. Set `DRASI_OBS05_FIXTURES_DIR` to the
//! isolated Core target directory containing matched `debug/` libraries and
//! `debug/examples/` SDK fixtures. Libraries are copied into an owned directory
//! and loaded through Server's verified family loader.

use super::*;
use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_computation_plugin_sdk::{abi, transport, wire};
use drasi_host_sdk::computation::{NativeComponentProxy, NativePlugin};
use drasi_lib::{computation::v1::*, observability::*};
use opentelemetry_proto::tonic::{
    collector::metrics::v1::ExportMetricsServiceRequest,
    metrics::v1::{metric::Data, number_data_point::Value as Number},
    trace::v1::Span,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const PROBE: &str = "45b5d856-324e-4d9d-9e97-28db6168f7cf";
const KIND: &str = "test-telemetry";
const FRAMES: &str = "drasi.plugin.test-telemetry.frames_received";

fn library_name(stem: &str) -> String {
    format!(
        "{}{stem}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

fn fixtures() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("DRASI_OBS05_FIXTURES_DIR").context(
            "set DRASI_OBS05_FIXTURES_DIR to the matched, isolated Core target directory",
        )?,
    ))
}

fn example(name: &str) -> Result<PathBuf> {
    Ok(fixtures()?.join("debug/examples").join(library_name(name)))
}

fn library(name: &str) -> Result<PathBuf> {
    Ok(fixtures()?.join("debug").join(library_name(name)))
}

/// Copy a matched artifact under a Server-recognized plugin name.
fn stage(source: &Path, directory: &Path, stem: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        source.is_file(),
        "missing matched fixture {}; build it as described in OBS-05 Task 6",
        source.display()
    );
    let target = directory.join(library_name(stem));
    std::fs::copy(source, &target)?;
    Ok(target)
}

/// Server family loading, including base-ABI metadata validation.
fn load(directory: &Path, expected: usize) -> Result<(drasi_server::PluginRegistry, usize)> {
    let mut registry = drasi_server::PluginRegistry::new();
    let stats = drasi_server::dynamic_loading::load_plugins(directory, &mut registry, None, None)?;
    anyhow::ensure!(stats.plugins_failed == 0, "{:?}", stats.failures);
    anyhow::ensure!(
        stats.plugins_loaded == expected,
        "loaded {}",
        stats.plugins_loaded
    );
    Ok((registry, stats.source_descriptors))
}

fn native(registry: &drasi_server::PluginRegistry) -> Result<Arc<NativePlugin>> {
    let mut plugins = registry.computation_plugins();
    let plugin = plugins
        .next()
        .context("no native plugin registered")?
        .clone();
    anyhow::ensure!(plugins.next().is_none(), "expected one native plugin");
    Ok(plugin)
}

/// Fixture-exported callbacks, opened from the same copied path Server loaded.
struct Callbacks(libloading::Library);

impl Callbacks {
    fn open(path: &Path) -> Result<Self> {
        Ok(Self(unsafe { libloading::Library::new(path)? }))
    }

    fn update(&self, instance: &str, generation: u64, absolute: bool, value: u64) -> Result<()> {
        let bytes = wire::encode(&(instance, PROBE, generation, absolute, value))?;
        unsafe {
            let call = self
                .0
                .get::<unsafe extern "C" fn(abi::BorrowedBytes) -> abi::Status>(
                    b"drasi_telemetry_fixture_update\0",
                )?;
            transport::take_status(call(abi::BorrowedBytes::new(&bytes)))?;
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        unsafe {
            let call = self
                .0
                .get::<unsafe extern "C" fn() -> abi::Status>(b"drasi_telemetry_fixture_clear\0")?;
            transport::take_status(call())?;
        }
        Ok(())
    }
}

/// Host-assigned scope from the actual Server-built Core.
fn binding(
    core: &drasi_lib::DrasiLib,
    generation: u64,
    role: ComponentRole,
) -> Result<PluginTelemetry> {
    let scope = core
        .telemetry()
        .scope()
        .context("configured Core has no telemetry scope")?
        .clone()
        .component(ComponentId::try_new(PROBE)?, role, role.into());
    Ok(core.telemetry().plugin_binding(
        scope,
        generation,
        KIND,
        core.component_logging().cloned(),
    )?)
}

fn create(
    factory: &drasi_host_sdk::computation::NativeFactory,
    binding: &PluginTelemetry,
    configuration: serde_json::Value,
) -> Result<NativeComponentProxy> {
    ExecutionScope::new(Default::default())
        .with_component(binding.config().scope.clone(), binding.config().generation)
        .with_plugin_telemetry(Some(binding.clone()))
        .in_scope(|| factory.create_component(ComponentId::try_new(PROBE)?, configuration))
}

async fn run(component: &mut NativeComponentProxy, parent: ExecutionContext) -> Result<()> {
    ExecutionScope::new(parent).scope(component.run()).await
}

/// Lifecycle hooks for driving a constructed generation directly; each call
/// gets a fresh operation ID, as the graph would issue.
fn operation(hook: &str, generation: u64) -> OperationScope {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    OperationScope::new(
        format!("obs05-{hook}-{id}"),
        PROBE,
        generation,
        None,
        CancellationToken::new(),
    )
}

fn initialize_ctx(generation: u64) -> InitializeContext {
    InitializeContext::new(operation("initialize", generation))
}

fn start_ctx(generation: u64) -> StartContext {
    StartContext::new(operation("start", generation))
}

fn stop_ctx(generation: u64) -> StopContext {
    StopContext::new(operation("stop", generation), StopMode::Drain)
}

/// Retirement that releases the object while preserving owned state.
fn dispose_ctx(generation: u64) -> Result<DisposeContext> {
    Ok(DisposeContext::new(
        operation("dispose", generation),
        DisposeReason::Shutdown,
        StateIntent::Preserve,
        1,
    )?)
}

fn parent(id: u8, sampled: bool) -> Result<ExecutionContext> {
    Ok(ExecutionContext::new(TraceContextV1::new(
        [id; 16],
        [id; 8],
        u8::from(sampled),
        None,
    )?))
}

async fn server(telemetry: serde_json::Value) -> Result<drasi_server::DrasiServerWithOwnedHandles> {
    let config: TelemetryConfig = serde_json::from_value(telemetry)?;
    Ok(DrasiServerBuilder::new()
        .with_id("primary")
        .disable_ui()
        .add_instance_builder(source("alpha")?)
        .add_instance_builder(source("beta")?)
        .with_telemetry(config)
        .build_with_owned_handles()
        .await?)
}

async fn receiver() -> MockServer {
    let receiver = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&receiver)
        .await;
    receiver
}

fn tracing(receiver: &MockServer, ratio: f64) -> serde_json::Value {
    json!({
        "endpoint": format!("{}/custom/traces", receiver.uri()),
        "protocol": "http/protobuf",
        "serviceName": "obs05-native",
        "sampleRatio": ratio,
    })
}

async fn spans(receiver: &MockServer) -> Result<Vec<Span>> {
    let mut spans = Vec::new();
    for request in receiver.received_requests().await.unwrap_or_default() {
        if request.url.path() == "/custom/traces" {
            let body = ExportTraceServiceRequest::decode(request.body.as_slice())?;
            spans.extend(
                body.resource_spans
                    .into_iter()
                    .flat_map(|resource| resource.scope_spans)
                    .flat_map(|scope| scope.spans),
            );
        }
    }
    Ok(spans)
}

fn free_port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

async fn scrape(port: u16, instance: &str) -> Result<f64> {
    let text = reqwest::get(format!("http://127.0.0.1:{port}/metrics"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    let label = format!("instance_id=\"{instance}\"");
    text.lines()
        .find(|line| {
            line.starts_with("drasi_plugin_test_telemetry_frames_received{")
                && line.contains(&label)
        })
        .context("plugin frame counter is not exposed")?
        .rsplit(' ')
        .next()
        .context("sample value")?
        .parse()
        .map_err(Into::into)
}

async fn otlp_frames(receiver: &MockServer, instance: &str) -> Result<Option<i64>> {
    let mut latest = None;
    for request in receiver.received_requests().await.unwrap_or_default() {
        if request.url.path() != "/custom/metrics" {
            continue;
        }
        let body = ExportMetricsServiceRequest::decode(request.body.as_slice())?;
        for metric in body
            .resource_metrics
            .iter()
            .flat_map(|resource| &resource.scope_metrics)
            .flat_map(|scope| &scope.metrics)
            .filter(|metric| metric.name == FRAMES)
        {
            if let Some(Data::Sum(sum)) = &metric.data {
                for point in &sum.data_points {
                    if attribute(&point.attributes, "instance_id").as_deref() == Some(instance) {
                        if let Some(Number::AsInt(value)) = point.value {
                            latest = Some(value);
                        }
                    }
                }
            }
        }
    }
    Ok(latest)
}

async fn decoded_logs(core: &drasi_lib::DrasiLib) -> Result<usize> {
    let logging = core.component_logging().context("component logging")?;
    logging.flush().await?;
    let key = ComponentLogKey::new(
        core.get_config().id.clone(),
        ComponentLogType::Service,
        ComponentId::try_new(PROBE)?,
    );
    Ok(logging
        .registry()
        .get_history_by_key(&key)
        .await
        .iter()
        .filter(|message| message.message == "decoded")
        .count())
}

pub(super) async fn scenario(case: &str) -> Result<()> {
    match case {
        "native-export" => export_once().await,
        "native-retire" => retirement().await,
        "native-fallback" => fallback().await,
        "native-unsampled-prometheus" => unsampled(false).await,
        "native-unsampled-otlp" => unsampled(true).await,
        _ => anyhow::bail!("unknown native scenario {case}"),
    }
}

// Same component UUID in two instances; one instance stops, the other exports.
async fn export_once() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = stage(
        &example("telemetry_fixture")?,
        directory.path(),
        "drasi_obs05_telemetry",
    )?;
    let (registry, _) = load(directory.path(), 1)?;
    let plugin = native(&registry)?;
    let callbacks = Callbacks::open(&path)?;
    callbacks.clear()?;
    let receiver = receiver().await;
    let built = server(json!({"profile": "basic", "tracing": tracing(&receiver, 1.0)})).await?;
    let factory = plugin.factories()[0].clone();
    let mut components = Vec::new();
    let mut bindings = Vec::new();
    for (seed, instance) in [(1u8, "alpha"), (2, "beta")] {
        let binding = binding(&built.servers[instance], 1, ComponentRole::Service)?;
        let mut component = create(&factory, &binding, json!({"absolute": 1}))?;
        component.initialize(&initialize_ctx(1)).await?;
        component.start(&start_ctx(1)).await?;
        run(&mut component, parent(seed, true)?).await?;
        components.push(component);
        bindings.push(binding);
    }
    let mut alpha = components.remove(0);
    alpha.stop(&stop_ctx(1)).await?;
    alpha.dispose(&dispose_ctx(1)?).await?;
    drop(alpha);
    built.servers["alpha"].stop().await?;
    run(&mut components[0], parent(3, true)?).await?;
    assert_eq!(decoded_logs(&built.servers["alpha"]).await?, 1);
    assert_eq!(decoded_logs(&built.servers["beta"]).await?, 2);
    let mut beta = components.remove(0);
    beta.stop(&stop_ctx(1)).await?;
    beta.dispose(&dispose_ctx(1)?).await?;
    drop((beta, bindings));
    built.shutdown().await?;
    callbacks.clear()?;

    let plugin_spans: Vec<_> = spans(&receiver)
        .await?
        .into_iter()
        .filter(|span| span.name == "plugin.decode")
        .collect();
    assert_eq!(plugin_spans.len(), 3, "each successful run exports once");
    let mut identities = std::collections::BTreeSet::new();
    for (seed, instance) in [(1u8, "alpha"), (2, "beta"), (3, "beta")] {
        let matching: Vec<_> = plugin_spans
            .iter()
            .filter(|span| span.trace_id == [seed; 16])
            .collect();
        assert_eq!(matching.len(), 1, "trace {seed}");
        let span = matching[0];
        assert_eq!(span.parent_span_id, [seed; 8]);
        assert!(identities.insert(span.span_id.clone()));
        assert_eq!(
            attribute(&span.attributes, "instance_id").as_deref(),
            Some(instance)
        );
        assert_eq!(
            attribute(&span.attributes, "component_id").as_deref(),
            Some(PROBE)
        );
        assert_eq!(
            attribute(&span.attributes, "plugin_kind").as_deref(),
            Some(KIND)
        );
    }
    Ok(())
}

// 100 then replacement 10 yields 110; stale and failed constructions do not write.
async fn retirement() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = stage(
        &example("telemetry_fixture")?,
        directory.path(),
        "drasi_obs05_telemetry",
    )?;
    let (registry, _) = load(directory.path(), 1)?;
    let plugin = native(&registry)?;
    let callbacks = Callbacks::open(&path)?;
    callbacks.clear()?;
    let receiver = receiver().await;
    let port = free_port()?;
    let built = server(json!({
        "profile": "basic",
        "tracing": tracing(&receiver, 1.0),
        "metrics": {"backend": "prometheus", "prometheus": {"path": "/metrics", "port": port}}
    }))
    .await?;
    let core = &built.servers["alpha"];
    let factory = plugin.factories()[0].clone();
    let old = binding(core, 1, ComponentRole::Service)?;
    let mut first = create(&factory, &old, json!({"absolute": 100}))?;
    first.initialize(&initialize_ctx(1)).await?;
    first.start(&start_ctx(1)).await?;
    run(&mut first, Default::default()).await?;
    assert_eq!(scrape(port, "alpha").await?, 100.0);
    first.stop(&stop_ctx(1)).await?;
    let new = binding(core, 2, ComponentRole::Service)?;
    let mut second = create(&factory, &new, json!({"absolute": 10}))?;
    second.initialize(&initialize_ctx(2)).await?;
    second.start(&start_ctx(2)).await?;
    run(&mut second, Default::default()).await?;
    assert_eq!(scrape(port, "alpha").await?, 110.0);
    callbacks.update("alpha", 1, true, 500)?;
    assert_eq!(scrape(port, "alpha").await?, 110.0);
    assert_eq!(old.diagnostics().snapshot().metric, 1);
    second.stop(&stop_ctx(2)).await?;
    let failed = binding(core, 3, ComponentRole::Service)?;
    assert!(create(&factory, &failed, json!({"absolute": 7, "fail": true})).is_err());
    assert_eq!(scrape(port, "alpha").await?, 110.0);
    first.dispose(&dispose_ctx(1)?).await?;
    second.dispose(&dispose_ctx(2)?).await?;
    drop((first, second, old, new, failed));
    built.shutdown().await?;
    callbacks.clear()?;
    Ok(())
}

// Base-only native plugins keep host spans; Host SDK reports the missing extension.
async fn fallback() -> Result<()> {
    let directory = tempfile::tempdir()?;
    stage(
        &example("execution_context_base_fixture")?,
        directory.path(),
        "drasi_obs05_context_base",
    )?;
    stage(
        &library("drasi_source_http")?,
        directory.path(),
        "drasi_source_http",
    )?;
    let (registry, ordinary) = load(directory.path(), 2)?;
    assert!(ordinary >= 1, "ordinary source family was not registered");
    let plugin = native(&registry)?;
    for factory in plugin.factories() {
        assert_eq!(factory.telemetry_capabilities(), 0);
        assert!(!factory.supports_execution_context());
    }
    let sink = plugin
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == "test/context-sink")
        .context("base sink factory")?
        .clone();
    let receiver = receiver().await;
    let built = server(json!({"profile": "basic", "tracing": tracing(&receiver, 1.0)})).await?;
    let binding = binding(&built.servers["alpha"], 1, ComponentRole::Sink)?;
    let mut component = create(&sink, &binding, json!({"trace": true}))?;
    component.initialize(&initialize_ctx(1)).await?;
    component.start(&start_ctx(1)).await?;
    component.stop(&stop_ctx(1)).await?;
    component.dispose(&dispose_ctx(1)?).await?;
    drop((component, binding));
    built.shutdown().await?;
    let exported = spans(&receiver).await?;
    assert!(
        exported.iter().any(|span| span.name == "component.start"
            && attribute(&span.attributes, "instance_id").as_deref() == Some("alpha")),
        "available host spans must remain"
    );
    assert!(
        !exported
            .iter()
            .any(|span| span.name.starts_with("fixture.")),
        "no plugin-internal span may be invented"
    );
    Ok(())
}

// Ratio 0 drops plugin spans, while logs and raw counters still advance.
async fn unsampled(otlp: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = stage(
        &example("telemetry_fixture")?,
        directory.path(),
        "drasi_obs05_telemetry",
    )?;
    let (registry, _) = load(directory.path(), 1)?;
    let plugin = native(&registry)?;
    let callbacks = Callbacks::open(&path)?;
    callbacks.clear()?;
    let receiver = receiver().await;
    let port = free_port()?;
    let metrics = if otlp {
        json!({"backend": "otlp", "otlp": {
            "endpoint": format!("{}/custom/metrics", receiver.uri()),
            "protocol": "http/protobuf", "exportInterval": 1, "exportTimeout": 1
        }})
    } else {
        json!({"backend": "prometheus", "prometheus": {"path": "/metrics", "port": port}})
    };
    let built = server(json!({
        "profile": "basic", "tracing": tracing(&receiver, 0.0), "metrics": metrics
    }))
    .await?;
    let core = &built.servers["alpha"];
    let binding = binding(core, 1, ComponentRole::Service)?;
    let factory = plugin.factories()[0].clone();
    let mut component = create(&factory, &binding, json!({"absolute": 5}))?;
    component.initialize(&initialize_ctx(1)).await?;
    component.start(&start_ctx(1)).await?;
    run(&mut component, Default::default()).await?;
    callbacks.update("alpha", 1, false, 2)?;
    if otlp {
        timeout(WAIT, async {
            while otlp_frames(&receiver, "alpha").await? != Some(7) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
    } else {
        assert_eq!(scrape(port, "alpha").await?, 7.0);
    }
    assert_eq!(decoded_logs(core).await?, 1);
    component.stop(&stop_ctx(1)).await?;
    component.dispose(&dispose_ctx(1)?).await?;
    drop((component, binding));
    built.shutdown().await?;
    callbacks.clear()?;
    assert!(
        !spans(&receiver)
            .await?
            .iter()
            .any(|span| span.name == "plugin.decode"),
        "an unsampled root must not export plugin spans"
    );
    Ok(())
}

// -- Direct native ingress: HTTP headers and gRPC metadata survive polling. --

const SOURCE_ID: &str = "e07b5b54-840c-4c48-bac0-0bbdf637d306";
const SINK_ID: &str = "f027d30c-f549-4d41-a5cf-bd7f0bc1e606";

struct Collector {
    descriptor: ComponentDescriptor,
    contexts: Arc<Mutex<Vec<ExecutionContext>>>,
}

#[async_trait]
impl ComputationComponent for Collector {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn description(&self) -> ComponentDescription {
        ComponentDescription::undeclared()
    }
    drasi_lib::passive_component_lifecycle!();
}

#[async_trait]
impl EnvelopeSink for Collector {
    fn completion(&self) -> SinkCompletion {
        SinkCompletion::Handled
    }
    async fn handle(&mut self, input: InputEnvelope) -> Result<()> {
        self.contexts
            .lock()
            .map_err(|_| anyhow::anyhow!("collector poisoned"))?
            .push(input.execution_context);
        Ok(())
    }
}

mod grpc_wire {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ElementReference {
        #[prost(string, tag = "1")]
        pub source_id: String,
        #[prost(string, tag = "2")]
        pub element_id: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ElementMetadata {
        #[prost(message, optional, tag = "1")]
        pub reference: Option<ElementReference>,
        #[prost(string, repeated, tag = "2")]
        pub labels: Vec<String>,
        #[prost(uint64, tag = "3")]
        pub effective_from: u64,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Node {
        #[prost(message, optional, tag = "1")]
        pub metadata: Option<ElementMetadata>,
    }
    /// `Element.node` is oneof field 1; this encoding is wire-identical.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Element {
        #[prost(message, optional, tag = "1")]
        pub node: Option<Node>,
    }
    /// `SourceChange.element` is oneof field 2; type 1 is insert.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SourceChange {
        #[prost(int32, tag = "1")]
        pub r#type: i32,
        #[prost(message, optional, tag = "2")]
        pub element: Option<Element>,
        #[prost(string, tag = "5")]
        pub source_id: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubmitEventRequest {
        #[prost(message, optional, tag = "1")]
        pub event: Option<SourceChange>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubmitEventResponse {
        #[prost(bool, tag = "1")]
        pub success: bool,
    }
}

async fn send(
    grpc: bool,
    port: u16,
    element: &str,
    parent: Option<&str>,
    state: Option<&str>,
) -> Result<()> {
    let mut request = if grpc {
        let message = grpc_wire::SubmitEventRequest {
            event: Some(grpc_wire::SourceChange {
                r#type: 1,
                element: Some(grpc_wire::Element {
                    node: Some(grpc_wire::Node {
                        metadata: Some(grpc_wire::ElementMetadata {
                            reference: Some(grpc_wire::ElementReference {
                                source_id: SOURCE_ID.into(),
                                element_id: element.into(),
                            }),
                            labels: vec!["Room".into()],
                            effective_from: 1_000_000,
                        }),
                    }),
                }),
                source_id: SOURCE_ID.into(),
            }),
        }
        .encode_to_vec();
        let mut body = vec![0u8];
        body.extend((message.len() as u32).to_be_bytes());
        body.extend(message);
        reqwest::Client::builder()
            .http2_prior_knowledge()
            .build()?
            .post(format!(
                "http://127.0.0.1:{port}/drasi.v1.SourceService/SubmitEvent"
            ))
            .header("content-type", "application/grpc")
            .header("te", "trailers")
            .body(body)
    } else {
        reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{port}/sources/{SOURCE_ID}/events"
            ))
            .json(&json!({"operation": "insert", "element": {
                "type": "node", "id": element, "labels": ["Room"], "properties": {"value": 1}
            }}))
    };
    if let Some(parent) = parent {
        request = request.header("traceparent", parent);
    }
    if let Some(state) = state {
        request = request.header("tracestate", state);
    }
    let response = request.send().await?.error_for_status()?;
    let body = response.bytes().await?;
    if grpc {
        anyhow::ensure!(body.len() > 5, "gRPC submit returned no message");
        let reply = grpc_wire::SubmitEventResponse::decode(&body[5..])?;
        anyhow::ensure!(reply.success, "gRPC submit was rejected");
    }
    Ok(())
}

async fn ingress(grpc: bool, instance: &str) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let stem = if grpc {
        "drasi_computation_source_grpc"
    } else {
        "drasi_computation_source_http"
    };
    stage(&library(stem)?, directory.path(), stem)?;
    let (registry, _) = load(directory.path(), 1)?;
    let factory = native(&registry)?.factories()[0].clone();
    let port = free_port()?;
    let contexts = Arc::new(Mutex::new(Vec::new()));
    let endpoint = |component: &str, port: &str| -> Result<Endpoint> {
        Ok(Endpoint::new(
            ComponentId::try_new(component)?,
            PortId::try_new(port)?,
        ))
    };
    let mut graph = ComputationGraph::builder(format!("ingress-{instance}"))
        .with_telemetry(
            TelemetryOptions::new(
                TelemetrySettings::new(TelemetryProfile::Basic)
                    .with_metrics(false)
                    .require_distributed_tracing(),
            )
            .with_instance_id(instance.to_owned())
            .with_context_adapter(Arc::new(OpenTelemetryContextAdapter)),
        )
        .component(
            ComponentName::try_new("source")?,
            factory.specification(
                ComponentId::try_new(SOURCE_ID)?,
                json!({
                    "stream": "source/out",
                    "host": "127.0.0.1", "port": port,
                }),
            )?,
            factory,
        )
        .sink(
            ComponentName::try_new("collected")?,
            Box::new(Collector {
                descriptor: ComponentDescriptor::try_new(
                    ComponentId::try_new(SINK_ID)?,
                    vec![PortDescriptor::new(
                        PortId::try_new("in")?,
                        PortDirection::Input,
                        GraphChangeCodec::schema().descriptor().clone(),
                        PipeRequirements::default(),
                    )],
                )?,
                contexts: contexts.clone(),
            }),
        )
        .bind_stream(
            endpoint(SOURCE_ID, "out")?,
            StreamId::try_new("source/out")?,
        )
        .connect(
            EdgeDefinition::new(endpoint(SOURCE_ID, "out")?, endpoint(SINK_ID, "in")?),
            Box::new(BoundedPipeConfig { capacity: 1 }),
        )
        .build()?;
    // An endless network source never completes the graph run; drive it
    // alongside the probes and cancel it through its control handle.
    let run = graph.start()?;
    let control = run.control();
    let probes = async {
        let result = probe(grpc, port, instance, &contexts).await;
        control.cancel();
        result
    };
    let (_, result) = tokio::join!(run, probes);
    result.with_context(|| format!("observed graph: {:?}", graph.observed()))?;
    graph.dispose().await?;
    Ok(())
}

async fn probe(
    grpc: bool,
    port: u16,
    instance: &str,
    contexts: &Mutex<Vec<ExecutionContext>>,
) -> Result<()> {
    timeout(WAIT, async {
        while tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("native source listener did not open")?;
    let sampled = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let unsampled = "00-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-bbbbbbbbbbbbbbbb-00";
    for (index, (parent, state)) in [
        (Some(sampled), Some("vendor=value")),
        (Some(unsampled), None),
        (Some("malformed"), None),
        (None, None),
    ]
    .into_iter()
    .enumerate()
    {
        send(grpc, port, &format!("row-{index}"), parent, state)
            .await
            .with_context(|| format!("{instance} send {index}"))?;
        timeout(WAIT, async {
            while contexts.lock().map(|values| values.len()).unwrap_or(0) <= index {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .with_context(|| format!("{instance} sink did not receive input {index}"))?;
        let received = contexts
            .lock()
            .map_err(|_| anyhow::anyhow!("collector poisoned"))?[index]
            .clone();
        assert_eq!(
            received,
            ExecutionContext::from_w3c_headers(parent.map(str::as_bytes), state.map(str::as_bytes)),
            "{instance} case {index}"
        );
    }
    Ok(())
}

pub(super) fn native_ingress() -> Result<()> {
    for flavor in ["current_thread", "multi_thread"] {
        let mut runtime = if flavor == "current_thread" {
            tokio::runtime::Builder::new_current_thread()
        } else {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.worker_threads(2);
            builder
        };
        runtime.enable_all().build()?.block_on(async {
            // Concurrent instances must not exchange contexts.
            let (http_id, grpc_id) = (format!("http-{flavor}"), format!("grpc-{flavor}"));
            let (http, grpc) = tokio::join!(ingress(false, &http_id), ingress(true, &grpc_id));
            http.context("native HTTP ingress")
                .and(grpc.context("native gRPC ingress"))
        })?;
    }
    Ok(())
}
