#![allow(clippy::unwrap_used)]

mod test_support;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_lib::{ComponentName, Query, QueryConfig};
use drasi_server::api::v1::{handlers, routes::build_v1_router};
use drasi_server::instance_registry::InstanceRegistry;
use drasi_server::plugin_registry::PluginRegistry;
use std::sync::Arc;
use test_support::{create_mock_reaction, create_mock_source};
use tower::ServiceExt;

/// Build a test router with the given instance ID and components.
async fn build_snapshot_test_router(
    instance_id: &str,
    sources: Vec<(ComponentName, test_support::mock_components::MockSource)>,
    queries: Vec<QueryConfig>,
    reactions: Vec<(ComponentName, test_support::mock_components::MockReaction)>,
) -> Router {
    let mut builder = crate::query_plugin::builder()
        .with_id(instance_id)
        .with_allow_external_component_ids(true);

    for (name, src) in sources {
        builder = builder.with_source(name, src);
    }
    for q in queries {
        builder = builder.with_query(q);
    }
    for (name, r) in reactions {
        builder = builder.with_reaction(name, r);
    }

    let core = builder.build().await.expect("Failed to build test core");
    let core = Arc::new(core);
    core.start().await.expect("Failed to start core");

    let mut instances_map = indexmap::IndexMap::new();
    instances_map.insert(instance_id.to_string(), core);
    let registry = InstanceRegistry::from_map(instances_map);

    let mut plugin_registry = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugin_registry);

    let v1_router = build_v1_router(
        registry,
        Arc::new(false),
        None,
        Arc::new(tokio::sync::RwLock::new(plugin_registry)),
        None,
    );

    Router::new()
        .route("/health", axum::routing::get(handlers::health_check))
        .merge(v1_router)
}

/// Helper: perform a GET request and return status + parsed JSON body.
async fn get_json(router: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn test_snapshot_returns_all_components() {
    let instance_id = "snap-all-components";

    let source = create_mock_source("snap-src");
    let query = Query::cypher(ComponentName::try_new("snap-query").unwrap())
        .query("MATCH (n:Node) RETURN n")
        .from_input_by_name(ComponentName::try_new("snap-src").unwrap())
        .auto_start(true)
        .build()
        .unwrap();
    let reaction = create_mock_reaction("snap-reaction", vec!["snap-query".to_string()]);

    let router = build_snapshot_test_router(
        instance_id,
        vec![(ComponentName::try_new("snap-src").unwrap(), source)],
        vec![query],
        vec![(ComponentName::try_new("snap-reaction").unwrap(), reaction)],
    )
    .await;

    let uri = format!("/instances/{instance_id}/snapshot");
    let (status, json) = get_json(router, &uri).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);

    let data = &json["data"];
    assert_eq!(data["instance_id"], instance_id);
    assert!(data["timestamp"].is_string(), "timestamp should be present");

    // Verify source
    let sources = data["sources"]
        .as_array()
        .expect("sources should be an array");
    assert!(
        sources.iter().any(|s| s["name"] == "snap-src"),
        "snapshot should contain source 'snap-src', got: {sources:?}"
    );

    // Verify query
    let queries = data["queries"]
        .as_array()
        .expect("queries should be an array");
    assert!(
        queries.iter().any(|q| q["name"] == "snap-query"),
        "snapshot should contain query 'snap-query', got: {queries:?}"
    );

    // Verify reaction
    let reactions = data["reactions"]
        .as_array()
        .expect("reactions should be an array");
    assert!(
        reactions.iter().any(|r| r["name"] == "snap-reaction"),
        "snapshot should contain reaction 'snap-reaction', got: {reactions:?}"
    );
}

#[tokio::test]
async fn test_snapshot_instance_not_found() {
    // Build a router with one instance but request a different one.
    let source = create_mock_source("dummy-src");
    let router = build_snapshot_test_router(
        "real-instance",
        vec![(ComponentName::try_new("dummy-src").unwrap(), source)],
        vec![],
        vec![],
    )
    .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/instances/nonexistent/snapshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_snapshot_includes_properties() {
    let instance_id = "snap-props";

    let source = create_mock_source("props-src");
    let router = build_snapshot_test_router(
        instance_id,
        vec![(ComponentName::try_new("props-src").unwrap(), source)],
        vec![],
        vec![],
    )
    .await;

    let uri = format!("/instances/{instance_id}/snapshot");
    let (status, json) = get_json(router, &uri).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);

    let sources = json["data"]["sources"]
        .as_array()
        .expect("sources should be an array");
    let src = sources
        .iter()
        .find(|s| s["name"] == "props-src")
        .expect("source 'props-src' should be in snapshot");

    // The "properties" key should exist (may be empty object for mocks).
    assert!(
        src.get("properties").is_some(),
        "source entry should have a 'properties' field, got: {src:?}"
    );
}

#[tokio::test]
async fn test_snapshot_includes_dependency_edges() {
    let instance_id = "snap-edges";

    let source = create_mock_source("edge-src");
    let query = Query::cypher(ComponentName::try_new("edge-query").unwrap())
        .query("MATCH (n:Node) RETURN n")
        .from_input_by_name(ComponentName::try_new("edge-src").unwrap())
        .auto_start(true)
        .build()
        .unwrap();
    let reaction = create_mock_reaction("edge-reaction", vec!["edge-query".to_string()]);

    let router = build_snapshot_test_router(
        instance_id,
        vec![(ComponentName::try_new("edge-src").unwrap(), source)],
        vec![query],
        vec![(ComponentName::try_new("edge-reaction").unwrap(), reaction)],
    )
    .await;

    let uri = format!("/instances/{instance_id}/snapshot");
    let (status, json) = get_json(router, &uri).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);

    let edges = json["data"]["edges"]
        .as_array()
        .expect("edges should be an array");
    assert!(
        !edges.is_empty(),
        "edges should be non-empty for source→query→reaction chain"
    );

    let component_id = |group: &str, name: &str| {
        json["data"][group]
            .as_array()
            .unwrap()
            .iter()
            .find(|component| component["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let source_id = component_id("sources", "edge-src");
    let query_id = component_id("queries", "edge-query");
    let reaction_id = component_id("reactions", "edge-reaction");
    let entity = |id: &str| serde_json::json!({"Component": id});

    // There should be an edge from source to query (source feeds query).
    let has_source_to_query = edges.iter().any(|e| {
        e["from"] == entity(&source_id)
            && e["to"] == entity(&query_id)
            && e["relationship"] == "Feeds"
    });
    assert!(
        has_source_to_query,
        "should have source→query edge, got: {edges:?}"
    );

    // There should be an edge from query to reaction (query feeds reaction).
    let has_query_to_reaction = edges.iter().any(|e| {
        e["from"] == entity(&query_id)
            && e["to"] == entity(&reaction_id)
            && e["relationship"] == "Feeds"
    });
    assert!(
        has_query_to_reaction,
        "should have query→reaction edge, got: {edges:?}"
    );
}

#[path = "query_plugin/mod.rs"]
mod query_plugin;
