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

//! HTTP content-negotiation integration tests.
//!
//! These tests spin up a *real* HTTP server (bound to a TCP port) and exercise
//! the REST API with both JSON and YAML request payloads, verifying that every
//! body-accepting route accepts the two formats interchangeably based on the
//! `Content-Type` header.

#![allow(clippy::unwrap_used)]

mod test_support;

use test_support::create_mock_source;

use axum::Router;
use drasi_lib::DrasiLib;
use drasi_server::api::v1::handlers;
use drasi_server::api::v1::routes::build_v1_router;
use drasi_server::instance_registry::InstanceRegistry;
use drasi_server::plugin_registry::PluginRegistry;
use std::sync::Arc;
use std::time::Duration;

/// Find a free TCP port by binding to port 0.
fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind to port 0");
    listener.local_addr().unwrap().port()
}

/// Build the production API router backed by a DrasiLib instance with a single
/// mock source, then start a real HTTP server on a random free port.
///
/// Returns the base URL the server is listening on.
async fn start_real_server() -> String {
    let instance_id = format!("http-fmt-{}", uuid::Uuid::new_v4());

    let core = DrasiLib::builder()
        .with_id(&instance_id)
        .with_source(create_mock_source("sensors"))
        .build()
        .await
        .expect("build core");
    let core = Arc::new(core);
    core.start().await.expect("start core");

    let mut instances_map = indexmap::IndexMap::new();
    instances_map.insert(instance_id.clone(), core.clone());
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

    let app = Router::new()
        .route("/health", axum::routing::get(handlers::health_check))
        .nest("/api/v1", v1_router);

    let port = find_free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind tcp listener");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("server error: {e}");
        }
    });

    // Wait until the server responds to /health.
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("server did not start within 10 seconds");
        }
        match client.get(format!("{base_url}/health")).send().await {
            Ok(resp) if resp.status().is_success() => break,
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }

    base_url
}

/// Creating a query with a JSON body (`Content-Type: application/json`) works.
#[tokio::test]
async fn test_create_query_with_json_payload() {
    let base_url = start_real_server().await;
    let client = reqwest::Client::new();

    let body = r#"{"id":"json-query","query":"MATCH (n) RETURN n"}"#;
    let resp = client
        .post(format!("{base_url}/api/v1/queries"))
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST query (json)");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "JSON payload should be accepted: {}",
        resp.text().await.unwrap_or_default()
    );

    // Verify it exists.
    let resp = client
        .get(format!("{base_url}/api/v1/queries/json-query"))
        .send()
        .await
        .expect("GET query");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}

/// Creating a query with a YAML body (`Content-Type: application/yaml`) works.
#[tokio::test]
async fn test_create_query_with_yaml_payload() {
    let base_url = start_real_server().await;
    let client = reqwest::Client::new();

    let body = "id: yaml-query\nquery: \"MATCH (n) RETURN n\"\n";
    let resp = client
        .post(format!("{base_url}/api/v1/queries"))
        .header("content-type", "application/yaml")
        .body(body)
        .send()
        .await
        .expect("POST query (yaml)");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "YAML payload should be accepted: {}",
        resp.text().await.unwrap_or_default()
    );

    // Verify it exists.
    let resp = client
        .get(format!("{base_url}/api/v1/queries/yaml-query"))
        .send()
        .await
        .expect("GET query");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}

/// The `text/yaml` media type is also recognised as YAML.
#[tokio::test]
async fn test_create_query_with_text_yaml_payload() {
    let base_url = start_real_server().await;
    let client = reqwest::Client::new();

    let body = "id: text-yaml-query\nquery: \"MATCH (n) RETURN n\"\nautoStart: false\n";
    let resp = client
        .post(format!("{base_url}/api/v1/queries"))
        .header("content-type", "text/yaml; charset=utf-8")
        .body(body)
        .send()
        .await
        .expect("POST query (text/yaml)");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "text/yaml payload should be accepted: {}",
        resp.text().await.unwrap_or_default()
    );
}

/// A malformed YAML body returns a structured 400 error.
#[tokio::test]
async fn test_invalid_yaml_payload_returns_400() {
    let base_url = start_real_server().await;
    let client = reqwest::Client::new();

    // Not a mapping — cannot deserialize into QueryConfigDto.
    let body = "- just\n- a\n- list\n";
    let resp = client
        .post(format!("{base_url}/api/v1/queries"))
        .header("content-type", "application/yaml")
        .body(body)
        .send()
        .await
        .expect("POST query (bad yaml)");

    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["code"], "INVALID_REQUEST");
}

/// Sending a YAML body to an instance-scoped route works too, confirming the
/// extractor is wired on both the convenience and instance-scoped handlers.
#[tokio::test]
async fn test_instance_scoped_route_accepts_yaml() {
    let base_url = start_real_server().await;
    let client = reqwest::Client::new();

    // Discover the instance id.
    let instances: serde_json::Value = client
        .get(format!("{base_url}/api/v1/instances"))
        .send()
        .await
        .expect("GET instances")
        .json()
        .await
        .unwrap();
    let instance_id = instances["data"][0]["id"]
        .as_str()
        .expect("instance id")
        .to_string();

    let body = "id: instance-yaml-query\nquery: \"MATCH (n) RETURN n\"\n";
    let resp = client
        .post(format!(
            "{base_url}/api/v1/instances/{instance_id}/queries"
        ))
        .header("content-type", "application/yaml")
        .body(body)
        .send()
        .await
        .expect("POST instance-scoped query (yaml)");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "instance-scoped YAML payload should be accepted: {}",
        resp.text().await.unwrap_or_default()
    );
}
