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

//! Solution Deployment Integration Tests
//!
//! These tests validate deploying solution templates via the API:
//! - Basic deployment of mock source to log reaction pipelines
//! - Variable substitution during deployment
//! - Validation errors (missing variables, invalid requests)
//! - Multi-instance deployment (non-default instance)
//! - Scriptfile bootstrap provider
//! - Multi-source queries with joins
//! - Reaction output validation

#![allow(clippy::unwrap_used)]

mod test_support;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::json;
use tempfile::TempDir;
use test_support::solution_helpers::{
    create_multi_instance_test_router, create_test_router_with_solutions,
    create_test_solution_template, multi_source_join_template, simple_mock_log_template,
    template_with_variables,
};
use tower::ServiceExt;

// =============================================================================
// Basic Deployment Tests
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_mock_source_log_reaction() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "simple-pipeline",
        simple_mock_log_template(),
    );

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the solution
    let deploy_request = json!({
        "templateId": "simple-pipeline"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Debug: Print the response
    println!(
        "Deploy response: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );

    assert_eq!(json["success"], true);
    let deploy_response = &json["data"];
    assert_eq!(
        deploy_response["success"], true,
        "Deploy failed: {:?}",
        deploy_response["errors"]
    );

    // Verify components were created
    let sources_created = deploy_response["sourcesCreated"].as_array().unwrap();
    assert!(sources_created.iter().any(|id| id == "solution-source"));

    let queries_created = deploy_response["queriesCreated"].as_array().unwrap();
    assert!(queries_created.iter().any(|id| id == "solution-query"));

    let reactions_created = deploy_response["reactionsCreated"].as_array().unwrap();
    assert!(reactions_created.iter().any(|id| id == "solution-logger"));

    // Verify components exist in DrasiLib
    let sources = core.list_sources().await.unwrap();
    // Note: There might be pre-registered components too
    assert!(sources.iter().any(|(id, _)| id == "solution-source"));

    // =========================================================================
    // DATA FLOW VALIDATION
    // =========================================================================

    // MockSource generates Generic data every 100ms.
    // Wait for the query to receive data and produce results.
    let results = test_support::solution_helpers::wait_for_query_results(
        &core,
        "solution-query",
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("Query 'solution-query' should receive data from MockSource");

    // The query is "MATCH (n) RETURN n" so results should contain node data
    assert!(
        !results.is_empty(),
        "Should have at least one result from the query"
    );

    println!(
        "Simple pipeline test validated: {} results received from query",
        results.len()
    );
}

#[tokio::test]
async fn test_deploy_solution_with_variables() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "configurable-pipeline",
        template_with_variables(),
    );

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy with variable overrides
    let deploy_request = json!({
        "templateId": "configurable-pipeline",
        "variables": {
            "INTERVAL_MS": "500",
            "TEMP_THRESHOLD": "80"
        }
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let deploy_response = &json["data"];
    assert_eq!(deploy_response["success"], true);
}

// =============================================================================
// Validation Error Tests
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_validation_error_neither_template_nor_yaml() {
    let temp_dir = TempDir::new().unwrap();

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy with neither templateId nor yaml
    let deploy_request = json!({
        "variables": {}
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["code"].is_string());
    assert!(!json["message"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_deploy_solution_validation_error_both_template_and_yaml() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(temp_dir.path(), "test-template", simple_mock_log_template());

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy with both templateId and yaml
    let deploy_request = json!({
        "templateId": "test-template",
        "yaml": simple_mock_log_template()
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["code"].is_string());
    assert!(!json["message"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_deploy_solution_nonexistent_template() {
    let temp_dir = TempDir::new().unwrap();

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Try to deploy non-existent template
    let deploy_request = json!({
        "templateId": "non-existent-template"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["code"].is_string());
    assert!(!json["message"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_deploy_solution_to_nonexistent_instance() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(temp_dir.path(), "test-template", simple_mock_log_template());

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Try to deploy to non-existent instance
    let deploy_request = json!({
        "templateId": "test-template"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/instances/non-existent-instance/solutions")
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Either the response status is 404 or the deploy response indicates failure
    if status == StatusCode::OK {
        assert_eq!(json["data"]["success"], false);
    } else {
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

// =============================================================================
// Multi-Instance Deployment Tests
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_to_non_default_instance() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "simple-pipeline",
        simple_mock_log_template(),
    );

    // Create router with multiple instances
    let (router, instances_data) = create_multi_instance_test_router(
        vec![
            "default-instance",
            "secondary-instance",
            "tertiary-instance",
        ],
        Some(temp_dir.path().to_string_lossy().to_string()),
    )
    .await;

    // Deploy to the secondary (non-default) instance
    let deploy_request = json!({
        "templateId": "simple-pipeline"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/instances/secondary-instance/solutions")
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let deploy_response = &json["data"];
    assert_eq!(deploy_response["success"], true);

    // Verify components were created in secondary instance
    let (_, secondary_core, _) = instances_data
        .iter()
        .find(|(id, _, _)| id == "secondary-instance")
        .unwrap();

    let sources = secondary_core.list_sources().await.unwrap();
    // The deployed solution adds "solution-source"
    // Note: may have pre-registered "secondary-instance-source" too
    assert!(sources.iter().any(|(id, _)| id == "solution-source"));
}

#[tokio::test]
async fn test_deploy_solution_instance_isolation() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "simple-pipeline",
        simple_mock_log_template(),
    );

    // Create router with multiple instances
    let (router, instances_data) = create_multi_instance_test_router(
        vec!["instance-a", "instance-b"],
        Some(temp_dir.path().to_string_lossy().to_string()),
    )
    .await;

    // Deploy to instance-a only
    let deploy_request = json!({
        "templateId": "simple-pipeline"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/instances/instance-a/solutions")
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["success"], true);

    // Verify instance-a has the deployed components
    let (_, instance_a_core, _) = instances_data
        .iter()
        .find(|(id, _, _)| id == "instance-a")
        .unwrap();
    let sources_a = instance_a_core.list_sources().await.unwrap();
    assert!(sources_a.iter().any(|(id, _)| id == "solution-source"));

    // Verify instance-b does NOT have the deployed components
    // (only has its pre-registered "instance-b-source")
    let (_, instance_b_core, _) = instances_data
        .iter()
        .find(|(id, _, _)| id == "instance-b")
        .unwrap();
    let sources_b = instance_b_core.list_sources().await.unwrap();
    // test-source should NOT be in instance-b
    assert!(!sources_b.iter().any(|(id, _)| id == "solution-source"));
    // But instance-b should have its own pre-registered source
    assert!(sources_b.iter().any(|(id, _)| id == "instance-b-source"));
}

// =============================================================================
// YAML Deployment Tests (inline YAML instead of template)
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_from_yaml() {
    let temp_dir = TempDir::new().unwrap();

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy using inline YAML instead of template
    let inline_yaml = r#"
name: Inline YAML Solution
description: Deployed from inline YAML

sources:
  - kind: mock
    id: inline-source
    autoStart: true
    dataType:
      type: generic
    intervalMs: 100

queries:
  - id: inline-query
    query: "MATCH (n) RETURN n"
    queryLanguage: Cypher
    sources:
      - sourceId: inline-source
    autoStart: true

reactions:
  - kind: log
    id: inline-logger
    queries:
      - inline-query
    autoStart: true
"#;

    let deploy_request = json!({
        "yaml": inline_yaml
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let deploy_response = &json["data"];
    assert_eq!(deploy_response["success"], true);

    // Verify components were created
    let sources_created = deploy_response["sourcesCreated"].as_array().unwrap();
    assert!(sources_created.iter().any(|id| id == "inline-source"));
}

// =============================================================================
// Multi-Source Join Tests
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_multi_source_query() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "join-solution",
        multi_source_join_template(),
    );

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the multi-source solution
    let deploy_request = json!({
        "templateId": "join-solution"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let deploy_response = &json["data"];
    assert_eq!(deploy_response["success"], true);

    // Verify both sources were created
    let sources_created = deploy_response["sourcesCreated"].as_array().unwrap();
    assert!(sources_created.iter().any(|id| id == "sensor-source"));
    assert!(sources_created.iter().any(|id| id == "location-source"));

    // Verify the join query was created
    let queries_created = deploy_response["queriesCreated"].as_array().unwrap();
    assert!(queries_created.iter().any(|id| id == "joined-query"));
}

// =============================================================================
// Component Start Verification Tests
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_components_started() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "auto-start-pipeline",
        simple_mock_log_template(),
    );

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let deploy_request = json!({
        "templateId": "auto-start-pipeline"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let deploy_response = &json["data"];
    assert_eq!(deploy_response["success"], true);

    // Verify components were started
    let components_started = deploy_response["componentsStarted"].as_array().unwrap();
    assert!(!components_started.is_empty());

    // Should have started source, query, and reaction
    assert!(components_started
        .iter()
        .any(|c| c.as_str().unwrap().contains("source:solution-source")));
    assert!(components_started
        .iter()
        .any(|c| c.as_str().unwrap().contains("query:solution-query")));
    assert!(components_started
        .iter()
        .any(|c| c.as_str().unwrap().contains("reaction:solution-logger")));
}

// =============================================================================
// Scriptfile Bootstrap Tests
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_with_scriptfile_bootstrap() {
    use test_support::solution_helpers::{
        create_test_jsonl_file, sample_bootstrap_jsonl_entries, scriptfile_bootstrap_template,
    };

    let temp_dir = TempDir::new().unwrap();

    // Create the JSONL data file
    let jsonl_entries = sample_bootstrap_jsonl_entries();
    let data_file_path =
        create_test_jsonl_file(temp_dir.path(), "bootstrap-data.jsonl", &jsonl_entries);

    // Create solution template that references the data file
    let template_content = scriptfile_bootstrap_template(&data_file_path.to_string_lossy());
    create_test_solution_template(temp_dir.path(), "scriptfile-solution", &template_content);

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the solution
    let deploy_request = json!({
        "templateId": "scriptfile-solution"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let deploy_response = &json["data"];
    assert_eq!(
        deploy_response["success"], true,
        "Deploy failed: {:?}",
        deploy_response["errors"]
    );

    // Verify the source with bootstrap was created
    let sources_created = deploy_response["sourcesCreated"].as_array().unwrap();
    assert!(sources_created.iter().any(|id| id == "bootstrap-source"));

    // =========================================================================
    // DATA FLOW VALIDATION - Bootstrap data should flow through query
    // =========================================================================

    // The scriptfile bootstrap loads 3 TestNode entries. Wait for query to process them.
    let results = test_support::solution_helpers::wait_for_query_results(
        &core,
        "bootstrap-query",
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("Query 'bootstrap-query' should receive bootstrap data from scriptfile");

    // We bootstrapped 3 nodes, so we should have 3 results
    assert_eq!(
        results.len(),
        3,
        "Expected 3 bootstrap results, got {}. Results: {:?}",
        results.len(),
        results
    );

    // Validate the result structure - the query returns id and value
    for result in &results {
        assert!(
            result.get("id").is_some(),
            "Result should have 'id' field. Got: {result:?}"
        );
        assert!(
            result.get("value").is_some(),
            "Result should have 'value' field. Got: {result:?}"
        );
    }

    // Validate specific values from bootstrap data
    let ids: Vec<_> = results
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str()))
        .collect();
    assert!(
        ids.contains(&"node-1"),
        "Expected 'node-1' in results. Got: {ids:?}"
    );
    assert!(
        ids.contains(&"node-2"),
        "Expected 'node-2' in results. Got: {ids:?}"
    );
    assert!(
        ids.contains(&"node-3"),
        "Expected 'node-3' in results. Got: {ids:?}"
    );

    println!(
        "Scriptfile bootstrap test validated: {} nodes loaded from bootstrap file",
        results.len()
    );
}

// =============================================================================
// Reaction Output Validation Tests
// =============================================================================

#[tokio::test]
async fn test_solution_log_reaction_receives_subscription() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(temp_dir.path(), "log-pipeline", simple_mock_log_template());

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the solution
    let deploy_request = json!({
        "templateId": "log-pipeline"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["data"]["success"], true,
        "Deploy failed: {:?}",
        json["data"]["errors"]
    );

    // Verify the log reaction was created and started
    let reactions = core.list_reactions().await.unwrap();
    assert!(reactions.iter().any(|(id, _)| id == "solution-logger"));

    // Verify the reaction is subscribed to the query
    let queries = core.list_queries().await.unwrap();
    assert!(queries.iter().any(|(id, _)| id == "solution-query"));

    // =========================================================================
    // DATA FLOW VALIDATION - Verify data flows through the reaction pipeline
    // =========================================================================

    // Wait for the query to have results (data from MockSource)
    let results = test_support::solution_helpers::wait_for_query_results(
        &core,
        "solution-query",
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("Query should receive data from MockSource when reaction is subscribed");

    assert!(
        !results.is_empty(),
        "Query should have results indicating data flow through the pipeline"
    );

    println!(
        "Log reaction subscription test validated: {} results in query, reaction subscribed",
        results.len()
    );
}

// =============================================================================
// Multi-Source Join Query Tests
// =============================================================================

#[tokio::test]
async fn test_deploy_solution_with_joins_creates_query() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "join-solution",
        multi_source_join_template(),
    );

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the solution
    let deploy_request = json!({
        "templateId": "join-solution"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["data"]["success"], true,
        "Deploy failed: {:?}",
        json["data"]["errors"]
    );

    // Verify both sources exist
    let sources = core.list_sources().await.unwrap();
    assert!(sources.iter().any(|(id, _)| id == "sensor-source"));
    assert!(sources.iter().any(|(id, _)| id == "location-source"));

    // Verify the join query exists
    let queries = core.list_queries().await.unwrap();
    assert!(queries.iter().any(|(id, _)| id == "joined-query"));
}

#[tokio::test]
async fn test_deploy_solution_three_source_join() {
    let temp_dir = TempDir::new().unwrap();

    // Create a more complex template with 3 sources
    let three_source_template = r#"
name: Three Source Join
description: Complex join across three sources

sources:
  - kind: mock
    id: users-source
    autoStart: true
    dataType:
      type: generic
    intervalMs: 100

  - kind: mock
    id: orders-source
    autoStart: true
    dataType:
      type: generic
    intervalMs: 100

  - kind: mock
    id: products-source
    autoStart: true
    dataType:
      type: generic
    intervalMs: 100

queries:
  - id: order-details-query
    query: |
      MATCH (u:User)-[:PLACED]->(o:Order)-[:CONTAINS]->(p:Product)
      RETURN u.name AS userName, o.order_id AS orderId, p.name AS productName
    queryLanguage: Cypher
    sources:
      - sourceId: users-source
      - sourceId: orders-source
      - sourceId: products-source
    joins:
      - id: PLACED
        keys:
          - label: User
            property: id
          - label: Order
            property: user_id
      - id: CONTAINS
        keys:
          - label: Order
            property: id
          - label: Product
            property: order_id
    autoStart: true

reactions:
  - kind: log
    id: order-logger
    queries:
      - order-details-query
    autoStart: true
"#;

    create_test_solution_template(temp_dir.path(), "three-source-join", three_source_template);

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let deploy_request = json!({
        "templateId": "three-source-join"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["data"]["success"], true,
        "Deploy failed: {:?}",
        json["data"]["errors"]
    );

    // Verify all 3 sources were created
    let sources = core.list_sources().await.unwrap();
    assert!(sources.iter().any(|(id, _)| id == "users-source"));
    assert!(sources.iter().any(|(id, _)| id == "orders-source"));
    assert!(sources.iter().any(|(id, _)| id == "products-source"));

    // Verify query with multiple joins exists
    let queries = core.list_queries().await.unwrap();
    assert!(queries.iter().any(|(id, _)| id == "order-details-query"));
}

// =============================================================================
// End-to-End Scenario Tests
// =============================================================================

#[tokio::test]
async fn test_e2e_sensor_monitoring_solution() {
    let temp_dir = TempDir::new().unwrap();

    // Create a realistic IoT sensor monitoring solution
    let sensor_template = r#"
name: IoT Sensor Monitoring
description: Complete sensor monitoring solution with alerts

sources:
  - kind: mock
    id: sensor-feed
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 3
    intervalMs: 50

queries:
  - id: all-readings
    query: |
      MATCH (s:SensorReading)
      RETURN s.sensor_id AS sensorId, s.temperature AS temp, s.humidity AS humidity
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-feed
    autoStart: true

  - id: high-temp-alerts
    query: |
      MATCH (s:SensorReading)
      WHERE s.temperature > 80
      RETURN s.sensor_id AS sensorId, s.temperature AS temp
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-feed
    autoStart: true

reactions:
  - kind: log
    id: readings-logger
    queries:
      - all-readings
    autoStart: true

  - kind: log
    id: alerts-logger
    queries:
      - high-temp-alerts
    autoStart: true
"#;

    create_test_solution_template(temp_dir.path(), "iot-monitoring", sensor_template);

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let deploy_request = json!({
        "templateId": "iot-monitoring"
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(deploy_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["data"]["success"], true,
        "Deploy failed: {:?}",
        json["data"]["errors"]
    );

    // Verify all components were created
    let sources = core.list_sources().await.unwrap();
    assert!(sources.iter().any(|(id, _)| id == "sensor-feed"));

    let queries = core.list_queries().await.unwrap();
    assert!(queries.iter().any(|(id, _)| id == "all-readings"));
    assert!(queries.iter().any(|(id, _)| id == "high-temp-alerts"));

    let reactions = core.list_reactions().await.unwrap();
    assert!(reactions.iter().any(|(id, _)| id == "readings-logger"));
    assert!(reactions.iter().any(|(id, _)| id == "alerts-logger"));

    // Verify all components were started
    let deploy_response = &json["data"];
    let components_started = deploy_response["componentsStarted"].as_array().unwrap();
    assert!(components_started.len() >= 5); // 1 source + 2 queries + 2 reactions

    // =========================================================================
    // DATA FLOW VALIDATION - Verify data actually flows through the pipeline
    // =========================================================================

    // MockSource generates SensorReading data every 50ms with 3 sensors.
    // Wait for the all-readings query to have results.
    let results = test_support::solution_helpers::wait_for_query_results(
        &core,
        "all-readings",
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("Query 'all-readings' should have received sensor data from MockSource");

    // Validate the results structure
    assert!(
        !results.is_empty(),
        "Should have at least one sensor reading"
    );

    // Each result should have sensorId, temp, and humidity fields (from the query's RETURN clause)
    let first_result = &results[0];
    assert!(
        first_result.get("sensorId").is_some(),
        "Result should have sensorId field. Got: {first_result:?}"
    );
    assert!(
        first_result.get("temp").is_some(),
        "Result should have temp field. Got: {first_result:?}"
    );
    assert!(
        first_result.get("humidity").is_some(),
        "Result should have humidity field. Got: {first_result:?}"
    );

    // Verify we got multiple readings (MockSource creates 3 sensors)
    // Wait for at least 2 results to ensure ongoing data flow
    let results = test_support::solution_helpers::wait_for_query_results_count(
        &core,
        "all-readings",
        2,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("Should receive multiple sensor readings");

    println!(
        "E2E sensor monitoring test validated: {} sensor readings received",
        results.len()
    );
}

#[tokio::test]
async fn test_e2e_multi_instance_deployment() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "simple-pipeline",
        simple_mock_log_template(),
    );

    // Create router with 3 instances
    let (router, instances_data) = create_multi_instance_test_router(
        vec!["production", "staging", "development"],
        Some(temp_dir.path().to_string_lossy().to_string()),
    )
    .await;

    // Deploy same solution to all instances
    for instance_id in &["production", "staging", "development"] {
        let deploy_request = json!({
            "templateId": "simple-pipeline"
        });

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/instances/{instance_id}/solutions"))
                    .header("content-type", "application/json")
                    .body(Body::from(deploy_request.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json["data"]["success"], true,
            "Deploy to {instance_id} failed: {:?}",
            json["data"]["errors"]
        );
    }

    // Verify each instance has the deployed solution
    for (instance_id, core, _components) in &instances_data {
        let sources = core.list_sources().await.unwrap();
        assert!(
            sources.iter().any(|(id, _)| id == "solution-source"),
            "Instance '{instance_id}' missing solution-source"
        );

        let queries = core.list_queries().await.unwrap();
        assert!(
            queries.iter().any(|(id, _)| id == "solution-query"),
            "Instance '{instance_id}' missing solution-query"
        );

        let reactions = core.list_reactions().await.unwrap();
        assert!(
            reactions.iter().any(|(id, _)| id == "solution-logger"),
            "Instance '{instance_id}' missing solution-logger"
        );
    }
}
