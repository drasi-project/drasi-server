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

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_lib::DrasiLib;
use drasi_reaction_application::ApplicationReaction;
use drasi_server::{
    api::mappings::DtoMapper,
    config::{
        loader::{from_json_str, from_yaml_str},
        DrasiServerConfig,
    },
    instance_registry::InstanceRegistry,
    persistence::ConfigPersistence,
    plugin_registry::PluginRegistry,
    DrasiServerBuilder,
};
use drasi_source_application::{ApplicationSource, ApplicationSourceConfig, PropertyMapBuilder};
use indexmap::IndexMap;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tower::ServiceExt;

fn router(registry: InstanceRegistry, persistence: Option<Arc<ConfigPersistence>>) -> Router {
    let mut plugins = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugins);
    Router::new().nest(
        "/api/v1",
        drasi_server::api::build_v1_router(
            registry,
            Arc::new(false),
            persistence,
            Arc::new(RwLock::new(plugins)),
            None,
        ),
    )
}

async fn request(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn configuration_has_no_runtime_selector_and_preserves_instance_settings() {
    let config: DrasiServerConfig = serde_yaml::from_str("id: default").unwrap();
    assert!(!serde_yaml::to_string(&config)
        .unwrap()
        .contains("executionMode"));

    let config: DrasiServerConfig = serde_yaml::from_str(
        "instances:\n  - id: analytics\n    defaultPriorityQueueCapacity: 64\n  - id: monitoring\n    defaultDispatchBufferCapacity: 32\n",
    )
    .unwrap();
    let resolved = config.resolved_instances(&DtoMapper::new()).unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].id, "analytics");
    assert_eq!(resolved[0].default_priority_queue_capacity, Some(64));
    assert_eq!(resolved[1].id, "monitoring");
    assert_eq!(resolved[1].default_dispatch_buffer_capacity, Some(32));
    assert!(!serde_json::to_string(&config)
        .unwrap()
        .contains("executionMode"));
}

#[test]
fn removed_config_selectors_are_rejected_without_echoing_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("server.yaml");
    for value in [
        json!("componentGraph"),
        json!("computationGraph"),
        json!("native"),
        Value::Null,
        json!(42),
        json!({"password": "selector-secret-do-not-echo"}),
    ] {
        for config in [
            json!({"id": "server", "executionMode": value}),
            json!({"instances": [{"id": "nested", "executionMode": value}]}),
        ] {
            let json = config.to_string();
            let yaml = serde_yaml::to_string(&config).unwrap();
            assert!(serde_json::from_str::<DrasiServerConfig>(&json).is_err());
            assert!(serde_yaml::from_str::<DrasiServerConfig>(&yaml).is_err());
            std::fs::write(&path, &yaml).unwrap();
            for error in [
                from_json_str::<DrasiServerConfig>(&json)
                    .unwrap_err()
                    .to_string(),
                from_yaml_str::<DrasiServerConfig>(&yaml)
                    .unwrap_err()
                    .to_string(),
                drasi_server::load_config_file(&path)
                    .unwrap_err()
                    .to_string(),
            ] {
                assert!(
                    error.contains("ComputationGraph is the only runtime"),
                    "{error}"
                );
                assert!(error.contains("remove executionMode"), "{error}");
                assert!(!error.contains("selector-secret-do-not-echo"), "{error}");
            }
        }
    }
}

#[tokio::test]
async fn builder_uses_graph_for_all_construction_paths() {
    let core = DrasiServerBuilder::new().build_core().await.unwrap();
    core.computation_control()
        .expect("default builder creates the graph");
    core.shutdown().await.unwrap();

    let handles = DrasiServerBuilder::new()
        .with_id("first")
        .add_instance_builder(DrasiLib::builder().with_id("second"))
        .add_instance_builder(DrasiLib::builder().with_id("third"))
        .build_with_handles()
        .await
        .unwrap();
    assert_eq!(handles.servers.len(), 3);
    for (id, core) in &handles.servers {
        assert_eq!(core.get_current_config().await.unwrap().id, *id);
        core.computation_control()
            .expect("every instance has a graph");
        assert!(core.is_running().await);
        core.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn api_creation_and_default_routes_preserve_instance_isolation() {
    let registry = InstanceRegistry::new();
    registry
        .add(
            "existing".into(),
            Arc::new(
                DrasiLib::builder()
                    .with_id("existing")
                    .build()
                    .await
                    .unwrap(),
            ),
        )
        .await
        .unwrap();
    let app = router(registry.clone(), None);
    let (status, body) = request(&app, "POST", "/api/v1/instances", json!({"id":"created"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let created = registry.get("created").await.unwrap();
    created.computation_control().unwrap();
    let (status, body) = request(
        &app,
        "GET",
        "/api/v1/instances/created/runtime",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"],
        json!({"instanceId":"created", "runtime":"computationGraph", "running":true})
    );
    assert_eq!(registry.get_default().await.unwrap().0, "existing");

    let removed = registry.remove("existing").await.unwrap();
    removed.shutdown().await.unwrap();
    assert_eq!(registry.get_default().await.unwrap().0, "created");
    let (status, body) = request(&app, "POST", "/api/v1/instances", json!({"id":"next"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(registry.list_ids().await, ["created", "next"]);
    for (_, core) in registry.list().await {
        core.computation_control().unwrap();
        core.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn api_rejects_removed_selectors_in_json_and_yaml_without_leaking_secrets() {
    let registry = InstanceRegistry::new();
    let app = router(registry.clone(), None);
    for value in [
        json!("componentGraph"),
        json!("computationGraph"),
        Value::Null,
        json!({"password": "selector-secret-do-not-echo"}),
    ] {
        for content_type in ["application/json", "application/yaml"] {
            let body = json!({"id":"rejected", "executionMode":value});
            let text = if content_type == "application/yaml" {
                serde_yaml::to_string(&body).unwrap()
            } else {
                body.to_string()
            };
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/instances")
                        .header("content-type", content_type)
                        .body(Body::from(text))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["code"], "INVALID_REQUEST");
            let message = body["message"].as_str().unwrap();
            assert!(
                message.contains("ComputationGraph is the only runtime"),
                "{body}"
            );
            assert!(message.contains("remove executionMode"), "{body}");
            assert!(!body.to_string().contains("selector-secret-do-not-echo"));
            assert!(registry.is_empty().await);
        }
    }
}

#[tokio::test]
async fn clone_and_deployment_reject_selectors_before_creating_nodes() {
    let registry = InstanceRegistry::new();
    let core = Arc::new(DrasiLib::builder().with_id("target").build().await.unwrap());
    registry.add("target".into(), core.clone()).await.unwrap();
    let app = router(registry, None);
    for value in [
        json!("componentGraph"),
        json!("computationGraph"),
        json!({"password": "selector-secret-do-not-echo"}),
    ] {
        for (path, body) in [
            (
                "/api/v1/instances/target/clone",
                json!({"sourceInstanceId":"target", "executionMode":value}),
            ),
            (
                "/api/v1/instances/target/solutions",
                json!({"yaml":"name: empty", "executionMode":value}),
            ),
            (
                "/api/v1/instances/target/solutions",
                json!({"yaml":serde_yaml::to_string(&json!({
                    "name":"obsolete-selector", "executionMode":value
                })).unwrap()}),
            ),
        ] {
            let (status, body) = request(&app, "POST", path, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(body["code"], "INVALID_REQUEST");
            assert!(
                body["message"]
                    .as_str()
                    .unwrap()
                    .contains("remove executionMode"),
                "{body}"
            );
            assert!(!body.to_string().contains("selector-secret-do-not-echo"));
        }
    }
    let snapshot = core.snapshot_configuration().await.unwrap();
    assert_eq!(
        snapshot
            .sources
            .iter()
            .map(|source| source.id.as_str())
            .collect::<Vec<_>>(),
        ["__component_graph__"]
    );
    assert!(snapshot.queries.is_empty());
    assert!(snapshot.reactions.is_empty());
    core.shutdown().await.unwrap();
}

#[test]
fn obsolete_cli_flags_are_rejected_for_every_command() {
    for command in [None, Some("run"), Some("validate")] {
        for value in ["component-graph", "computation-graph"] {
            let mut process = std::process::Command::new(env!("CARGO_BIN_EXE_drasi-server"));
            if let Some(command) = command {
                process.arg(command);
            }
            let output = process.args(["--execution-mode", value]).output().unwrap();
            assert!(!output.status.success());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(
                error.contains("unexpected argument '--execution-mode'"),
                "{error}"
            );
        }
    }
}

#[test]
fn openapi_no_longer_advertises_runtime_selection() {
    use utoipa::OpenApi;
    let schema = serde_json::to_value(drasi_server::api::v1::openapi::ApiDocV1::openapi()).unwrap();
    let schemas = schema["components"]["schemas"].as_object().unwrap();
    assert!(!schemas.contains_key("ExecutionModeConfig"));
    for name in [
        "DrasiServerConfig",
        "DrasiLibInstanceConfig",
        "CreateInstanceRequest",
        "InstanceRuntimeInfo",
    ] {
        assert!(!schemas[name]["properties"]
            .as_object()
            .unwrap()
            .contains_key("executionMode"));
    }
}

#[tokio::test]
async fn ordinary_api_query_and_reaction_execute_on_the_default_graph() {
    let (source, input) = ApplicationSource::new(
        "input",
        ApplicationSourceConfig {
            properties: Default::default(),
            durability: None,
        },
    )
    .unwrap();
    let core = Arc::new(
        DrasiServerBuilder::new()
            .with_id(format!("api-{}", uuid::Uuid::new_v4()))
            .with_source(source)
            .build_core()
            .await
            .unwrap(),
    );
    core.computation_control().unwrap();
    core.start().await.unwrap();
    let registry = InstanceRegistry::new();
    registry.add("test".into(), core.clone()).await.unwrap();
    let app = router(registry, None);
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/queries",
        json!({
            "id":"selected", "query":"MATCH (n:Item) WHERE n.value > 10 RETURN n.value AS value",
            "queryLanguage":"Cypher", "sources":[{"sourceId":"input"}],
            "autoStart":true, "enableBootstrap":false,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    tokio::time::timeout(
        Duration::from_secs(10),
        core.computation_component("selected")
            .unwrap()
            .wait_started(),
    )
    .await
    .unwrap()
    .unwrap();
    let (reaction, output) = ApplicationReaction::new("output", vec!["selected".into()]);
    let mut receiver = output.take_receiver().await.unwrap();
    core.add_reaction(reaction).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(10),
        core.computation_component("output").unwrap().wait_started(),
    )
    .await
    .unwrap()
    .unwrap();
    input
        .send_node_insert(
            "one",
            vec!["Item"],
            PropertyMapBuilder::new().with_integer("value", 42).build(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let result = receiver.recv().await.expect("reaction output");
            assert_eq!(result.query_id, "selected");
            if result.results.iter().any(|diff| {
                matches!(
                    diff,
                    drasi_lib::channels::ResultDiff::Add { data, .. }
                        if data == &json!({"value": 42})
                )
            }) {
                break;
            }
        }
    })
    .await
    .expect("query change reaches ordinary reaction through the graph");
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (status, body) =
                request(&app, "GET", "/api/v1/queries/selected/results", Value::Null).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            if body["data"] == json!([{"value":42}]) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("ordinary query results return native data");
    let (status, body) = request(&app, "GET", "/api/v1/instances/test/runtime", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"],
        json!({"instanceId":"test", "runtime":"computationGraph", "running":true})
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn persistence_and_reload_preserve_single_and_multi_instance_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("server.yaml");
    let core = Arc::new(
        DrasiLib::builder()
            .with_id("existing")
            .build()
            .await
            .unwrap(),
    );
    core.start().await.unwrap();
    let registry = InstanceRegistry::new();
    registry.add("existing".into(), core.clone()).await.unwrap();
    let original: DrasiServerConfig = serde_json::from_value(json!({
        "id":"server",
        "enableUi":false,
        "verifyPlugins":false,
        "corsAllowedOrigins":["https://dashboard.example.com"],
        "instances":[{
            "id":"existing",
            "defaultPriorityQueueCapacity":64,
            "defaultDispatchBufferCapacity":32
        }],
    }))
    .unwrap();
    let persistence = Arc::new(ConfigPersistence::new(
        path.clone(),
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
    persistence
        .register_instance(original.instances[0].clone())
        .await;
    persistence.save().await.unwrap();
    let saved = drasi_server::load_config_file(&path).unwrap();
    assert!(saved.instances.is_empty(), "one instance uses root fields");
    assert!(!saved.enable_ui);
    assert!(!saved.verify_plugins);
    assert_eq!(saved.cors_allowed_origins, original.cors_allowed_origins);
    let resolved = saved.resolved_instances(&DtoMapper::new()).unwrap();
    assert_eq!(resolved[0].id, "existing");
    assert_eq!(resolved[0].default_priority_queue_capacity, Some(64));
    assert_eq!(resolved[0].default_dispatch_buffer_capacity, Some(32));
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains("executionMode"));

    let app = router(registry.clone(), Some(persistence));
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/instances",
        json!({
            "id":"new",
            "defaultPriorityQueueCapacity":128,
            "defaultDispatchBufferCapacity":16
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let saved = drasi_server::load_config_file(&path).unwrap();
    assert_eq!(saved.instances.len(), 2);
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains("executionMode"));
    let resolved = saved.resolved_instances(&DtoMapper::new()).unwrap();
    assert_eq!(resolved[0].id, "existing");
    assert_eq!(resolved[1].id, "new");
    assert_eq!(resolved[0].default_priority_queue_capacity, Some(64));
    assert_eq!(resolved[1].default_priority_queue_capacity, Some(128));
    assert_eq!(resolved[1].default_dispatch_buffer_capacity, Some(16));
    let plugins = tmp.path().join("plugins");
    std::fs::create_dir(&plugins).unwrap();
    drasi_server::DrasiServer::new(path, 8080, plugins, true, false)
        .await
        .expect("saved configuration reloads without selectors");
    for (_, instance) in registry.list().await {
        instance.shutdown().await.unwrap();
    }
}

struct ServerProcess(std::process::Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            if let Err(error) = self.0.kill() {
                log::warn!("Could not stop test server {}: {error}", self.0.id());
            }
        }
        if let Err(error) = self.0.wait() {
            log::warn!("Could not reap test server {}: {error}", self.0.id());
        }
    }
}

#[tokio::test]
#[ignore = "requires matching HTTP source and log reaction cdylibs in DRASI_NATIVE_TEST_PLUGINS"]
async fn native_binary_loads_plugins_and_processes_http_events() {
    let plugins = std::fs::canonicalize(
        std::env::var("DRASI_NATIVE_TEST_PLUGINS").expect("matching plugin directory"),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("server.json");
    let log_path = directory.path().join("server.log");
    let api_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let source_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let api_port = api_listener.local_addr().unwrap().port();
    let source_port = source_listener.local_addr().unwrap().port();
    std::fs::write(
            &config,
            serde_json::to_vec(&json!({
                "id":"native-plugin-test", "host":"127.0.0.1", "port":api_port,
                "persistConfig":false,
                "enableUi":false, "autoInstallPlugins":false,
                "sources":[{"kind":"http", "id":"input", "host":"127.0.0.1", "port":source_port}],
                "queries":[{
                    "id":"selected", "queryLanguage":"Cypher", "autoStart":true,
                    "enableBootstrap":false, "sources":[{"sourceId":"input"}],
                    "query":"MATCH (n:Item) WHERE n.value > 10 RETURN n.marker AS marker, n.value AS value"
                }],
                "reactions":[{"kind":"log", "id":"output", "queries":["selected"]}]
            }))
            .unwrap(),
        )
        .unwrap();
    drop((api_listener, source_listener));
    let log = std::fs::File::create(&log_path).unwrap();
    let mut server = ServerProcess(
        std::process::Command::new(env!("CARGO_BIN_EXE_drasi-server"))
            .current_dir(directory.path())
            .arg("--config")
            .arg(&config)
            .arg("--plugins-dir")
            .arg(plugins)
            .args([
                "--disable-ui",
                // The caller supplies independently verified local build artifacts.
                "--skip-verification",
            ])
            .env("RUST_LOG", "info")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let base = format!("http://127.0.0.1:{api_port}/api/v1");
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            assert!(
                server.0.try_wait().unwrap().is_none(),
                "Server exited: {}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            match client
                .get(format!("{base}/instances/native-plugin-test/runtime"))
                .send()
                .await
            {
                Ok(response) => {
                    let runtime: Value = response.error_for_status().unwrap().json().await.unwrap();
                    assert!(runtime["data"].get("executionMode").is_none());
                    assert_eq!(runtime["data"]["runtime"], "computationGraph");
                    assert_eq!(runtime["data"]["running"], true);
                    break;
                }
                Err(error) if error.is_connect() => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => panic!("Runtime inspection failed: {error}"),
            }
        }
    })
    .await
    .unwrap_or_else(|error| {
        panic!(
            "Startup: {error}; {}",
            std::fs::read_to_string(&log_path).unwrap()
        )
    });

    let loaded: Value = client
        .get(format!("{base}/plugins"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    for kind in ["http", "log"] {
        let plugin = loaded["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|plugin| {
                plugin["kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["kind"] == kind)
            })
            .unwrap_or_else(|| panic!("Missing loaded {kind} metadata: {loaded}"));
        assert_eq!(
            plugin["sdkVersion"],
            drasi_plugin_sdk::ffi::FFI_SDK_VERSION,
            "{plugin}"
        );
    }

    client
        .post(format!(
            "http://127.0.0.1:{source_port}/sources/input/events"
        ))
        .json(&json!({
            "operation":"insert",
            "element":{"type":"node", "id":"one", "labels":["Item"], "properties":{
                "marker":"server-native-cdylib-proof", "value":42
            }}
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let rows: Value = client
                .get(format!("{base}/queries/selected/results"))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            if rows["data"] == json!([{"marker":"server-native-cdylib-proof","value":42}])
                && std::fs::read_to_string(&log_path)
                    .unwrap()
                    .contains("server-native-cdylib-proof")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|error| {
        panic!(
            "Plugin dataflow: {error}; {}",
            std::fs::read_to_string(&log_path).unwrap()
        )
    });
}
