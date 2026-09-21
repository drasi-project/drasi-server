// Copyright 2026 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(clippy::unwrap_used)]

#[allow(dead_code)]
#[path = "test_support/mock_components.rs"]
mod mock_components;

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_lib::{
    channels::{ComponentStatus, QueryResult, SubscriptionResponse},
    computation::v1::{GraphEntity, GraphEntityId, GraphError, PluginIdentity, RealizationState},
    context::{ReactionRuntimeContext, SourceRuntimeContext},
    DrasiLib, ExecutionMode, Query, Reaction, Source,
};
use drasi_plugin_sdk::{ReactionPluginDescriptor, SourcePluginDescriptor};
use drasi_server::{
    api::mappings::DtoMapper, config::DrasiServerConfig, instance_registry::InstanceRegistry,
    persistence::ConfigPersistence, plugin_registry::PluginRegistry,
};
use drasi_source_application::{ApplicationSource, ApplicationSourceConfig, PropertyMapBuilder};
use futures_util::StreamExt;
use indexmap::IndexMap;
use mock_components::{MockReaction, MockSource};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::{Notify, RwLock};
use tower::ServiceExt;

const DEADLINE: Duration = Duration::from_secs(10);
const INVALID_QUERY: &str = "NOT A CYPHER QUERY";
const SOURCE_PLUGIN: &str = "mock-fixture";
const REACTION_PLUGIN: &str = "log-fixture";
const SOURCE_PACKAGE_VERSION: &str = "2.3.4";
const REACTION_PACKAGE_VERSION: &str = "3.4.5";

#[derive(Default)]
struct LifecycleProbe {
    starts: AtomicUsize,
    initializations: AtomicUsize,
    initialization_gate: Notify,
}

struct ProbedSource {
    inner: MockSource,
    auto_start: bool,
    properties: HashMap<String, Value>,
    probe: Arc<LifecycleProbe>,
}

#[async_trait]
impl Source for ProbedSource {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn type_name(&self) -> &str {
        self.inner.type_name()
    }

    fn properties(&self) -> HashMap<String, Value> {
        self.properties.clone()
    }

    fn auto_start(&self) -> bool {
        self.auto_start
    }

    async fn initialize(&self, context: SourceRuntimeContext) {
        self.probe.initializations.fetch_add(1, Ordering::SeqCst);
        if self.properties.get("blockInitialization") == Some(&Value::Bool(true)) {
            self.probe.initialization_gate.notified().await;
        }
        self.inner.initialize(context).await;
    }

    async fn start(&self) -> anyhow::Result<()> {
        self.probe.starts.fetch_add(1, Ordering::SeqCst);
        if self.properties.get("failStart") == Some(&Value::Bool(true)) {
            anyhow::bail!("source fixture start failure");
        }
        self.inner.start().await
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.inner.stop().await
    }

    async fn status(&self) -> ComponentStatus {
        self.inner.status().await
    }

    async fn subscribe(
        &self,
        settings: drasi_lib::config::SourceSubscriptionSettings,
    ) -> anyhow::Result<SubscriptionResponse> {
        self.inner.subscribe(settings).await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct SourceDescriptor(Arc<LifecycleProbe>);

#[async_trait]
impl SourcePluginDescriptor for SourceDescriptor {
    fn kind(&self) -> &str {
        "mock"
    }

    fn config_version(&self) -> &str {
        "1.0.0"
    }

    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.to_string()
    }

    fn config_schema_name(&self) -> &str {
        "ProbedSourceConfig"
    }

    async fn create_source(
        &self,
        id: &str,
        config: &Value,
        auto_start: bool,
    ) -> anyhow::Result<Box<dyn Source>> {
        if config["failFactory"] == true {
            anyhow::bail!("source fixture factory failure");
        }
        Ok(Box::new(ProbedSource {
            inner: MockSource::new(id),
            auto_start,
            properties: serde_json::from_value(config.clone())?,
            probe: self.0.clone(),
        }))
    }
}

struct ProbedReaction {
    inner: MockReaction,
    auto_start: bool,
    properties: HashMap<String, Value>,
    probe: Arc<LifecycleProbe>,
}

#[async_trait]
impl Reaction for ProbedReaction {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn type_name(&self) -> &str {
        self.inner.type_name()
    }

    fn properties(&self) -> HashMap<String, Value> {
        self.properties.clone()
    }

    fn query_ids(&self) -> Vec<String> {
        self.inner.query_ids()
    }

    fn auto_start(&self) -> bool {
        self.auto_start
    }

    async fn initialize(&self, context: ReactionRuntimeContext) {
        if self.properties.get("blockInitialization") == Some(&Value::Bool(true)) {
            self.probe.initialization_gate.notified().await;
        }
        self.inner.initialize(context).await;
    }

    async fn start(&self) -> anyhow::Result<()> {
        self.probe.starts.fetch_add(1, Ordering::SeqCst);
        if self.properties.get("failStart") == Some(&Value::Bool(true)) {
            anyhow::bail!("reaction fixture start failure");
        }
        self.inner.start().await
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.inner.stop().await
    }

    async fn status(&self) -> ComponentStatus {
        self.inner.status().await
    }

    async fn enqueue_query_result(&self, result: QueryResult) -> anyhow::Result<()> {
        self.inner.enqueue_query_result(result).await
    }
}

struct ReactionDescriptor(Arc<LifecycleProbe>);

#[async_trait]
impl ReactionPluginDescriptor for ReactionDescriptor {
    fn kind(&self) -> &str {
        "log"
    }

    fn config_version(&self) -> &str {
        "1.0.0"
    }

    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.to_string()
    }

    fn config_schema_name(&self) -> &str {
        "ProbedReactionConfig"
    }

    async fn create_reaction(
        &self,
        id: &str,
        query_ids: Vec<String>,
        config: &Value,
        auto_start: bool,
    ) -> anyhow::Result<Box<dyn Reaction>> {
        if config["failFactory"] == true {
            anyhow::bail!("reaction fixture factory failure");
        }
        Ok(Box::new(ProbedReaction {
            inner: MockReaction::new(id, query_ids),
            auto_start,
            properties: serde_json::from_value(config.clone())?,
            probe: self.0.clone(),
        }))
    }
}

struct Harness {
    app: Router,
    core: Arc<DrasiLib>,
    registry: InstanceRegistry,
    source_probe: Arc<LifecycleProbe>,
    reaction_probe: Arc<LifecycleProbe>,
    config_path: PathBuf,
    _directory: TempDir,
}

impl Harness {
    async fn new(mode: &str, running: bool) -> Self {
        let original: DrasiServerConfig = serde_json::from_value(json!({
            "id": "native",
            "executionMode": mode,
        }))
        .unwrap();
        let core = Arc::new(
            DrasiLib::builder()
                .with_id("native")
                .with_execution_mode(original.execution_mode.into())
                .build()
                .await
                .unwrap(),
        );
        if running {
            core.start().await.unwrap();
        }
        let registry = InstanceRegistry::new();
        registry.add("native".into(), core.clone()).await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("server.yaml");
        let persistence = Arc::new(ConfigPersistence::new(
            config_path.clone(),
            registry.clone(),
            "127.0.0.1".into(),
            8080,
            "info".into(),
            true,
            IndexMap::new(),
            IndexMap::new(),
            None,
            &original,
        ));
        let source_probe = Arc::new(LifecycleProbe::default());
        let reaction_probe = Arc::new(LifecycleProbe::default());
        let mut plugins = PluginRegistry::new();
        drasi_server::register_core_plugins(&mut plugins);
        plugins.register_source_with_package_version(
            Arc::new(SourceDescriptor(source_probe.clone())),
            SOURCE_PLUGIN,
            Some(SOURCE_PACKAGE_VERSION),
        );
        plugins.register_reaction_with_package_version(
            Arc::new(ReactionDescriptor(reaction_probe.clone())),
            REACTION_PLUGIN,
            Some(REACTION_PACKAGE_VERSION),
        );
        let app = Router::new().nest(
            "/api/v1",
            drasi_server::api::build_v1_router(
                registry.clone(),
                Arc::new(false),
                Some(persistence),
                Arc::new(RwLock::new(plugins)),
                None,
            ),
        );
        Self {
            app,
            core,
            registry,
            source_probe,
            reaction_probe,
            config_path,
            _directory: directory,
        }
    }

    fn saved(&self) -> DrasiServerConfig {
        drasi_server::load_config_file(&self.config_path).unwrap()
    }
}

async fn request(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = tokio::time::timeout(
        DEADLINE,
        app.clone().oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        ),
    )
    .await
    .expect("API request must complete")
    .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn assert_status(harness: &Harness, resource: &str, id: &str, expected: &str) -> Value {
    let (status, body) = request(
        &harness.app,
        "GET",
        &format!("/api/v1/{resource}/{id}?view=full"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], expected, "{body}");
    body
}

#[tokio::test]
async fn native_post_and_put_acknowledge_added_nodes_before_initialization_and_persist_failures() {
    let harness = Harness::new("computationGraph", true).await;
    assert_eq!(
        harness.core.execution_mode(),
        ExecutionMode::ComputationGraph
    );
    for (resource, kind, method, probe) in [
        ("sources", "mock", "POST", &harness.source_probe),
        ("sources", "mock", "PUT", &harness.source_probe),
        ("reactions", "log", "POST", &harness.reaction_probe),
        ("reactions", "log", "PUT", &harness.reaction_probe),
    ] {
        let id = format!("{resource}-{method}");
        let path = if method == "PUT" {
            format!("/api/v1/{resource}/{id}")
        } else {
            format!("/api/v1/{resource}")
        };
        let mut config = json!({
            "kind": kind, "id": id, "autoStart": true,
            "blockInitialization": true, "failStart": true,
        });
        if resource == "reactions" {
            config["queries"] = json!([]);
        }
        let before = probe.starts.load(Ordering::SeqCst);
        let (status, body) = request(&harness.app, method, &path, config).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["success"], true);
        assert_eq!(probe.starts.load(Ordering::SeqCst), before);
        assert_status(&harness, resource, &id, "Added").await;

        let persisted = serde_json::to_value(harness.saved()).unwrap();
        assert!(
            persisted[resource]
                .as_array()
                .unwrap()
                .iter()
                .any(|config| config["id"] == id && config["failStart"] == true),
            "pending node configuration must be saved: {persisted}"
        );

        let handle = harness.core.computation_component(&id).unwrap();
        probe.initialization_gate.notify_one();
        let error = tokio::time::timeout(DEADLINE, handle.wait_started())
            .await
            .unwrap()
            .unwrap_err();
        assert!(
            error.to_string().contains("fixture start failure"),
            "{error}"
        );
        assert_eq!(probe.starts.load(Ordering::SeqCst), before + 1);
        let body = assert_status(&harness, resource, &id, "Error").await;
        assert!(body["data"]["error_message"]
            .as_str()
            .unwrap()
            .contains("fixture start failure"));

        let (status, body) = request(
            &harness.app,
            "POST",
            &format!("/api/v1/{resource}/{id}/start"),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert_eq!(probe.starts.load(Ordering::SeqCst), before + 2);
        assert_status(&harness, resource, &id, "Error").await;
    }
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn native_create_does_not_autostart_a_stopped_instance_but_explicit_start_waits() {
    let harness = Harness::new("computationGraph", false).await;
    for (resource, kind, probe) in [
        ("sources", "mock", &harness.source_probe),
        ("reactions", "log", &harness.reaction_probe),
    ] {
        let mut config = json!({"kind": kind, "id": resource, "autoStart": true});
        if resource == "reactions" {
            config["queries"] = json!([]);
        }
        let (status, body) =
            request(&harness.app, "POST", &format!("/api/v1/{resource}"), config).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let handle = harness.core.computation_component(resource).unwrap();
        tokio::time::timeout(DEADLINE, handle.wait_created())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(probe.starts.load(Ordering::SeqCst), 0);
        assert_status(&harness, resource, resource, "Added").await;

        let (status, body) = request(
            &harness.app,
            "POST",
            &format!("/api/v1/{resource}/{resource}/start"),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(probe.starts.load(Ordering::SeqCst), 1);
        assert_status(&harness, resource, resource, "Running").await;
        handle.stop().await.unwrap();
    }
}

#[tokio::test]
async fn native_query_creation_keeps_invalid_configuration_and_start_reports_failure() {
    let harness = Harness::new("computationGraph", true).await;
    let config = json!({
        "id": "invalid-query", "query": INVALID_QUERY, "queryLanguage": "Cypher",
        "sources": [{"sourceId": "missing-source"}], "autoStart": true,
    });
    let (status, body) = request(&harness.app, "POST", "/api/v1/queries", config.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let handle = harness.core.computation_component("invalid-query").unwrap();
    assert!(tokio::time::timeout(DEADLINE, handle.wait_created())
        .await
        .unwrap()
        .is_err());
    let body = assert_status(&harness, "queries", "invalid-query", "Error").await;
    assert_eq!(body["data"]["config"]["query"], INVALID_QUERY);
    assert!(body["data"]["error_message"].is_string());
    let saved = harness.saved();
    assert_eq!(saved.queries[0].query, INVALID_QUERY);
    assert!(saved.queries[0].auto_start);

    let (status, body) = request(&harness.app, "POST", "/api/v1/queries", config).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = request(
        &harness.app,
        "POST",
        "/api/v1/queries/invalid-query/start",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_status(&harness, "queries", "invalid-query", "Error").await;
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn native_factory_failures_remain_create_errors_without_nodes() {
    let harness = Harness::new("computationGraph", true).await;
    for (resource, kind) in [("sources", "mock"), ("reactions", "log")] {
        let mut config = json!({
            "id": "factory-failure", "kind": kind, "autoStart": true, "failFactory": true,
        });
        if resource == "reactions" {
            config["queries"] = json!([]);
        }
        let (status, body) =
            request(&harness.app, "POST", &format!("/api/v1/{resource}"), config).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert!(body["message"]
            .as_str()
            .unwrap()
            .contains("factory failure"));
        let (status, body) = request(
            &harness.app,
            "GET",
            &format!("/api/v1/{resource}/factory-failure"),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }
    assert!(!harness.config_path.exists());
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn native_query_attach_starts_one_temporary_reaction_and_streams_results() {
    let harness = Harness::new("computationGraph", true).await;
    let (source, input) = ApplicationSource::new(
        "input",
        ApplicationSourceConfig {
            properties: Default::default(),
            durability: None,
        },
    )
    .unwrap();
    let source = harness.core.add_source_with_handle(source).await.unwrap();
    tokio::time::timeout(DEADLINE, source.wait_started())
        .await
        .unwrap()
        .unwrap();
    let query = harness
        .core
        .add_query_with_handle(
            Query::cypher("selected")
                .query("MATCH (n:Item) RETURN n.value AS value")
                .from_source("input")
                .enable_bootstrap(false)
                .auto_start(true)
                .build(),
        )
        .await
        .unwrap();
    tokio::time::timeout(DEADLINE, query.wait_started())
        .await
        .unwrap()
        .unwrap();
    let response = tokio::time::timeout(
        DEADLINE,
        harness.app.clone().oneshot(
            Request::builder()
                .uri("/api/v1/queries/selected/attach")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let reactions = harness.core.list_reactions().await.unwrap();
    assert_eq!(reactions.len(), 1);
    assert!(reactions[0].0.starts_with("__attach_selected_"));
    assert_eq!(reactions[0].1, ComponentStatus::Running);
    let mut stream = response.into_body().into_data_stream();
    input
        .send_node_insert(
            "item",
            vec!["Item"],
            PropertyMapBuilder::new().with_integer("value", 42).build(),
        )
        .await
        .unwrap();
    let chunk = tokio::time::timeout(DEADLINE, stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(std::str::from_utf8(&chunk)
        .unwrap()
        .contains("\"value\":42"));
    drop(stream);
    tokio::time::timeout(DEADLINE, async {
        while !harness.core.list_reactions().await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("temporary attach reaction is removed when the stream closes");
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn native_solution_preserves_source_and_reaction_plugin_version_nodes() {
    let harness = Harness::new("computationGraph", true).await;
    let yaml = serde_yaml::to_string(&json!({
        "name": "Plugin metadata",
        "sources": [{"kind": "mock", "id": "source", "autoStart": false}],
        "reactions": [{"kind": "log", "id": "reaction", "queries": [], "autoStart": false}],
    }))
    .unwrap();
    let (status, body) = request(
        &harness.app,
        "POST",
        "/api/v1/instances/native/solutions",
        json!({"yaml": yaml}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["success"], true, "{body}");
    let topology = harness
        .core
        .computation_control()
        .unwrap()
        .inspector()
        .topology();
    for (id, plugin_id, package_version) in [
        ("source", SOURCE_PLUGIN, SOURCE_PACKAGE_VERSION),
        ("reaction", REACTION_PLUGIN, REACTION_PACKAGE_VERSION),
    ] {
        let handle = harness.core.computation_component(id).unwrap();
        let identity = PluginIdentity {
            id: Arc::from(plugin_id),
            version: Arc::from(package_version),
        };
        let Some(GraphEntity::Plugin(plugin)) =
            topology.nodes.get(&GraphEntityId::Plugin(identity))
        else {
            panic!("Missing plugin/version node for {id}: {topology:?}");
        };
        assert!(plugin.dependent_components.contains(handle.id()));
        assert!(!topology
            .nodes
            .contains_key(&GraphEntityId::Plugin(PluginIdentity {
                id: Arc::from(plugin_id),
                version: Arc::from("1.0.0"),
            })));
    }
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn native_solution_reports_core_construction_deadline_and_retains_failed_node() {
    let harness = Harness::new("computationGraph", true).await;
    let yaml = serde_yaml::to_string(&json!({
        "name": "Deferred creation",
        "sources": [{
            "kind": "mock", "id": "deferred", "autoStart": false,
            "blockInitialization": true,
        }],
    }))
    .unwrap();
    let mut deployment = Box::pin(
        harness.app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/instances/native/solutions")
                .header("content-type", "application/json")
                .body(Body::from(json!({"yaml": yaml}).to_string()))
                .unwrap(),
        ),
    );
    tokio::time::pause();
    let started = tokio::time::Instant::now();
    tokio::time::timeout(DEADLINE, async {
        loop {
            assert!(futures_util::poll!(&mut deployment).is_pending());
            if harness.source_probe.initializations.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("source initialization starts while deployment waits for its health");
    assert!(futures_util::poll!(&mut deployment).is_pending());
    let handle = harness.core.computation_component("deferred").unwrap();
    assert_eq!(
        handle.observed().unwrap().realization,
        RealizationState::Creating
    );
    tokio::time::advance(Duration::from_secs(4)).await;
    assert!(futures_util::poll!(&mut deployment).is_pending());
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::time::resume();

    let response = tokio::time::timeout(DEADLINE, deployment)
        .await
        .expect("the core construction deadline must finish deployment")
        .unwrap();
    assert!(started.elapsed() <= Duration::from_secs(30));
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["data"]["success"], false, "{body}");
    assert_eq!(body["data"]["sourcesCreated"], json!(["deferred"]));
    let errors = body["data"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{body}");
    assert_eq!(errors[0]["phase"], "creation");
    assert_eq!(errors[0]["componentId"], "deferred");
    let message = errors[0]["message"].as_str().unwrap();
    assert!(message.contains("Node was added"), "{message}");
    assert!(
        message.contains(&GraphError::ReconciliationTimeout.to_string()),
        "{message}"
    );
    let saved = harness.saved();
    assert_eq!(saved.sources.len(), 1);
    assert_eq!(saved.sources[0].id, "deferred");
    assert_eq!(saved.sources[0].config["blockInitialization"], true);
    assert!(!saved.sources[0].auto_start);

    let observed = handle.observed().unwrap();
    assert_eq!(observed.realization, RealizationState::CreationFailed);
    assert!(matches!(
        observed.failure.unwrap().cause.as_ref(),
        GraphError::ReconciliationTimeout
    ));
    assert_status(&harness, "sources", "deferred", "Error").await;
    assert_eq!(harness.source_probe.starts.load(Ordering::SeqCst), 0);
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn native_solution_retains_committed_nodes_and_reports_creation_and_start_health() {
    let harness = Harness::new("computationGraph", true).await;
    let yaml = serde_yaml::to_string(&json!({
        "name": "Native partial deployment",
        "sources": [
            {"kind": "mock", "id": "good-source", "autoStart": true},
            {"kind": "mock", "id": "factory-source", "failFactory": true, "autoStart": true},
            {"kind": "mock", "id": "failed-source", "failStart": true, "autoStart": true},
        ],
        "queries": [
            {"id": "invalid-query", "query": INVALID_QUERY,
             "sources": [{"sourceId": "good-source"}], "autoStart": true},
            {"id": "good-query", "query": "MATCH (n) RETURN n",
             "sources": [{"sourceId": "good-source"}], "autoStart": false},
        ],
        "reactions": [
            {"kind": "log", "id": "failed-reaction", "queries": [],
             "failStart": true, "autoStart": true},
            {"kind": "log", "id": "factory-reaction", "queries": [],
             "failFactory": true, "autoStart": true},
        ],
    }))
    .unwrap();
    let (status, body) = request(
        &harness.app,
        "POST",
        "/api/v1/instances/native/solutions",
        json!({"yaml": yaml}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["success"], false, "{body}");
    assert_eq!(
        data["sourcesCreated"],
        json!(["good-source", "failed-source"])
    );
    assert_eq!(
        data["queriesCreated"],
        json!(["invalid-query", "good-query"])
    );
    assert_eq!(data["reactionsCreated"], json!(["failed-reaction"]));
    assert_eq!(data["componentsStarted"], json!(["source:good-source"]));
    let errors = data["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 5, "{body}");
    for (id, phase, fragment) in [
        ("factory-source", "creation", "factory failure"),
        ("factory-reaction", "creation", "factory failure"),
        ("invalid-query", "creation", "invalid-query"),
        ("failed-source", "start", "fixture start failure"),
        ("failed-reaction", "start", "fixture start failure"),
    ] {
        assert!(
            errors.iter().any(|error| error["componentId"] == id
                && error["phase"] == phase
                && error["message"].as_str().unwrap().contains(fragment)),
            "{id}: {body}"
        );
    }
    assert_eq!(harness.source_probe.starts.load(Ordering::SeqCst), 2);
    assert_eq!(harness.reaction_probe.starts.load(Ordering::SeqCst), 1);
    assert_status(&harness, "sources", "good-source", "Running").await;
    assert_status(&harness, "sources", "failed-source", "Error").await;
    assert_status(&harness, "queries", "invalid-query", "Error").await;
    assert_status(&harness, "reactions", "failed-reaction", "Error").await;
    let saved = harness.saved();
    assert_eq!(saved.sources.len(), 2);
    assert_eq!(saved.queries.len(), 2);
    assert_eq!(saved.reactions.len(), 1);
    assert!(saved
        .queries
        .iter()
        .any(|query| query.query == INVALID_QUERY));
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn native_clone_keeps_failed_nodes_and_prior_additions_after_a_factory_failure() {
    let harness = Harness::new("computationGraph", true).await;
    let origin = Arc::new(
        DrasiLib::builder()
            .with_id("origin")
            .with_execution_mode(ExecutionMode::ComputationGraph)
            .build()
            .await
            .unwrap(),
    );
    origin
        .add_source(MockSource::new("clone-source"))
        .await
        .unwrap();
    for (id, query) in [
        ("invalid-query", INVALID_QUERY),
        ("good-query", "MATCH (n) RETURN n"),
    ] {
        origin
            .add_query(
                Query::cypher(id)
                    .query(query)
                    .from_source("clone-source")
                    .auto_start(false)
                    .build(),
            )
            .await
            .unwrap();
    }
    origin
        .add_reaction(ProbedReaction {
            inner: MockReaction::new("factory-reaction", vec![]),
            auto_start: false,
            properties: HashMap::from([("failFactory".into(), Value::Bool(true))]),
            probe: Arc::new(LifecycleProbe::default()),
        })
        .await
        .unwrap();
    harness
        .registry
        .add("origin".into(), origin.clone())
        .await
        .unwrap();
    let (status, body) = request(
        &harness.app,
        "POST",
        "/api/v1/instances/native/clone",
        json!({"sourceInstanceId": "origin"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["success"], false, "{body}");
    assert_eq!(data["sourcesCreated"], json!(["clone-source"]));
    assert_eq!(data["queriesCreated"].as_array().unwrap().len(), 2);
    assert_eq!(data["reactionsCreated"], json!([]));
    let errors = data["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2, "{body}");
    assert!(errors
        .iter()
        .any(|error| error.as_str().unwrap().contains("invalid-query")));
    assert!(errors
        .iter()
        .any(|error| error.as_str().unwrap().contains("fixture factory failure")));
    assert_status(&harness, "sources", "clone-source", "Added").await;
    assert_status(&harness, "queries", "invalid-query", "Error").await;
    assert_eq!(harness.source_probe.starts.load(Ordering::SeqCst), 0);
    assert_eq!(harness.reaction_probe.starts.load(Ordering::SeqCst), 0);
    let saved = harness
        .saved()
        .resolved_instances(&DtoMapper::new())
        .unwrap()
        .into_iter()
        .find(|instance| instance.id == "native")
        .unwrap();
    assert_eq!(saved.sources.len(), 1);
    assert_eq!(saved.queries.len(), 2);
    assert!(saved
        .queries
        .iter()
        .any(|query| query.query == INVALID_QUERY));
    harness.core.stop().await.unwrap();
}

#[tokio::test]
async fn legacy_start_failure_is_still_a_create_http_failure() {
    let harness = Harness::new("componentGraph", true).await;
    for (resource, kind) in [("sources", "mock"), ("reactions", "log")] {
        let mut config = json!({
            "id": format!("{resource}-legacy-failure"),
            "kind": kind, "autoStart": true, "failStart": true,
        });
        if resource == "reactions" {
            config["queries"] = json!([]);
        }
        let (status, body) =
            request(&harness.app, "POST", &format!("/api/v1/{resource}"), config).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert!(body["message"]
            .as_str()
            .unwrap()
            .contains("fixture start failure"));
    }
    assert_eq!(harness.source_probe.starts.load(Ordering::SeqCst), 1);
    assert_eq!(harness.reaction_probe.starts.load(Ordering::SeqCst), 1);
    assert!(!harness.config_path.exists());
    harness.core.stop().await.unwrap();
}
