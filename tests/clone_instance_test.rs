// Copyright 2025 The Drasi Authors.
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

//! Integration tests for the clone instance endpoint.
//!
//! POST /instances/{targetInstanceId}/clone
//! Body: { "sourceInstanceId": "..." }
//!
//! These tests validate that cloning captures all components from a source
//! instance and recreates them in a target instance, filtering internal sources.

#![allow(clippy::unwrap_used)]

mod test_support;

use test_support::{create_mock_reaction, create_mock_source};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_lib::{DrasiLib, Query};
use drasi_plugin_sdk::{ReactionPluginDescriptor, SourcePluginDescriptor};
use drasi_server::api::v1::handlers;
use drasi_server::api::v1::routes::build_v1_router;
use drasi_server::instance_registry::InstanceRegistry;
use drasi_server::plugin_registry::PluginRegistry;
use std::sync::Arc;
use tower::ServiceExt;

const SOURCE_INSTANCE: &str = "source-instance";
const TARGET_INSTANCE: &str = "target-instance";
const QUERY_TEXT: &str = "MATCH (s:Sensor) WHERE s.temperature > 75 RETURN s";

/// Plugin descriptor that produces `MockSource` instances for the clone factory.
struct MockSourceDescriptor;

#[async_trait]
impl SourcePluginDescriptor for MockSourceDescriptor {
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
        "MockSourceConfig"
    }
    async fn create_source(
        &self,
        id: &str,
        _config_json: &serde_json::Value,
        _auto_start: bool,
    ) -> anyhow::Result<Box<dyn drasi_lib::sources::Source>> {
        Ok(Box::new(create_mock_source(id)))
    }
}

/// Plugin descriptor that produces `MockReaction` instances for the clone factory.
struct MockReactionDescriptor;

#[async_trait]
impl ReactionPluginDescriptor for MockReactionDescriptor {
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
        "MockReactionConfig"
    }
    async fn create_reaction(
        &self,
        id: &str,
        query_ids: Vec<String>,
        _config_json: &serde_json::Value,
        _auto_start: bool,
    ) -> anyhow::Result<Box<dyn drasi_lib::reactions::Reaction>> {
        Ok(Box::new(create_mock_reaction(id, query_ids)))
    }
}

/// Build a router with two DrasiLib instances: a populated source and an empty target.
async fn create_clone_test_router() -> Router {
    // --- Source instance: has a source, query, and reaction ---
    let mock_src = create_mock_source("clone-src");
    let mock_reaction = create_mock_reaction("clone-rx", vec!["clone-query".to_string()]);

    let clone_query = Query::cypher("clone-query")
        .query(QUERY_TEXT)
        .from_source("clone-src")
        .auto_start(false)
        .build();

    let source_core = DrasiLib::builder()
        .with_id(SOURCE_INSTANCE)
        .with_source(mock_src)
        .with_query(clone_query)
        .with_reaction(mock_reaction)
        .build()
        .await
        .expect("Failed to build source instance");

    let source_core = Arc::new(source_core);
    source_core
        .start()
        .await
        .expect("Failed to start source instance");

    // --- Target instance: empty ---
    let target_core = DrasiLib::builder()
        .with_id(TARGET_INSTANCE)
        .build()
        .await
        .expect("Failed to build target instance");

    let target_core = Arc::new(target_core);
    target_core
        .start()
        .await
        .expect("Failed to start target instance");

    // Build registry with both instances
    let mut instances = indexmap::IndexMap::new();
    instances.insert(SOURCE_INSTANCE.to_string(), source_core);
    instances.insert(TARGET_INSTANCE.to_string(), target_core);
    let registry = InstanceRegistry::from_map(instances);

    let read_only = Arc::new(false);
    let config_persistence = None;
    let mut plugin_registry = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugin_registry);
    plugin_registry.register_source(Arc::new(MockSourceDescriptor));
    plugin_registry.register_reaction(Arc::new(MockReactionDescriptor));
    let solutions_dir = None;

    let v1_router = build_v1_router(
        registry,
        read_only,
        config_persistence,
        Arc::new(tokio::sync::RwLock::new(plugin_registry)),
        solutions_dir,
    );

    Router::new()
        .route("/health", axum::routing::get(handlers::health_check))
        .merge(v1_router)
}

/// Helper: issue POST clone request and return (status, body JSON).
async fn clone_request(
    router: Router,
    target_id: &str,
    source_id: &str,
) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({ "sourceInstanceId": source_id });
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{target_id}/clone"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

/// Helper: GET a resource list from an instance and return the parsed JSON body.
async fn get_list(router: Router, instance_id: &str, resource: &str) -> serde_json::Value {
    let response = router
        .oneshot(
            Request::builder()
                .uri(format!("/instances/{instance_id}/{resource}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Clone from a populated source instance to an empty target.
/// Verify all components (source, query, reaction) appear in the target.
#[tokio::test]
async fn test_clone_creates_all_components() {
    let router = create_clone_test_router().await;

    // Perform clone
    let (status, json) = clone_request(router.clone(), TARGET_INSTANCE, SOURCE_INSTANCE).await;

    assert_eq!(status, StatusCode::OK);
    let data = &json["data"];
    assert_eq!(data["success"], true);
    assert!(
        data["sourcesCreated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "clone-src"),
        "Expected clone-src in sourcesCreated: {data}"
    );
    assert!(
        data["queriesCreated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "clone-query"),
        "Expected clone-query in queriesCreated: {data}"
    );
    assert!(
        data["reactionsCreated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "clone-rx"),
        "Expected clone-rx in reactionsCreated: {data}"
    );

    // Verify target has the components via GET lists
    let sources = get_list(router.clone(), TARGET_INSTANCE, "sources").await;
    let source_ids: Vec<&str> = sources["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(
        source_ids.contains(&"clone-src"),
        "Target sources should contain clone-src: {source_ids:?}"
    );

    let queries = get_list(router.clone(), TARGET_INSTANCE, "queries").await;
    let query_ids: Vec<&str> = queries["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_str().unwrap())
        .collect();
    assert!(
        query_ids.contains(&"clone-query"),
        "Target queries should contain clone-query: {query_ids:?}"
    );

    let reactions = get_list(router.clone(), TARGET_INSTANCE, "reactions").await;
    let reaction_ids: Vec<&str> = reactions["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert!(
        reaction_ids.contains(&"clone-rx"),
        "Target reactions should contain clone-rx: {reaction_ids:?}"
    );
}

/// Clone from a nonexistent source instance returns an error response.
#[tokio::test]
async fn test_clone_source_instance_not_found() {
    let router = create_clone_test_router().await;

    let (status, json) =
        clone_request(router.clone(), TARGET_INSTANCE, "nonexistent-instance").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        json["message"].is_string(),
        "Expected error message in response when source instance not found: {json}"
    );

    let msg = json["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("not found"),
        "Error message should mention 'not found': {json}"
    );
}

/// Clone preserves query configuration (query text, sources).
/// Verify by GET-ing the target query with ?view=full.
#[tokio::test]
async fn test_clone_preserves_query_config() {
    let router = create_clone_test_router().await;

    // Clone
    let (status, _) = clone_request(router.clone(), TARGET_INSTANCE, SOURCE_INSTANCE).await;
    assert_eq!(status, StatusCode::OK);

    // GET the cloned query with ?view=full to retrieve its config
    let response = router
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/instances/{TARGET_INSTANCE}/queries/clone-query?view=full"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let config = &json["data"]["config"];
    assert!(
        !config.is_null(),
        "Expected config in view=full response: {json}"
    );
    assert_eq!(
        config["query"].as_str().unwrap(),
        QUERY_TEXT,
        "Cloned query text should match source"
    );

    // Verify source subscription was preserved
    let sources = config["sources"].as_array().unwrap();
    let source_ids: Vec<&str> = sources
        .iter()
        .map(|s| s["sourceId"].as_str().unwrap())
        .collect();
    assert!(
        source_ids.contains(&"clone-src"),
        "Cloned query should reference clone-src: {source_ids:?}"
    );
}

/// Clone filters out internal sources (those starting with "__").
/// The __component_graph__ source created by DrasiLib should not be cloned.
#[tokio::test]
async fn test_clone_filters_internal_sources() {
    let router = create_clone_test_router().await;

    // First verify the source instance has a __component_graph__ source
    // (DrasiLib creates this automatically)
    let source_sources = get_list(router.clone(), SOURCE_INSTANCE, "sources").await;
    let _source_source_ids: Vec<&str> = source_sources["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    // The list may or may not expose internal sources depending on the API filter,
    // but the snapshot will contain them - the key point is the clone skips them.

    // Clone
    let (status, json) = clone_request(router.clone(), TARGET_INSTANCE, SOURCE_INSTANCE).await;
    assert_eq!(status, StatusCode::OK);

    let data = &json["data"];
    assert_eq!(data["success"], true);

    // Verify no internal sources were created
    let sources_created = data["sourcesCreated"].as_array().unwrap();
    for src in sources_created {
        let id = src.as_str().unwrap();
        assert!(
            !id.starts_with("__"),
            "Internal source '{id}' should not be cloned"
        );
    }

    // Also verify target sources list has no internal sources
    let target_sources = get_list(router.clone(), TARGET_INSTANCE, "sources").await;
    let target_ids: Vec<&str> = target_sources["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();

    // Should have clone-src but NOT __component_graph__ (from the clone)
    assert!(
        target_ids.contains(&"clone-src"),
        "Target should have clone-src"
    );
    for id in &target_ids {
        // __component_graph__ created by the target's own DrasiLib is OK,
        // but there should be at most one (not duplicated from clone)
        if id.starts_with("__") {
            // This is the target instance's own internal source, not a cloned one.
            // Ensure it wasn't in the sourcesCreated list
            assert!(
                !sources_created.iter().any(|s| s.as_str().unwrap() == *id),
                "Internal source '{id}' should not appear in sourcesCreated"
            );
        }
    }
}
