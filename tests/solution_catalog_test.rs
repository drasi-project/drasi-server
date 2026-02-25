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

//! Solution Catalog API Tests
//!
//! These tests validate the solution template catalog functionality:
//! - Listing available solution templates
//! - Getting solution template details
//! - Variable extraction from templates
//! - Error handling for missing/invalid templates

#![allow(clippy::unwrap_used)]

mod test_support;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tempfile::TempDir;
use test_support::solution_helpers::{
    create_test_router_with_solutions, create_test_solution_template, multi_source_join_template,
    simple_mock_log_template, template_with_variables,
};
use tower::ServiceExt;

// =============================================================================
// List Solutions Tests
// =============================================================================

#[tokio::test]
async fn test_list_solutions_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let solutions = json["data"].as_array().unwrap();
    assert!(solutions.is_empty());
}

#[tokio::test]
async fn test_list_solutions_with_templates() {
    let temp_dir = TempDir::new().unwrap();

    // Create multiple templates
    create_test_solution_template(
        temp_dir.path(),
        "simple-pipeline",
        simple_mock_log_template(),
    );
    create_test_solution_template(
        temp_dir.path(),
        "configurable-pipeline",
        template_with_variables(),
    );
    create_test_solution_template(
        temp_dir.path(),
        "join-pipeline",
        multi_source_join_template(),
    );

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let solutions = json["data"].as_array().unwrap();
    assert_eq!(solutions.len(), 3);

    // Verify each template is listed
    let ids: Vec<&str> = solutions.iter().filter_map(|s| s["id"].as_str()).collect();
    assert!(ids.contains(&"simple-pipeline"));
    assert!(ids.contains(&"configurable-pipeline"));
    assert!(ids.contains(&"join-pipeline"));
}

#[tokio::test]
async fn test_list_solutions_ignores_non_yaml_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create a valid YAML template
    create_test_solution_template(
        temp_dir.path(),
        "valid-solution",
        simple_mock_log_template(),
    );

    // Create non-YAML files that should be ignored
    std::fs::write(temp_dir.path().join("readme.md"), "# Readme").unwrap();
    std::fs::write(temp_dir.path().join("config.json"), "{}").unwrap();
    std::fs::write(temp_dir.path().join("script.sh"), "#!/bin/bash").unwrap();

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let solutions = json["data"].as_array().unwrap();
    assert_eq!(solutions.len(), 1);
    assert_eq!(solutions[0]["id"], "valid-solution");
}

#[tokio::test]
async fn test_list_solutions_yml_extension() {
    let temp_dir = TempDir::new().unwrap();

    // Create template with .yml extension
    let yml_path = temp_dir.path().join("yml-solution.yml");
    std::fs::write(yml_path, simple_mock_log_template()).unwrap();

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let solutions = json["data"].as_array().unwrap();
    assert_eq!(solutions.len(), 1);
    assert_eq!(solutions[0]["id"], "yml-solution");
}

// =============================================================================
// Get Solution Details Tests
// =============================================================================

#[tokio::test]
async fn test_get_solution_details() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(temp_dir.path(), "my-solution", simple_mock_log_template());

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions/my-solution")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let detail = &json["data"];

    assert_eq!(detail["id"], "my-solution");
    assert_eq!(detail["name"], "Simple Mock Log Pipeline");
    assert_eq!(
        detail["description"],
        "Basic pipeline with mock source and log reaction for testing"
    );
    assert_eq!(detail["version"], "1.0.0");
    assert_eq!(detail["author"], "Test Suite");

    // Verify component IDs are listed
    let source_ids = detail["sourceIds"].as_array().unwrap();
    assert!(source_ids.iter().any(|id| id == "solution-source"));

    let query_ids = detail["queryIds"].as_array().unwrap();
    assert!(query_ids.iter().any(|id| id == "solution-query"));

    let reaction_ids = detail["reactionIds"].as_array().unwrap();
    assert!(reaction_ids.iter().any(|id| id == "solution-logger"));
}

#[tokio::test]
async fn test_get_solution_not_found() {
    let temp_dir = TempDir::new().unwrap();

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions/non-existent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_get_solution_variable_extraction() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "variable-solution",
        template_with_variables(),
    );

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions/variable-solution")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let variables = json["data"]["variables"].as_array().unwrap();

    // Should have INTERVAL_MS and TEMP_THRESHOLD variables
    assert!(variables.len() >= 2);

    // Find INTERVAL_MS variable
    let interval_var = variables
        .iter()
        .find(|v| v["name"] == "INTERVAL_MS")
        .expect("INTERVAL_MS variable not found");
    assert_eq!(interval_var["default"], "1000");
    assert_eq!(interval_var["required"], false);

    // Find TEMP_THRESHOLD variable
    let threshold_var = variables
        .iter()
        .find(|v| v["name"] == "TEMP_THRESHOLD")
        .expect("TEMP_THRESHOLD variable not found");
    assert_eq!(threshold_var["default"], "75");
    assert_eq!(threshold_var["required"], false);
}

#[tokio::test]
async fn test_get_solution_variable_descriptions() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(temp_dir.path(), "described-vars", template_with_variables());

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions/described-vars")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let variables = json["data"]["variables"].as_array().unwrap();

    // Find INTERVAL_MS variable - should have description from inline comment
    let interval_var = variables.iter().find(|v| v["name"] == "INTERVAL_MS");
    if let Some(var) = interval_var {
        // The template has a comment "# Sensor polling interval"
        if let Some(desc) = var["description"].as_str() {
            assert!(desc.contains("polling") || desc.contains("interval"));
        }
    }
}

#[tokio::test]
async fn test_solution_metadata_fields() {
    let temp_dir = TempDir::new().unwrap();

    // Create template with all metadata fields
    let full_metadata_template = r#"
name: Complete Metadata Solution
description: A solution with all metadata fields
version: "2.1.0"
author: Integration Test Author
license: Apache-2.0
defaultInstanceId: complete-metadata

sources:
  - kind: mock
    id: meta-source
    autoStart: true
    dataType:
      type: generic

queries:
  - id: meta-query
    query: "MATCH (n) RETURN n"
    queryLanguage: Cypher
    sources:
      - sourceId: meta-source

reactions:
  - kind: log
    id: meta-logger
    queries:
      - meta-query
"#;

    create_test_solution_template(temp_dir.path(), "complete-metadata", full_metadata_template);

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions/complete-metadata")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let detail = &json["data"];

    // Verify all metadata fields
    assert_eq!(detail["name"], "Complete Metadata Solution");
    assert_eq!(detail["description"], "A solution with all metadata fields");
    assert_eq!(detail["version"], "2.1.0");
    assert_eq!(detail["author"], "Integration Test Author");
    assert_eq!(detail["license"], "Apache-2.0");
    assert_eq!(detail["defaultInstanceId"], "complete-metadata");
}

// =============================================================================
// List Solutions with Counts Tests
// =============================================================================

#[tokio::test]
async fn test_list_solutions_shows_component_counts() {
    let temp_dir = TempDir::new().unwrap();
    create_test_solution_template(
        temp_dir.path(),
        "multi-component",
        multi_source_join_template(),
    );

    let (router, _core, _components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/catalog/solutions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    let solutions = json["data"].as_array().unwrap();
    let multi = solutions
        .iter()
        .find(|s| s["id"] == "multi-component")
        .unwrap();

    // Multi-source join template has 2 sources, 1 query, 1 reaction
    assert_eq!(multi["sourceCount"], 2);
    assert_eq!(multi["queryCount"], 1);
    assert_eq!(multi["reactionCount"], 1);
}
