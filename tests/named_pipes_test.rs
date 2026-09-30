// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_host_sdk::computation::NativePlugin;
use drasi_lib::{computation::v1::*, DrasiLib};
use drasi_server::{
    api::{shared::handlers::clone_instance, v1::routes::build_v1_router},
    config::DrasiServerConfig,
    instance_registry::InstanceRegistry,
    persistence::ConfigPersistence,
};
use drasi_server::{
    computation::{
        configuration_from_snapshot, register_components, validate_definition, ComputationConfig,
    },
    plugin_registry::PluginRegistry,
};
use indexmap::IndexMap;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::sync::RwLock;
use tower::ServiceExt;

fn plugin_path() -> PathBuf {
    std::env::var_os("DRASI_NATIVE_STANDARD_PLUGIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/debug")
                .join(format!(
                    "{}drasi_computation_standard{}",
                    std::env::consts::DLL_PREFIX,
                    std::env::consts::DLL_SUFFIX
                ))
        })
}

fn plugin() -> Arc<NativePlugin> {
    static PLUGIN: OnceLock<Arc<NativePlugin>> = OnceLock::new();
    PLUGIN
        .get_or_init(|| {
            drasi_host_sdk::computation::load(plugin_path())
                .expect("build the native standard plugin; see tests/README.md")
        })
        .clone()
}

fn registry() -> Result<PluginRegistry> {
    let mut registry = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut registry);
    registry.register_computation_plugin(plugin())?;
    Ok(registry)
}

fn id(value: &str) -> ComponentId {
    ComponentId::try_new(value).unwrap()
}

fn specification(factory: &str, name: &str, config: Value) -> Result<ComponentSpecification> {
    plugin()
        .factories()
        .iter()
        .find(|value| value.metadata().implementation.name.as_ref() == factory)
        .context("fixture factory")?
        .specification(id(name), config)
}

fn input(output: &Path, pipe: Value, multicast: bool) -> Result<Value> {
    let source = specification(
        "drasi.standard/volatile-counter",
        "producer",
        json!({"stream":"events","count":4,"start":2,"step":3}),
    )?;
    let sink = specification("drasi.standard/capture", "consumer", json!({"path":output}))?;
    let mut config = json!({
        "pipes":{"events":pipe},
        "components":{
            "producer":{"factory":source,"ports":{"out":"events"},"streams":{"out":"events"},"lifecycle":{"auto_start":false}},
            "consumer":{"factory":sink,"ports":{"in":"events"},"lifecycle":{"auto_start":false}}
        }
    });
    if multicast {
        config["components"]["second"] = json!({
            "factory":specification("drasi.standard/capture", "second", json!({"path":output.with_extension("second.jsonl")}))?,
            "ports":{"in":{"pipe":"events","subscriber":"second-cursor","start":"Earliest"}},
            "lifecycle":{"auto_start":false}
        });
    }
    Ok(config)
}

async fn wait_output(path: &Path) -> Result<Vec<Value>> {
    let rows = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match tokio::fs::read_to_string(path).await {
                Ok(text) => {
                    let rows = text
                        .lines()
                        .map(serde_json::from_str)
                        .collect::<std::result::Result<Vec<Value>, _>>();
                    if let Ok(rows) = rows {
                        if rows.len() == 4 {
                            break Ok::<_, anyhow::Error>(rows);
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => break Err(error.into()),
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    let mut codec = EnvelopeCodec::new(std::num::NonZeroUsize::new(1024 * 1024).unwrap());
    codec.register_schema(GraphChangeCodec::schema())?;
    for (index, row) in rows.iter().enumerate() {
        let envelope = codec.decode(&serde_json::to_vec(row)?)?;
        assert_eq!(envelope.system().sequence(), index as u64 + 1);
        let changes = GraphChangeCodec::decode_changes(&envelope)?;
        let [drasi_core::models::SourceChange::Insert {
            element: drasi_core::models::Element::Node { properties, .. },
        }] = changes.as_slice()
        else {
            anyhow::bail!("expected one counter insert");
        };
        assert_eq!(
            properties.get("value"),
            Some(&drasi_core::models::ElementValue::Integer(
                2 + index as i64 * 3
            ))
        );
    }
    Ok(rows)
}

async fn start(core: &DrasiLib, config: &ComputationConfig) -> Result<()> {
    core.start().await?;
    let control = core.computation_control()?;
    let report = control
        .start_requested(
            control.desired_snapshot().revision,
            GraphSelection::Exact(
                config
                    .definition
                    .components
                    .iter()
                    .map(|component| component.descriptor.id().clone())
                    .collect(),
            ),
        )
        .await?;
    anyhow::ensure!(report.summary == OperationSummary::Completed, "{report:?}");
    Ok(())
}

async fn batch(
    config: &ComputationConfig,
    core: &DrasiLib,
    registry: &PluginRegistry,
) -> Result<ComponentBatch> {
    drasi_server::computation::build_components(
        config,
        core,
        registry.computation_factory_registry()?,
        registry.transactional_transformer_registry(core.middleware_registry())?,
    )
    .await
}

#[tokio::test]
async fn named_transport_delivery_and_snapshot_roundtrip() -> Result<()> {
    let registry = registry()?;
    for (pipe, multicast) in [
        (json!({"type":"bounded","capacity":8}), false),
        (
            json!({"type":"broadcast","capacity":8,"lagPolicy":"Report"}),
            false,
        ),
        (json!({"type":"qos","capacity":8}), true),
    ] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("output.jsonl");
        let config: ComputationConfig = serde_json::from_value(input(&path, pipe, multicast)?)?;
        let from_json: ComputationConfig = serde_json::from_str(&serde_json::to_string(&config)?)?;
        let from_yaml: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
        assert_eq!(from_json.definition, config.definition);
        assert_eq!(from_yaml.definition, config.definition);
        let core = DrasiLib::builder()
            .with_id("named-delivery")
            .build()
            .await?;
        let report = register_components(&config, &core, &registry).await?;
        assert!(
            report.committed && report.summary == OperationSummary::Completed,
            "{report:?}"
        );
        assert!(!path.exists());
        let snapshot =
            configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?
                .context("native configuration")?;
        let wire = serde_json::to_value(&snapshot)?;
        assert!(wire.get("definition").is_none());
        assert_eq!(wire["components"]["producer"]["ports"]["out"], "events");
        assert_eq!(
            wire["components"]["consumer"]["ports"]["in"]["pipe"],
            "events"
        );
        let restored: ComputationConfig = serde_json::from_value(wire)?;
        assert_eq!(snapshot.definition, restored.definition);
        start(&core, &config).await?;
        let output = wait_output(&path).await?;
        assert_eq!(output.len(), 4);
        if multicast {
            assert_eq!(
                wait_output(&path.with_extension("second.jsonl")).await?,
                output
            );
        }
        core.shutdown().await?;
    }
    Ok(())
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    payload: &str,
    media_type: &str,
) -> Result<(StatusCode, Value)> {
    let response = tokio::time::timeout(
        Duration::from_secs(20),
        app.clone().oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", media_type)
                .body(Body::from(payload.to_string()))?,
        ),
    )
    .await??;
    let status = response.status();
    Ok((
        status,
        serde_json::from_slice(&to_bytes(response.into_body(), 8 * 1024 * 1024).await?)?,
    ))
}

#[tokio::test]
async fn named_api_save_clone_reload_delete_and_instance_scope() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("output.jsonl");
    let path = directory.path().join("server.yaml");
    let first = Arc::new(DrasiLib::builder().with_id("first").build().await?);
    let second = Arc::new(DrasiLib::builder().with_id("second").build().await?);
    first
        .add_query(
            drasi_lib::Query::cypher("ordinary")
                .query("MATCH (n) RETURN n")
                .auto_start(false)
                .build(),
        )
        .await?;
    let instances = InstanceRegistry::new();
    instances
        .add("first".into(), first.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    instances
        .add("second".into(), second.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let plugins = Arc::new(RwLock::new(registry()?));
    let original: DrasiServerConfig =
        serde_json::from_value(json!({"instances":[{"id":"first"},{"id":"second"}]}))?;
    let persistence = Arc::new(ConfigPersistence::new(
        path.clone(),
        instances.clone(),
        "127.0.0.1".into(),
        8080,
        "info".into(),
        true,
        IndexMap::new(),
        IndexMap::new(),
        None,
        &original,
    ));
    let app = build_v1_router(
        instances.clone(),
        Arc::new(false),
        Some(persistence.clone()),
        plugins.clone(),
        None,
    );
    let payload = input(&output, json!({"type":"bounded","capacity":8}), false)?;
    let config: ComputationConfig = serde_json::from_value(payload.clone())?;
    let (status, body) = request(
        &app,
        "POST",
        "/instances/first/computation/components",
        &serde_yaml::to_string(&config)?,
        "application/yaml",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!output.exists());
    let (status, snapshot) = request(
        &app,
        "GET",
        "/instances/first/computation/configuration",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        snapshot["data"]["native_components"]["topology"]["resource_configurations"]
            ["server-pipe/events"]["name"],
        "events"
    );
    let saved = drasi_server::load_config_file(&path)?;
    assert_eq!(
        serde_json::to_value(&saved.instances[0].computation)?["pipes"]["events"],
        json!({"type":"bounded","capacity":8})
    );
    let first_revision = first.computation_control()?.desired_snapshot().revision;
    let second_revision = second.computation_control()?.desired_snapshot().revision;
    let bytes = std::fs::read(&path)?;
    for route in ["sources/producer", "reactions/consumer"] {
        let (status, body) = request(
            &app,
            "DELETE",
            &format!("/instances/first/{route}"),
            "",
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let mut collision = payload.clone();
    for (old, new) in [
        ("producer", "other-producer"),
        ("consumer", "other-consumer"),
    ] {
        let mut component = collision["components"]
            .as_object_mut()
            .unwrap()
            .remove(old)
            .unwrap();
        component["factory"]["descriptor"]["id"] = json!(new);
        collision["components"][new] = component;
    }
    let (status, body) = request(
        &app,
        "POST",
        "/instances/first/computation/components",
        &collision.to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(first.computation_component("other-producer").is_err());
    let mut malformed = config.definition.clone();
    if let DesiredPipe::External {
        exclusive_resources,
        ..
    } = &mut malformed.relationships[0].pipe
    {
        exclusive_resources.clear();
    }
    let (status, body) = request(
        &app,
        "POST",
        "/instances/second/computation/components",
        &json!({"definition":malformed}).to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let mut cross_instance = payload.clone();
    cross_instance["components"]
        .as_object_mut()
        .unwrap()
        .remove("producer");
    let (status, body) = request(
        &app,
        "POST",
        "/instances/second/computation/components",
        &cross_instance.to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        second.computation_control()?.desired_snapshot().revision,
        second_revision
    );
    let (status, _) = request(
        &app,
        "POST",
        "/instances/missing/computation/components",
        &payload.to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let readonly = build_v1_router(
        instances.clone(),
        Arc::new(true),
        None,
        plugins.clone(),
        None,
    );
    let (status, _) = request(
        &readonly,
        "POST",
        "/instances/second/computation/components",
        &payload.to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = request(
        &app,
        "DELETE",
        "/instances/first/computation/components",
        &json!({"components":["producer","consumer"]}).to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "named resource must be removed with its endpoints"
    );
    assert_eq!(
        first.computation_control()?.desired_snapshot().revision,
        first_revision
    );
    assert_eq!(std::fs::read(&path)?, bytes);
    let result = clone_instance(
        instances.clone(),
        Arc::new(false),
        plugins,
        Some(persistence.clone()),
        "second",
        "first",
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?
    .0
    .data
    .context("clone result")?;
    assert!(result.success, "{:?}", result.errors);
    let saved = drasi_server::load_config_file(&path)?;
    for instance in &saved.instances {
        let named = serde_json::to_value(&instance.computation)?;
        assert_eq!(
            named["pipes"]["events"],
            json!({"type":"bounded","capacity":8})
        );
        assert_eq!(named["components"]["producer"]["ports"]["out"], "events");
        assert_eq!(
            named["components"]["consumer"]["lifecycle"]["auto_start"],
            false
        );
    }
    start(&first, &config).await?;
    let expected = wait_output(&output).await?;
    first.shutdown().await?;
    std::fs::remove_file(&output)?;
    start(&second, &config).await?;
    assert_eq!(wait_output(&output).await?, expected);
    second.shutdown().await?;
    std::fs::remove_file(&output)?;
    let restored = Arc::new(DrasiLib::builder().with_id("first").build().await?);
    register_components(
        saved.instances[0].computation.as_ref().unwrap(),
        &restored,
        &registry()?,
    )
    .await?;
    start(&restored, &config).await?;
    assert_eq!(wait_output(&output).await?, expected);
    let single = InstanceRegistry::new();
    single
        .add("first".into(), restored.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let persistence = Arc::new(ConfigPersistence::new(
        path.clone(),
        single.clone(),
        "127.0.0.1".into(),
        8080,
        "info".into(),
        true,
        IndexMap::new(),
        IndexMap::new(),
        None,
        &saved,
    ));
    persistence
        .register_instance(saved.instances[0].clone())
        .await;
    let app = build_v1_router(
        single,
        Arc::new(false),
        Some(persistence),
        Arc::new(RwLock::new(registry()?)),
        None,
    );
    let (status, body) = request(
        &app,
        "DELETE",
        "/instances/first/computation/components",
        &json!({"components":["producer","consumer"],"resources":["server-pipe/events"]})
            .to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(drasi_server::load_config_file(&path)?.computation.is_none());
    restored.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn named_qos_storage_replays_unacknowledged_subscribers_and_isolates_instances() -> Result<()>
{
    use drasi_core::models::{ElementMetadata, ElementReference, SourceChange};
    let directory = tempfile::tempdir()?;
    let config: ComputationConfig = serde_json::from_value(input(
        &directory.path().join("unused.jsonl"),
        json!({"type":"qos","capacity":2,"path":directory.path().join("journals")}),
        true,
    )?)?;
    let registry = registry()?;
    let first = DrasiLib::builder().with_id("first").build().await?;
    let prepared = batch(&config, &first, &registry).await?;
    let resource = ResourceId::try_new("server-pipe/events")?;
    let channel = prepared.bindings.resources[&resource].get::<QosChannel>()?;
    let change = GraphChangeCodec::encode_change(
        SourceChange::Delete {
            metadata: ElementMetadata {
                reference: ElementReference::new("input", "row"),
                labels: Arc::from([Arc::from("Item")]),
                effective_from: 1,
            },
        },
        StreamId::try_new("events")?,
        1,
        None,
    )?;
    channel.publish(&change).await?;
    let qos_configs = config
        .definition
        .relationships
        .iter()
        .map(|edge| match &edge.pipe {
            DesiredPipe::Qos(pipe) => pipe.clone(),
            _ => panic!("QoS"),
        })
        .collect::<Vec<_>>();
    {
        let mut binding = qos_configs[0].create_with_resources(&prepared.bindings.resources)?;
        let mut receiver = binding.pipe.take_receiver()?;
        let delivery = receiver.receive().await?.context("first delivery")?;
        assert_eq!(delivery.envelope().system().sequence(), 1);
        delivery
            .into_parts()
            .1
            .context("acknowledgement")?
            .complete(HandlingOutcome::Handled)
            .await?;
    }
    {
        let mut binding = qos_configs[1].create_with_resources(&prepared.bindings.resources)?;
        let mut receiver = binding.pipe.take_receiver()?;
        let delivery = receiver.receive().await?.context("second delivery")?;
        assert_eq!(delivery.envelope().system().sequence(), 1);
        drop(delivery);
    }
    ResourceCleanup::shutdown(channel.as_ref()).await?;
    drop(channel);
    drop(prepared);
    let reopened = batch(&config, &first, &registry).await?;
    {
        let mut binding = qos_configs[0].create_with_resources(&reopened.bindings.resources)?;
        let mut receiver = binding.pipe.take_receiver()?;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), receiver.receive())
                .await
                .is_err(),
            "acknowledged subscriber must not replay"
        );
    }
    {
        let mut binding = qos_configs[1].create_with_resources(&reopened.bindings.resources)?;
        let mut receiver = binding.pipe.take_receiver()?;
        let replay = tokio::time::timeout(Duration::from_secs(2), receiver.receive())
            .await??
            .context("replayed delivery")?;
        assert_eq!(replay.envelope().system().sequence(), 1);
        replay
            .into_parts()
            .1
            .context("replay acknowledgement")?
            .complete(HandlingOutcome::Handled)
            .await?;
    }
    let second = DrasiLib::builder().with_id("second").build().await?;
    let independent = batch(&config, &second, &registry).await?;
    let other_channel = independent.bindings.resources[&resource].get::<QosChannel>()?;
    assert_eq!(
        other_channel.publish(&change).await?.position(),
        Some(1),
        "clone instance starts with an independent journal"
    );
    let reopened_channel = reopened.bindings.resources[&resource].get::<QosChannel>()?;
    ResourceCleanup::shutdown(reopened_channel.as_ref()).await?;
    ResourceCleanup::shutdown(other_channel.as_ref()).await?;
    drop(reopened_channel);
    drop(other_channel);
    drop(reopened);
    drop(independent);
    first.shutdown().await?;
    second.shutdown().await?;
    Ok(())
}

#[test]
fn named_qos_and_policy_roundtrip_is_exact() -> Result<()> {
    let mut value = input(
        Path::new("unused"),
        json!({"type":"qos","capacity":8,"retention":"PruneOldest"}),
        true,
    )?;
    let policy = RelationshipPolicy {
        dynamically_replaceable: false,
        propagate_failure: true,
        ..Default::default()
    };
    value["components"]["consumer"]["ports"]["in"] = json!({"pipe":"events","subscriber":"cursor-one","start":{"After":3},"gapPolicy":"SkipWithNotification","policy":policy});
    let config: ComputationConfig = serde_json::from_value(value.clone())?;
    let yaml = serde_yaml::to_string(&config)?;
    let restored: ComputationConfig = serde_yaml::from_str(&yaml)?;
    assert_eq!(restored.definition, config.definition);
    let mut bad = value.clone();
    bad["components"]["second"]["ports"]["in"]["subscriber"] = json!("cursor-one");
    assert!(serde_json::from_value::<ComputationConfig>(bad).is_err());
    let mut bad = value.clone();
    bad["pipes"]["events"]["retention"] = json!("Backpressure");
    assert!(serde_json::from_value::<ComputationConfig>(bad).is_err());
    let mut bad = value;
    bad["components"]["producer"]["streams"] = json!({});
    assert!(serde_json::from_value::<ComputationConfig>(bad).is_err());
    use utoipa::OpenApi;
    let schema = serde_json::to_value(drasi_server::api::v1::openapi::ApiDocV1::openapi())?;
    assert!(schema["components"]["schemas"]["ComputationConfig"]["oneOf"].is_array());
    assert!(schema["components"]["schemas"]["NamedGraphConfig"]["properties"]["pipes"].is_object());
    Ok(())
}

#[tokio::test]
async fn native_query_delete_cannot_orphan_a_named_pipe() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut wire = input(
        &directory.path().join("unused"),
        json!({"type":"qos","capacity":8}),
        false,
    )?;
    wire["components"]
        .as_object_mut()
        .unwrap()
        .remove("consumer");
    let definition = ContinuousQueryDefinition {
        graph_id: "__drasi_lib_runtime__".into(),
        id: id("native-query"),
        query: "MATCH (n) RETURN n".into(),
        language: ComputationQueryLanguage::Cypher,
        output_stream: StreamId::try_new("query-results")?,
        outbox_capacity: std::num::NonZeroUsize::new(8).unwrap(),
    };
    let indexes = ResourceId::try_new("indexes")?;
    let specification = ComponentSpecification {
        descriptor: definition.descriptor(),
        role: ComponentRole::Query,
        completion: None,
        implementation: ContinuousQueryFactory::default()
            .descriptor()
            .implementation
            .clone(),
        configuration_version: 1,
        configuration: BTreeMap::from([
            (
                "query".into(),
                ConfigurationValue::Literal(json!(definition.query)),
            ),
            (
                "stream".into(),
                ConfigurationValue::Literal(json!(definition.output_stream)),
            ),
        ]),
        dependencies: BTreeMap::from([("indexes".into(), vec![indexes.clone()])]),
    };
    wire["components"]["native-query"] = json!({
        "factory":specification,"ports":{"in":"events"},"streams":{"out":"query-results"},"lifecycle":{"auto_start":false}
    });
    wire["resources"] =
        json!([{"id":indexes,"role":"IndexBackend","ownership":"Graph","binding":"indexes"}]);
    wire["resourceConfigurations"] = json!({"indexes":{"kind":"memoryIndexes"}});
    wire["allowIncomplete"] = json!(true);
    let config: ComputationConfig = serde_json::from_value(wire)?;
    let core = Arc::new(DrasiLib::builder().with_id("query").build().await?);
    let plugins = registry()?;
    register_components(&config, &core, &plugins).await?;
    let revision = core.computation_control()?.desired_snapshot().revision;
    let registry = InstanceRegistry::new();
    registry
        .add("query".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let app = build_v1_router(
        registry,
        Arc::new(false),
        None,
        Arc::new(RwLock::new(plugins)),
        None,
    );
    let (status, body) = request(
        &app,
        "DELETE",
        "/instances/query/queries/native-query",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        core.computation_control()?.desired_snapshot().revision,
        revision
    );
    assert!(
        configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?.is_some()
    );
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn durable_named_qos_clone_uses_a_new_journal() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("capture.jsonl");
    let config: ComputationConfig = serde_json::from_value(input(
        &output,
        json!({"type":"qos","capacity":8,"path":directory.path().join("journals")}),
        true,
    )?)?;
    let registry = registry()?;
    let first = Arc::new(DrasiLib::builder().with_id("original").build().await?);
    let second = Arc::new(DrasiLib::builder().with_id("clone").build().await?);
    register_components(&config, &first, &registry).await?;
    start(&first, &config).await?;
    let expected = wait_output(&output).await?;
    assert_eq!(
        wait_output(&output.with_extension("second.jsonl")).await?,
        expected
    );
    let instances = InstanceRegistry::new();
    instances
        .add("original".into(), first.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    instances
        .add("clone".into(), second.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let result = clone_instance(
        instances,
        Arc::new(false),
        Arc::new(RwLock::new(registry)),
        None,
        "clone",
        "original",
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?
    .0
    .data
    .context("clone response")?;
    assert!(result.success, "{:?}", result.errors);
    assert_eq!(
        std::fs::read_dir(directory.path().join("journals"))?.count(),
        2,
        "separate instance-scoped roots"
    );
    first.shutdown().await?;
    std::fs::remove_file(&output)?;
    std::fs::remove_file(output.with_extension("second.jsonl"))?;
    start(&second, &config).await?;
    assert_eq!(wait_output(&output).await?, expected);
    assert_eq!(
        wait_output(&output.with_extension("second.jsonl")).await?,
        expected
    );
    second.shutdown().await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn stock_binary_loads_named_config_and_accepts_named_api_batches() -> Result<()> {
    use std::process::Stdio;
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("capture.jsonl");
    let plugins = directory.path().join("plugins");
    std::fs::create_dir(&plugins)?;
    let library = plugin_path();
    std::fs::copy(
        &library,
        plugins.join(library.file_name().context("plugin filename")?),
    )?;
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let computation: ComputationConfig = serde_json::from_value(input(
        &output,
        json!({"type":"bounded","capacity":8}),
        false,
    )?)?;
    let mut config = DrasiServerConfig {
        id: drasi_server::api::models::ConfigValue::Static("stock-named".into()),
        computation: Some(computation),
        ..Default::default()
    };
    config.host = drasi_server::api::models::ConfigValue::Static("127.0.0.1".into());
    config.port = drasi_server::api::models::ConfigValue::Static(port);
    config.enable_ui = false;
    config.auto_install_plugins = false;
    let path = directory.path().join("server.yaml");
    config.save_to_file(&path)?;
    let validation =
        drasi_server::config::plugin_validation::validate_with_plugins(&config, Some(&plugins));
    assert!(!validation.has_errors(), "{validation:?}");
    let log = directory.path().join("server.log");
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_drasi-server"))
        .arg("--config")
        .arg(&path)
        .arg("--port")
        .arg(port.to_string())
        .arg("--plugins-dir")
        .arg(&plugins)
        .args(["--skip-verification", "--disable-ui"])
        .env("RUST_LOG", "warn")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&log)?)
        .stderr(std::fs::File::create(directory.path().join("stderr.log"))?)
        .kill_on_drop(true)
        .spawn()?;
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}/api/v1/instances/stock-named/computation");
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::bail!(
                    "stock Server exited {status}: {}",
                    std::fs::read_to_string(&log)?
                );
            }
            if let Ok(response) = client.get(&base).send().await {
                if response.status().is_success() {
                    break Ok::<_, anyhow::Error>(());
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await??;
    assert!(!output.exists());
    let response = client
        .post(format!("{base}/start"))
        .json(&json!({"components":["producer","consumer"]}))
        .send()
        .await?;
    assert!(response.status().is_success(), "{}", response.text().await?);
    assert_eq!(wait_output(&output).await?.len(), 4);
    let second_output = directory.path().join("second-batch.jsonl");
    let mut payload = input(&second_output, json!({"type":"qos","capacity":8}), false)?;
    payload["pipes"] = json!({"next":{"type":"qos","capacity":8}});
    for (old, new, port) in [
        ("producer", "next-producer", "out"),
        ("consumer", "next-consumer", "in"),
    ] {
        let mut component = payload["components"]
            .as_object_mut()
            .unwrap()
            .remove(old)
            .unwrap();
        component["factory"]["descriptor"]["id"] = json!(new);
        component["ports"][port] = json!("next");
        payload["components"][new] = component;
    }
    payload["components"]["next-producer"]["streams"]["out"] = json!("next-events");
    payload["components"]["next-producer"]["factory"]["configuration"]["stream"] =
        json!({"Literal":"next-events"});
    let response = client
        .post(format!("{base}/components"))
        .json(&payload)
        .send()
        .await?;
    assert!(response.status().is_success(), "{}", response.text().await?);
    let response = client
        .post(format!("{base}/start"))
        .json(&json!({"components":["next-producer","next-consumer"]}))
        .send()
        .await?;
    assert!(response.status().is_success(), "{}", response.text().await?);
    assert_eq!(wait_output(&second_output).await?.len(), 4);
    let saved = serde_json::to_value(drasi_server::load_config_file(&path)?.computation)?;
    assert_eq!(saved["pipes"].as_object().context("named pipes")?.len(), 2);
    assert_eq!(
        saved["components"]["next-consumer"]["ports"]["in"]["pipe"],
        "next"
    );
    let pid = child.id().context("Server PID")?;
    assert!(std::process::Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()?
        .success());
    let status = tokio::time::timeout(Duration::from_secs(20), child.wait()).await??;
    assert!(
        status.success(),
        "{status}: {}",
        std::fs::read_to_string(&log)?
    );
    Ok(())
}

#[test]
fn named_references_and_transport_descriptors_are_strict() -> Result<()> {
    let valid = input(
        Path::new("not-started.jsonl"),
        json!({"type":"bounded","capacity":8}),
        false,
    )?;
    let mut cases = Vec::new();
    let mut case = valid.clone();
    case["pipes"]["events"]["capacity"] = json!(0);
    cases.push(case);
    let mut case = valid.clone();
    case["pipes"]["events"]["type"] = json!("retained");
    cases.push(case);
    let mut case = valid.clone();
    case["pipes"]["events"]["extra"] = json!(true);
    cases.push(case);
    let mut case = valid.clone();
    case["components"]["producer"]["ports"]["out"] = json!("missing");
    cases.push(case);
    let mut case = valid.clone();
    case["components"]["producer"]["ports"] = json!({"in":"events"});
    cases.push(case);
    let mut case = valid.clone();
    case["components"]["consumer"]["factory"]["descriptor"]["ports"][0]["direction"] =
        json!("Output");
    cases.push(case);
    let mut case = valid.clone();
    case["components"]["consumer"]["factory"]["descriptor"]["ports"][0]["schema"]["encoding"] =
        json!("incompatible");
    cases.push(case);
    let mut case = valid.clone();
    case["components"]["consumer"]["factory"]["descriptor"]["ports"][0]["requirements"] =
        serde_json::to_value(PipeRequirements::new([PipeCapability::ExactlyOnce]))?;
    cases.push(case);
    let mut case = valid.clone();
    case["components"]
        .as_object_mut()
        .unwrap()
        .remove("producer");
    cases.push(case);
    let mut case = valid.clone();
    case["components"]
        .as_object_mut()
        .unwrap()
        .remove("consumer");
    cases.push(case);
    let mut case = valid.clone();
    case["components"]["consumer"]["ports"]["in"] =
        json!({"pipe":"events","subscriber":"unexpected"});
    cases.push(case);
    let mut case = valid.clone();
    case["components"]["consumer"]["ports"]["in"] = json!({"pipe":"events","typo":true});
    cases.push(case);
    cases.push(input(
        Path::new("not-started.jsonl"),
        json!({"type":"bounded","capacity":8}),
        true,
    )?);
    for case in cases {
        assert!(
            serde_json::from_value::<ComputationConfig>(case.clone()).is_err(),
            "accepted {case}"
        );
    }
    let config: ComputationConfig = serde_json::from_value(valid)?;
    let original = config.definition.relationships[0].pipe.clone();
    let DesiredPipe::External {
        binding,
        capabilities,
        capacity,
        resources,
        exclusive_resources,
    } = original
    else {
        anyhow::bail!("expected named resource binding")
    };
    for pipe in [
        DesiredPipe::External {
            binding: "forged".into(),
            capabilities: capabilities.clone(),
            capacity,
            resources: resources.clone(),
            exclusive_resources: exclusive_resources.clone(),
        },
        DesiredPipe::External {
            binding: binding.clone(),
            capabilities: vec![PipeCapability::ExactlyOnce],
            capacity,
            resources: resources.clone(),
            exclusive_resources: exclusive_resources.clone(),
        },
        DesiredPipe::External {
            binding: binding.clone(),
            capabilities: capabilities.clone(),
            capacity: Some(99),
            resources: resources.clone(),
            exclusive_resources: exclusive_resources.clone(),
        },
        DesiredPipe::External {
            binding: binding.clone(),
            capabilities: capabilities.clone(),
            capacity,
            resources: BTreeMap::new(),
            exclusive_resources: exclusive_resources.clone(),
        },
        DesiredPipe::External {
            binding,
            capabilities,
            capacity,
            resources,
            exclusive_resources: vec![],
        },
    ] {
        let mut forged = config.clone();
        forged.definition.relationships[0].pipe = pipe;
        assert!(validate_definition(&forged).is_err());
    }
    let mut doubled = config.clone();
    doubled
        .definition
        .relationships
        .push(doubled.definition.relationships[0].clone());
    assert!(validate_definition(&doubled).is_err());
    let duplicate = r#"{"pipes":{"events":{"type":"bounded","capacity":1},"events":{"type":"bounded","capacity":2}},"components":{}}"#;
    assert!(serde_json::from_str::<ComputationConfig>(duplicate)
        .unwrap_err()
        .to_string()
        .contains("duplicate key"));
    Ok(())
}
