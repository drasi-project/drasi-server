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
use drasi_lib::{DrasiLib, ExecutionMode};
use drasi_reaction_application::ApplicationReaction;
use drasi_server::{
    api::mappings::DtoMapper,
    config::{DrasiServerConfig, ExecutionModeConfig, ExecutionModePolicy},
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
fn config_defaults_inherits_and_rejects_unknown_modes() {
    let legacy: DrasiServerConfig = serde_yaml::from_str("id: legacy").unwrap();
    assert_eq!(legacy.execution_mode, ExecutionModeConfig::ComponentGraph);
    assert!(!serde_yaml::to_string(&legacy)
        .unwrap()
        .contains("executionMode"));

    let config: DrasiServerConfig = serde_yaml::from_str(
        "executionMode: computationGraph\ninstances:\n  - id: native\n  - id: legacy\n    executionMode: componentGraph\n",
    )
    .unwrap();
    let resolved = config.resolved_instances(&DtoMapper::new()).unwrap();
    assert_eq!(resolved[0].execution_mode, ExecutionMode::ComputationGraph);
    assert_eq!(resolved[1].execution_mode, ExecutionMode::ComponentGraph);
    assert!(serde_yaml::from_str::<DrasiServerConfig>("executionMode: native").is_err());
}

#[tokio::test]
async fn builder_uses_actual_mode_for_all_construction_paths() {
    let legacy = DrasiServerBuilder::new().build_core().await.unwrap();
    assert_eq!(legacy.execution_mode(), ExecutionMode::ComponentGraph);
    let native = DrasiServerBuilder::new()
        .with_execution_mode(ExecutionMode::ComputationGraph)
        .build_core()
        .await
        .unwrap();
    assert_eq!(native.execution_mode(), ExecutionMode::ComputationGraph);

    let handles = DrasiServerBuilder::new()
        .with_id("first")
        .add_instance_builder(DrasiLib::builder().with_id("second"))
        .with_execution_mode(ExecutionMode::ComputationGraph)
        .add_instance_builder(DrasiLib::builder().with_id("third"))
        .build_with_handles()
        .await
        .unwrap();
    assert_eq!(handles.servers.len(), 3);
    for core in handles.servers.values() {
        assert_eq!(core.execution_mode(), ExecutionMode::ComputationGraph);
        core.stop().await.unwrap();
    }
}

#[tokio::test]
async fn api_creation_uses_stable_default_and_rejects_conflicting_force() {
    let registry = InstanceRegistry::new().with_execution_mode_policy(ExecutionModePolicy {
        default_mode: ExecutionModeConfig::ComputationGraph,
        forced_mode: None,
    });
    // A different first instance must not change the server-level default.
    registry
        .add(
            "legacy".into(),
            Arc::new(DrasiLib::builder().build().await.unwrap()),
        )
        .await
        .unwrap();
    let app = router(registry.clone(), None);
    let (status, body) = request(&app, "POST", "/api/v1/instances", json!({"id":"native"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let native = registry.get("native").await.unwrap();
    assert_eq!(native.execution_mode(), ExecutionMode::ComputationGraph);
    let (status, body) =
        request(&app, "GET", "/api/v1/instances/native/runtime", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["executionMode"], "computationGraph");
    assert_eq!(body["data"]["running"], true);
    native.stop().await.unwrap();

    let forced = InstanceRegistry::new().with_execution_mode_policy(ExecutionModePolicy {
        default_mode: ExecutionModeConfig::ComputationGraph,
        forced_mode: Some(ExecutionModeConfig::ComputationGraph),
    });
    let app = router(forced.clone(), None);
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/instances",
        json!({"id":"conflict", "executionMode":"componentGraph"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(forced.is_empty().await);
    let (status, body) = request(&app, "POST", "/api/v1/instances", json!({"id":"first"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let first = forced.get("first").await.unwrap();
    assert_eq!(first.execution_mode(), ExecutionMode::ComputationGraph);
    first.stop().await.unwrap();
}

#[tokio::test]
async fn ordinary_api_query_and_reaction_execute_in_both_modes() {
    for mode in [
        ExecutionMode::ComponentGraph,
        ExecutionMode::ComputationGraph,
    ] {
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
                .with_execution_mode(mode)
                .with_source(source)
                .build_core()
                .await
                .unwrap(),
        );
        assert_eq!(core.execution_mode(), mode);
        core.start().await.unwrap();
        let registry = InstanceRegistry::new();
        registry.add("test".into(), core.clone()).await.unwrap();
        let app = router(registry, None);
        let (status, body) = request(&app, "POST", "/api/v1/queries", json!({
            "id":"selected", "query":"MATCH (n:Item) WHERE n.value > 10 RETURN n.value AS value",
            "queryLanguage":"Cypher", "sources":[{"sourceId":"input"}],
            "autoStart":true, "enableBootstrap":false,
        })).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (reaction, output) = ApplicationReaction::new("output", vec!["selected".into()]);
        let mut receiver = output.take_receiver().await.unwrap();
        core.add_reaction(reaction).await.unwrap();
        drasi_lib::wait_for_status(
            &core.component_graph(),
            "output",
            &[drasi_lib::ComponentStatus::Running],
            Duration::from_secs(10),
        )
        .await
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
        .expect("query change reaches ordinary reaction in selected runtime");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let (status, body) =
                    request(&app, "GET", "/api/v1/queries/selected/results", Value::Null).await;
                assert_eq!(status, StatusCode::OK, "{body}");
                if body["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["value"] == 42)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("ordinary query results return native data");
        let (status, body) =
            request(&app, "GET", "/api/v1/instances/test/runtime", Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["data"]["executionMode"],
            serde_json::to_value(ExecutionModeConfig::from(mode)).unwrap()
        );
        core.stop().await.unwrap();
    }
}

#[tokio::test]
async fn runtime_persistence_preserves_modes_and_root_default_on_unrelated_mutation() {
    for (default_mode, actual) in [
        (
            ExecutionModeConfig::ComponentGraph,
            ExecutionMode::ComponentGraph,
        ),
        (
            ExecutionModeConfig::ComponentGraph,
            ExecutionMode::ComputationGraph,
        ),
        (
            ExecutionModeConfig::ComputationGraph,
            ExecutionMode::ComponentGraph,
        ),
        (
            ExecutionModeConfig::ComputationGraph,
            ExecutionMode::ComputationGraph,
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("server.yaml");
        let core = Arc::new(
            DrasiLib::builder()
                .with_id("existing")
                .with_execution_mode(actual)
                .build()
                .await
                .unwrap(),
        );
        core.start().await.unwrap();
        let registry = InstanceRegistry::new().with_execution_mode_policy(ExecutionModePolicy {
            default_mode,
            forced_mode: None,
        });
        registry.add("existing".into(), core.clone()).await.unwrap();
        let original: DrasiServerConfig = serde_json::from_value(json!({
            "id":"server", "executionMode":default_mode,
            "instances":[{"id":"existing", "executionMode":ExecutionModeConfig::from(actual)}],
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
        persistence.save().await.unwrap();
        let saved = drasi_server::load_config_file(&path).unwrap();
        assert_eq!(saved.execution_mode, default_mode);
        assert_eq!(
            saved.instances.len(),
            usize::from(ExecutionMode::from(default_mode) != actual),
            "don't erase distinct root default by flattening"
        );
        assert_eq!(
            saved.resolved_instances(&DtoMapper::new()).unwrap()[0].execution_mode,
            actual
        );

        let app = router(registry.clone(), Some(persistence));
        let (status, body) = request(&app, "POST", "/api/v1/instances", json!({"id":"new"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let saved = drasi_server::load_config_file(&path).unwrap();
        assert_eq!(saved.execution_mode, default_mode);
        if default_mode == ExecutionModeConfig::ComponentGraph
            && actual == ExecutionMode::ComponentGraph
        {
            assert!(!std::fs::read_to_string(&path)
                .unwrap()
                .contains("executionMode"));
        }
        let resolved = saved.resolved_instances(&DtoMapper::new()).unwrap();
        assert_eq!(resolved[0].execution_mode, actual);
        assert_eq!(resolved[1].execution_mode, default_mode.into());
        for config in resolved {
            let reloaded = DrasiLib::builder()
                .with_id(&config.id)
                .with_execution_mode(config.execution_mode)
                .build()
                .await
                .unwrap();
            assert_eq!(reloaded.execution_mode(), config.execution_mode);
        }
        for (_, instance) in registry.list().await {
            instance.stop().await.unwrap();
        }
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
                "executionMode":"componentGraph", "persistConfig":false,
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
                "--execution-mode",
                "computation-graph",
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
                    assert_eq!(runtime["data"]["executionMode"], "computationGraph");
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
