// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

mod test_support;

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use drasi_lib::{channels::ComponentStatus, DrasiLib, Query};
use drasi_plugin_sdk::{ReactionPluginDescriptor, SourcePluginDescriptor};
use drasi_server::{
    api::v1::routes::build_v1_router, instance_registry::InstanceRegistry,
    plugin_registry::PluginRegistry,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use test_support::{create_mock_reaction, create_mock_source};
use tokio::sync::RwLock;
use tower::ServiceExt;

struct CountingDescriptor(Arc<AtomicUsize>);

#[async_trait]
impl SourcePluginDescriptor for CountingDescriptor {
    fn kind(&self) -> &str {
        "mock"
    }
    fn config_version(&self) -> &str {
        "1.0.0"
    }
    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.into()
    }
    fn config_schema_name(&self) -> &str {
        "CountingSourceConfig"
    }
    async fn create_source(
        &self,
        id: &str,
        _config: &Value,
        auto_start: bool,
    ) -> anyhow::Result<Box<dyn drasi_lib::Source>> {
        Ok(Box::new(
            create_mock_source(id)
                .with_auto_start(auto_start)
                .with_start_counter(self.0.clone()),
        ))
    }
}

#[async_trait]
impl ReactionPluginDescriptor for CountingDescriptor {
    fn kind(&self) -> &str {
        "log"
    }
    fn config_version(&self) -> &str {
        "1.0.0"
    }
    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.into()
    }
    fn config_schema_name(&self) -> &str {
        "CountingReactionConfig"
    }
    async fn create_reaction(
        &self,
        id: &str,
        queries: Vec<String>,
        _config: &Value,
        auto_start: bool,
    ) -> anyhow::Result<Box<dyn drasi_lib::Reaction>> {
        Ok(Box::new(
            create_mock_reaction(id, queries)
                .with_auto_start(auto_start)
                .with_start_counter(self.0.clone()),
        ))
    }
}

async fn request(router: &Router, method: Method, uri: &str, body: Value) -> Value {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["success"], true, "{body}");
    body
}

async fn check_autostart(method: Method, category: &str) {
    for auto_start in [false, true] {
        let core = Arc::new(
            DrasiLib::builder()
                .with_id("autostart")
                .with_source(create_mock_source("input"))
                .with_query(
                    Query::cypher("query")
                        .query("MATCH (n) RETURN n")
                        .from_source("input")
                        .build(),
                )
                .build()
                .await
                .unwrap(),
        );
        core.start().await.unwrap();
        drasi_lib::wait_for_status(
            &core.component_graph(),
            "query",
            &[ComponentStatus::Running],
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        let starts = Arc::new(AtomicUsize::new(0));
        let descriptor = Arc::new(CountingDescriptor(starts.clone()));
        let mut plugins = PluginRegistry::new();
        plugins.register_source(descriptor.clone());
        plugins.register_reaction(descriptor);
        let registry = InstanceRegistry::from_map(
            [("autostart".to_string(), core.clone())]
                .into_iter()
                .collect(),
        );
        let router = build_v1_router(
            registry,
            Arc::new(false),
            None,
            Arc::new(RwLock::new(plugins)),
            None,
        );
        let collection = format!("/instances/autostart/{category}");
        let resource = format!("{collection}/counted");
        let uri = if method == Method::POST {
            &collection
        } else {
            &resource
        };
        let mut config = json!({
            "id": "counted",
            "kind": if category == "sources" { "mock" } else { "log" },
            "autoStart": auto_start,
        });
        if category == "reactions" {
            config["queries"] = json!(["query"]);
        }

        // Include deletion/recreation: the real SSE failure occurred during browser repair.
        for generation in 0..2 {
            request(&router, method.clone(), uri, config.clone()).await;
            assert_eq!(
                starts.load(Ordering::SeqCst),
                generation + usize::from(auto_start),
                "{method} {category}, autoStart={auto_start}: creation must start exactly once or not at all"
            );
            if !auto_start {
                request(
                    &router,
                    Method::POST,
                    &format!("{resource}/start"),
                    json!({}),
                )
                .await;
            }
            assert_eq!(starts.load(Ordering::SeqCst), generation + 1);
            drasi_lib::wait_for_status(
                &core.component_graph(),
                "counted",
                &[ComponentStatus::Running],
                Duration::from_secs(5),
            )
            .await
            .unwrap();
            let full = request(
                &router,
                Method::GET,
                &format!("{resource}?view=full"),
                Value::Null,
            )
            .await;
            assert_eq!(full["data"]["status"], "Running");
            request(&router, Method::DELETE, &resource, Value::Null).await;
        }
        core.stop().await.unwrap();
    }
}

#[tokio::test]
async fn post_source_autostarts_once() {
    check_autostart(Method::POST, "sources").await;
}

#[tokio::test]
async fn put_source_autostarts_once() {
    check_autostart(Method::PUT, "sources").await;
}

#[tokio::test]
async fn post_reaction_autostarts_once() {
    check_autostart(Method::POST, "reactions").await;
}

#[tokio::test]
async fn put_reaction_autostarts_once() {
    check_autostart(Method::PUT, "reactions").await;
}
