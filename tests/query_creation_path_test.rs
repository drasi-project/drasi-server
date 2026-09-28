// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_lib::{computation::v1::*, DrasiLib, Query};
use drasi_plugin_sdk::{ReactionPluginDescriptor, SourcePluginDescriptor};
use drasi_reaction_application::{ApplicationReaction, ApplicationReactionHandle};
use drasi_server::{
    api::v1::routes::build_v1_router, instance_registry::InstanceRegistry,
    plugin_registry::PluginRegistry,
};
use drasi_source_application::{
    ApplicationSource, ApplicationSourceConfig, ApplicationSourceHandle, PropertyMapBuilder,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock};
use tower::ServiceExt;

const DEADLINE: Duration = Duration::from_secs(10);

struct SourceFactory(Arc<Mutex<Vec<ApplicationSourceHandle>>>);

#[async_trait::async_trait]
impl SourcePluginDescriptor for SourceFactory {
    fn kind(&self) -> &str {
        "application"
    }
    fn config_version(&self) -> &str {
        "1.0.0"
    }
    fn config_schema_name(&self) -> &str {
        "ApplicationSourceFixture"
    }
    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.into()
    }
    async fn create_source(
        &self,
        id: &str,
        config: &Value,
        auto_start: bool,
    ) -> Result<Box<dyn drasi_lib::Source>> {
        anyhow::ensure!(auto_start, "this fixture requires auto-start");
        let (source, input) = ApplicationSource::new(
            id,
            ApplicationSourceConfig {
                properties: serde_json::from_value(config.clone())?,
                durability: None,
            },
        )?;
        self.0.lock().await.push(input);
        Ok(Box::new(source))
    }
}

struct ReactionFactory(Arc<Mutex<Vec<ApplicationReactionHandle>>>);

#[async_trait::async_trait]
impl ReactionPluginDescriptor for ReactionFactory {
    fn kind(&self) -> &str {
        "application"
    }
    fn config_version(&self) -> &str {
        "1.0.0"
    }
    fn config_schema_name(&self) -> &str {
        "ApplicationReactionFixture"
    }
    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.into()
    }
    async fn create_reaction(
        &self,
        id: &str,
        queries: Vec<String>,
        _: &Value,
        auto_start: bool,
    ) -> Result<Box<dyn drasi_lib::Reaction>> {
        anyhow::ensure!(auto_start, "this fixture requires auto-start");
        let (reaction, output) = ApplicationReaction::new(id, queries);
        self.0.lock().await.push(output);
        Ok(Box::new(reaction))
    }
}

async fn request(
    app: &Router,
    method: &str,
    uri: &str,
    value: Value,
) -> Result<(StatusCode, Value)> {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&value)?))?,
        )
        .await?;
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

async fn scenario(native: bool) -> Result<()> {
    let (source, input) = ApplicationSource::new(
        "input",
        ApplicationSourceConfig {
            properties: Default::default(),
            durability: None,
        },
    )?;
    let core = Arc::new(
        DrasiLib::builder()
            .with_id("first")
            .with_source(source)
            .build()
            .await?,
    );
    core.computation_component("input")?.wait_created().await?;
    let second = Arc::new(DrasiLib::builder().with_id("second").build().await?);
    let instances = InstanceRegistry::new();
    instances
        .add("first".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    instances
        .add("second".into(), second.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let source_inputs = Arc::new(Mutex::new(Vec::new()));
    let reaction_outputs = Arc::new(Mutex::new(Vec::new()));
    let mut plugins = PluginRegistry::new();
    plugins.register_source(Arc::new(SourceFactory(source_inputs.clone())));
    plugins.register_reaction(Arc::new(ReactionFactory(reaction_outputs.clone())));
    let app = build_v1_router(
        instances,
        Arc::new(false),
        None,
        Arc::new(RwLock::new(plugins)),
        None,
    );
    let config = Query::cypher("items")
        .query("MATCH (n:Item) RETURN n.value AS value")
        .from_source("input")
        .enable_bootstrap(false)
        .build();
    if native {
        let batch = core
            .computation_pipeline()?
            .source(
                core.borrow_computation_source("input").await?,
                SourceSubscriptionOptions::default(),
            )?
            .query(config.clone())
            .build()?;
        core.add_components(batch).await?;
    } else {
        let dto = drasi_server::api::models::QueryConfigDto::try_from(config.clone())?;
        let (status, body) = request(
            &app,
            "POST",
            "/instances/first/queries",
            serde_json::to_value(dto)?,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "{body}");
    }
    for prefix in ["/instances/first", ""] {
        let (status, body) =
            request(&app, "GET", &format!("{prefix}/queries"), Value::Null).await?;
        anyhow::ensure!(status == StatusCode::OK, "{body}");
        assert!(body["data"]
            .as_array()
            .context("query list")?
            .iter()
            .any(|item| item["id"] == "items"));
        let (status, body) = request(
            &app,
            "GET",
            &format!("{prefix}/queries/items?view=full"),
            Value::Null,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "{body}");
        assert_eq!(body["data"]["id"], "items");
        assert_eq!(body["data"]["config"]["query"], config.query);
        for path in ["logs", "events"] {
            let (status, body) = request(
                &app,
                "GET",
                &format!("{prefix}/queries/items/{path}"),
                Value::Null,
            )
            .await?;
            anyhow::ensure!(status == StatusCode::OK, "{path}: {body}");
        }
    }
    let (status, _) = request(&app, "GET", "/instances/second/queries/items", Value::Null).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    core.start().await?;
    core.computation_component("items")?.wait_started().await?;
    let stream = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/instances/first/queries/items/attach")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(stream.status(), StatusCode::OK);
    let mut stream = stream.into_body().into_data_stream();
    input
        .send_node_insert(
            "one",
            vec!["Item"],
            PropertyMapBuilder::new().with_integer("value", 7).build(),
        )
        .await?;
    use futures_util::StreamExt;
    let event = tokio::time::timeout(DEADLINE, stream.next())
        .await?
        .context("SSE event")??;
    let event = String::from_utf8(event.to_vec())?;
    let data = event
        .lines()
        .find_map(|line| line.strip_prefix("data:"))
        .context("SSE data")?;
    let result: Value = serde_json::from_str(data.trim())?;
    assert_eq!(result["query_id"], "items");
    for prefix in ["/instances/first", ""] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("{prefix}/queries/items/results"),
            Value::Null,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "{body}");
        assert_eq!(body["data"], json!([{"value":7}]));
    }
    drop(stream);
    tokio::time::timeout(DEADLINE, async {
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        loop {
            if core
                .list_reactions()
                .await?
                .iter()
                .all(|(id, _)| !id.starts_with("__attach_items_"))
            {
                return Ok::<_, anyhow::Error>(());
            }
            interval.tick().await;
        }
    })
    .await
    .context("closed SSE attachment must release its temporary reaction")??;
    for operation in ["stop", "start"] {
        let (status, body) = request(
            &app,
            "POST",
            &format!("/instances/first/queries/items/{operation}"),
            Value::Null,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "{operation}: {body}");
    }
    let mut replacement = config.clone();
    replacement.query = "MATCH (n:Item) RETURN n.value * 2 AS value".into();
    core.update_query("items", replacement).await?;
    let (status, body) = request(
        &app,
        "PUT",
        "/instances/first/sources/input",
        json!({"id":"input","kind":"application","autoStart":true}),
    )
    .await?;
    anyhow::ensure!(status == StatusCode::OK, "source replacement: {body}");
    let replacement_input = source_inputs
        .lock()
        .await
        .pop()
        .context("replacement source input")?;
    for _ in 0..2 {
        let (status, body) = request(
            &app,
            "PUT",
            "/instances/first/reactions/capture",
            json!({"id":"capture","kind":"application","queries":["items"],"autoStart":true}),
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "reaction upsert: {body}");
        tokio::time::timeout(
            DEADLINE,
            core.computation_component("capture")?.wait_started(),
        )
        .await??;
    }
    let output = reaction_outputs
        .lock()
        .await
        .pop()
        .context("updated reaction output")?;
    let mut received = output
        .take_receiver()
        .await
        .context("updated reaction receiver")?;
    replacement_input
        .send_node_insert(
            "after-replacement",
            vec!["Item"],
            PropertyMapBuilder::new().with_integer("value", 11).build(),
        )
        .await?;
    let event = tokio::time::timeout(DEADLINE, received.recv())
        .await?
        .context("updated query output")?;
    assert_eq!(event.query_id, "items");
    assert!(matches!(&event.results[..],
        [drasi_lib::channels::ResultDiff::Add { data, .. }] if data == &json!({"value":22})
    ));
    for prefix in ["/instances/first", ""] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("{prefix}/queries/items/results"),
            Value::Null,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "{body}");
        assert_eq!(body["data"], json!([{"value":22}]));
    }
    let (status, _) = request(
        &app,
        "DELETE",
        "/instances/first/queries/items",
        Value::Null,
    )
    .await?;
    assert!(
        !status.is_success(),
        "a dependent reaction must prevent query removal"
    );
    let (status, body) = request(
        &app,
        "DELETE",
        "/instances/first/reactions/capture",
        Value::Null,
    )
    .await?;
    anyhow::ensure!(status == StatusCode::OK, "{body}");
    let (status, body) = request(
        &app,
        "DELETE",
        "/instances/first/queries/items",
        Value::Null,
    )
    .await?;
    anyhow::ensure!(status == StatusCode::OK, "{body}");
    for prefix in ["/instances/first", ""] {
        let (status, _) =
            request(&app, "GET", &format!("{prefix}/queries/items"), Value::Null).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    core.shutdown().await?;
    second.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn ordinary_query_creation_supports_default_and_instance_management_routes() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), scenario(false)).await?
}

#[tokio::test]
async fn native_query_creation_supports_default_and_instance_management_routes() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), scenario(true)).await?
}

struct NativeLifecycleComponent {
    descriptor: ComponentDescriptor,
    opaque: bool,
}

#[async_trait::async_trait]
impl ComputationComponent for NativeLifecycleComponent {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<Value> {
        anyhow::ensure!(!self.opaque, "fixture configuration is deliberately opaque");
        Ok(json!({"marker":"native-component"}))
    }
    async fn start(&mut self) -> Result<()> {
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }
    async fn deprovision(&mut self) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl ComputationService for NativeLifecycleComponent {
    async fn run(&mut self) -> Result<()> {
        std::future::pending().await
    }
}

async fn native_lifecycle_routes(kind: ComponentSemanticKind, route: &str) -> Result<()> {
    let core = Arc::new(
        DrasiLib::builder()
            .with_id("native-components")
            .build()
            .await?,
    );
    let batch = ComponentBatch::builder()
        .service(Box::new(NativeLifecycleComponent {
            descriptor: ComponentDescriptor::try_new(
                ComponentId::try_new("component")?,
                Vec::new(),
            )?
            .with_semantic_kind(kind),
            opaque: false,
        }))
        .build()?;
    assert!(core.add_components(batch).await?.committed);
    let registry = InstanceRegistry::new();
    registry
        .add("native-components".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let app = Router::new().nest(
        "/api/v1",
        build_v1_router(
            registry,
            Arc::new(false),
            None,
            Arc::new(RwLock::new(PluginRegistry::new())),
            None,
        ),
    );
    for prefix in ["/api/v1", "/api/v1/instances/native-components"] {
        let (status, body) =
            request(&app, "GET", &format!("{prefix}/{route}"), Value::Null).await?;
        anyhow::ensure!(status == StatusCode::OK, "{body}");
        assert!(body["data"]
            .as_array()
            .context("component list")?
            .iter()
            .any(|item| item["id"] == "component"));
        let (status, body) = request(
            &app,
            "GET",
            &format!("{prefix}/{route}/component?view=full"),
            Value::Null,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "{body}");
        assert_eq!(body["data"]["config"]["marker"], "native-component");
        for suffix in ["events", "logs"] {
            let (status, body) = request(
                &app,
                "GET",
                &format!("{prefix}/{route}/component/{suffix}"),
                Value::Null,
            )
            .await?;
            anyhow::ensure!(status == StatusCode::OK, "{body}");
        }
    }
    assert!(core.get_graph().await.contains("component"));
    let descriptive = core.snapshot_configuration().await?;
    if kind == ComponentSemanticKind::Source {
        assert!(descriptive
            .sources
            .iter()
            .any(|source| source.id == "component"));
        assert!(core.get_source_schema("component").await?.is_none());
    } else {
        assert!(descriptive
            .reactions
            .iter()
            .any(|reaction| reaction.id == "component"));
    }
    let reconstruction = core.snapshot_computation_configuration().await?;
    assert!(!reconstruction
        .instance
        .sources
        .iter()
        .any(|source| source.id == "component"));
    assert!(!reconstruction
        .instance
        .reactions
        .iter()
        .any(|reaction| reaction.id == "component"));
    assert!(reconstruction
        .native_components
        .context("native reconstruction")?
        .topology
        .components
        .iter()
        .any(|component| component.descriptor.id().as_str() == "component"));
    for operation in ["start", "stop", "start"] {
        let (status, body) = request(
            &app,
            "POST",
            &format!("/api/v1/{route}/component/{operation}"),
            Value::Null,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "{operation}: {body}");
    }
    let (status, _) = request(
        &app,
        "POST",
        &format!("/api/v1/{route}/component/start"),
        Value::Null,
    )
    .await?;
    assert!(
        !status.is_success(),
        "starting an already running native component must match ordinary lifecycle rules"
    );
    let (status, body) = request(
        &app,
        "DELETE",
        &format!("/api/v1/{route}/component"),
        Value::Null,
    )
    .await?;
    anyhow::ensure!(status == StatusCode::OK, "{body}");
    let (status, _) = request(
        &app,
        "GET",
        &format!("/api/v1/{route}/component"),
        Value::Null,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    core.add_components(
        ComponentBatch::builder()
            .service(Box::new(NativeLifecycleComponent {
                descriptor: ComponentDescriptor::try_new(
                    ComponentId::try_new("namespace/component")?,
                    Vec::new(),
                )?
                .with_semantic_kind(kind),
                opaque: false,
            }))
            .build()?,
    )
    .await?;
    let (status, body) = request(&app, "GET", &format!("/api/v1/{route}"), Value::Null).await?;
    anyhow::ensure!(status == StatusCode::OK, "{body}");
    let named = body["data"]
        .as_array()
        .context("namespaced component list")?
        .iter()
        .find(|component| component["id"] == "namespace/component")
        .context("namespaced component")?;
    let link = named["links"]["full"]
        .as_str()
        .context("full component link")?;
    assert!(link.contains("namespace%2Fcomponent"), "{link}");
    let (status, body) = request(&app, "GET", link, Value::Null).await?;
    anyhow::ensure!(status == StatusCode::OK, "follow component link: {body}");

    let opaque = ComponentBatch::builder()
        .service(Box::new(NativeLifecycleComponent {
            descriptor: ComponentDescriptor::try_new(ComponentId::try_new("opaque")?, Vec::new())?
                .with_semantic_kind(kind),
            opaque: true,
        }))
        .build()?;
    core.add_components(opaque).await?;
    let (status, body) = request(&app, "GET", &format!("/api/v1/{route}"), Value::Null).await?;
    anyhow::ensure!(status == StatusCode::OK, "{body}");
    assert!(body["data"]
        .as_array()
        .context("opaque list")?
        .iter()
        .any(|item| item["id"] == "opaque"));
    let (status, _) = request(
        &app,
        "GET",
        &format!("/api/v1/{route}/opaque?view=full"),
        Value::Null,
    )
    .await?;
    assert!(
        !status.is_success(),
        "unavailable configuration must not be invented"
    );
    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "an opaque component still exists"
    );
    for suffix in ["events", "logs"] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/api/v1/{route}/opaque/{suffix}"),
            Value::Null,
        )
        .await?;
        anyhow::ensure!(status == StatusCode::OK, "opaque diagnostics: {body}");
    }
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn native_source_roles_use_ordinary_management_routes_without_a_shadow_instance() -> Result<()>
{
    tokio::time::timeout(
        Duration::from_secs(20),
        native_lifecycle_routes(ComponentSemanticKind::Source, "sources"),
    )
    .await?
}

#[tokio::test]
async fn native_reaction_roles_use_ordinary_management_routes_without_a_shadow_instance(
) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        native_lifecycle_routes(ComponentSemanticKind::Reaction, "reactions"),
    )
    .await?
}

struct FailingNativeFactory(FactoryDescriptor);

#[async_trait::async_trait]
impl ComponentFactory for FailingNativeFactory {
    fn descriptor(&self) -> &FactoryDescriptor {
        &self.0
    }
    fn validate(&self, _: &ComponentSpecification) -> Result<()> {
        Ok(())
    }
    async fn create(
        &self,
        _: ConstructionContext,
    ) -> std::result::Result<ConstructedComponent, ComponentCreationError> {
        Err(ComponentCreationError::terminal(anyhow::anyhow!(
            "injected native construction failure"
        )))
    }
}

#[tokio::test]
async fn failed_native_source_and_reaction_declarations_remain_manageable() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(20), async {
        for (kind, route) in [
            (ComponentSemanticKind::Source, "sources"),
            (ComponentSemanticKind::Reaction, "reactions"),
        ] {
            let core = Arc::new(DrasiLib::builder().with_id("failed-native").build().await?);
            let factory = Arc::new(FailingNativeFactory(FactoryDescriptor {
                implementation: ImplementationIdentity::try_new("fixture/native-role", "1")?,
                role: ComponentRole::Service,
                configuration_version: 1,
                configuration: ConfigurationSchema::default(),
                dependencies: Default::default(),
            }));
            let specification = ComponentSpecification {
                descriptor: ComponentDescriptor::try_new(
                    ComponentId::try_new("failed")?,
                    Vec::new(),
                )?
                .with_semantic_kind(kind),
                role: ComponentRole::Service,
                completion: None,
                implementation: factory.descriptor().implementation.clone(),
                configuration_version: 1,
                configuration: Default::default(),
                dependencies: Default::default(),
            };
            assert!(
                core.add_components(
                    ComponentBatch::builder()
                        .component(specification, factory)
                        .build()?
                )
                .await?
                .committed
            );
            assert!(core
                .computation_component("failed")?
                .wait_created()
                .await
                .is_err());
            let registry = InstanceRegistry::new();
            registry
                .add("failed-native".into(), core.clone())
                .await
                .map_err(anyhow::Error::msg)?;
            let app = build_v1_router(
                registry,
                Arc::new(false),
                None,
                Arc::new(RwLock::new(PluginRegistry::new())),
                None,
            );
            let (status, body) = request(
                &app,
                "GET",
                &format!("/{route}/failed?view=full"),
                Value::Null,
            )
            .await?;
            anyhow::ensure!(status == StatusCode::OK, "{body}");
            assert_eq!(body["data"]["status"], "Error");
            assert!(body["data"]["error_message"]
                .as_str()
                .context("construction error")?
                .contains("injected native construction failure"));
            let (status, body) =
                request(&app, "GET", &format!("/{route}/failed/events"), Value::Null).await?;
            anyhow::ensure!(status == StatusCode::OK, "{body}");
            assert!(!body["data"]
                .as_array()
                .context("failure events")?
                .is_empty());
            let (status, body) =
                request(&app, "DELETE", &format!("/{route}/failed"), Value::Null).await?;
            anyhow::ensure!(status == StatusCode::OK, "{body}");
            core.shutdown().await?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await?
}
