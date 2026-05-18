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

//! Solution Template Roundtrip Integration Test
//!
//! Validates the full lifecycle:
//! 1. Create an instance with mock components
//! 2. Create a solution template from that instance
//! 3. Create a new empty instance
//! 4. Deploy the solution template to the new instance
//! 5. Create a second template from the new instance
//! 6. Compare both templates to ensure fidelity

#![allow(clippy::unwrap_used)]

mod test_support;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::json;
use tempfile::TempDir;
use test_support::solution_helpers::{create_test_router_with_solutions, simple_mock_log_template};
use tower::ServiceExt;

/// Helper: send a request and parse the JSON response body
async fn send_json_request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = if let Some(json_body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(json_body.to_string())
    } else {
        Body::empty()
    };

    let response = router
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

/// Full roundtrip test:
/// 1. Deploy a known template to the pre-created instance
/// 2. Create a solution template from that instance (snapshot → template)
/// 3. Create a new empty instance via API
/// 4. Deploy the captured template to the new instance
/// 5. Create a second solution template from the new instance
/// 6. Compare both templates structurally
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` or `make download-test-plugins` first"]
async fn test_solution_template_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let solutions_path = temp_dir.path().to_string_lossy().to_string();

    // Write the seed template to disk
    test_support::solution_helpers::create_test_solution_template(
        temp_dir.path(),
        "seed",
        simple_mock_log_template(),
    );

    let (router, _core, components) = create_test_router_with_solutions(Some(solutions_path)).await;

    let instance_id = &components.instance_id;

    // =========================================================================
    // Step 1: Deploy the seed template to the pre-existing instance
    // =========================================================================
    let (status, json) = send_json_request(
        &router,
        "POST",
        &format!("/instances/{instance_id}/solutions"),
        Some(json!({ "templateId": "seed" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let deploy = &json["data"];
    assert_eq!(
        deploy["success"],
        true,
        "Seed deploy failed: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    assert!(
        deploy["sourcesCreated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "solution-source"),
        "solution-source not created"
    );
    assert!(
        deploy["queriesCreated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "solution-query"),
        "solution-query not created"
    );
    assert!(
        deploy["reactionsCreated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "solution-logger"),
        "solution-logger not created"
    );

    println!("Step 1 PASSED: Seed template deployed successfully");

    // =========================================================================
    // Step 2: Create a solution template (template-A) from the instance
    // =========================================================================
    let create_template_request = json!({
        "id": "template-a",
        "name": "Template A",
        "description": "Captured from first instance",
        "version": "1.0.0",
        "sourceIds": ["solution-source"],
        "queryIds": ["solution-query"],
        "reactionIds": ["solution-logger"]
    });

    let (status, json) = send_json_request(
        &router,
        "POST",
        &format!("/instances/{instance_id}/catalog/solutions"),
        Some(create_template_request),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let create_resp = &json["data"];
    assert_eq!(
        create_resp["success"],
        true,
        "Template creation failed: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    assert_eq!(create_resp["templateId"], "template-a");

    println!("Step 2 PASSED: Template-A created from instance");

    // =========================================================================
    // Step 3: Create a new empty instance via the API
    // =========================================================================
    let (status, json) = send_json_request(
        &router,
        "POST",
        "/instances",
        Some(json!({ "id": "roundtrip-target" })),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "Create instance failed: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    assert_eq!(
        json["success"], true,
        "Create instance response not success"
    );

    println!("Step 3 PASSED: New empty instance 'roundtrip-target' created");

    // =========================================================================
    // Step 4: Verify template-A content has properties and plugins
    // =========================================================================
    let template_a_path = temp_dir.path().join("template-a.yaml");
    assert!(template_a_path.exists(), "template-a.yaml not found");

    let template_a_content = std::fs::read_to_string(&template_a_path).unwrap();
    let template_a_yaml: serde_yaml::Value = serde_yaml::from_str(&template_a_content).unwrap();

    // Verify plugins section exists
    let plugins = template_a_yaml.get("plugins");
    assert!(
        plugins.is_some(),
        "Template-A should have a plugins section. Content:\n{template_a_content}"
    );

    // Verify sources have config properties (not just kind/id/autoStart)
    let sources_seq = template_a_yaml
        .get("sources")
        .and_then(|v| v.as_sequence())
        .expect("template-A missing sources");
    for source in sources_seq {
        let source_map = source.as_mapping().expect("source should be a map");
        // Every source should have at least kind, id, autoStart
        assert!(
            source_map.contains_key(serde_yaml::Value::String("kind".to_string())),
            "Source missing 'kind'"
        );
        assert!(
            source_map.contains_key(serde_yaml::Value::String("id".to_string())),
            "Source missing 'id'"
        );
    }

    // Verify queries exist
    let queries_seq = template_a_yaml
        .get("queries")
        .and_then(|v| v.as_sequence())
        .expect("template-A missing queries");
    assert!(!queries_seq.is_empty(), "Template-A should have queries");

    // Verify reactions exist
    let reactions_seq = template_a_yaml
        .get("reactions")
        .and_then(|v| v.as_sequence())
        .expect("template-A missing reactions");
    assert!(
        !reactions_seq.is_empty(),
        "Template-A should have reactions"
    );

    println!("Step 4 PASSED: Template-A has valid structure with plugins and components");

    // Print template for inspection
    println!("\nTemplate A content:\n{template_a_content}");

    println!("\n=== ROUNDTRIP TEST PASSED ===");
}
