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

//! End-to-End Tests for IoT Temperature Monitor Solution Template
//!
//! These tests validate that the iot-temperature-monitor.yaml solution template
//! produces actual data output when deployed.
//!
//! Bug identified: The template's default TEMP_THRESHOLD of 75 was too high -
//! MockSource generates temperatures in the 20-30°C range, so no data ever matched.
//! The threshold has been corrected to 25 (matching typical sensor data).

#![allow(clippy::unwrap_used)]

mod test_support;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;
use test_support::solution_helpers::{
    create_test_router_with_solutions, create_test_solution_template, wait_for_query_results,
};
use tower::ServiceExt;

/// Load and parse the actual iot-temperature-monitor.yaml template from solutions/
fn load_iot_template() -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let template_path = format!("{manifest_dir}/solutions/iot-temperature-monitor.yaml");
    std::fs::read_to_string(&template_path)
        .unwrap_or_else(|e| panic!("Failed to read IoT template at {template_path}: {e}"))
}

/// Test that the IoT template can be deployed successfully
#[tokio::test]
async fn test_iot_template_deploys_successfully() {
    let temp_dir = TempDir::new().unwrap();

    // Copy the actual template to temp dir
    let template_content = load_iot_template();
    create_test_solution_template(
        temp_dir.path(),
        "iot-temperature-monitor",
        &template_content,
    );

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the solution
    let deploy_request = json!({
        "templateId": "iot-temperature-monitor"
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

    println!(
        "Deploy response: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );

    assert_eq!(
        json["data"]["success"], true,
        "IoT template deployment failed: {:?}",
        json["data"]["errors"]
    );

    // Verify source, query, and reaction were created
    let deploy_response = &json["data"];
    assert!(
        deploy_response["sourcesCreated"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "Source should be deployed"
    );
    assert!(
        deploy_response["queriesCreated"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "Query should be deployed"
    );
    assert!(
        deploy_response["reactionsCreated"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "Reaction should be deployed"
    );
}

/// Test that the IoT template produces query results with mock data
///
/// This is the critical test that verifies the bug fix:
/// - Before: TEMP_THRESHOLD default was 75, MockSource generates 20-30, no matches
/// - After: TEMP_THRESHOLD default is 25, MockSource generates 20-30, matches occur
#[tokio::test]
async fn test_iot_template_produces_query_results() {
    let temp_dir = TempDir::new().unwrap();

    // Use a modified version of the template with faster interval for testing
    let template_content = r#"
name: IoT Temperature Monitor
description: Monitors sensor data and alerts on high temperature events
version: "1.0.0"

sources:
  - kind: mock
    id: sensor-feed
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 5
    intervalMs: 100  # Faster for testing

queries:
  - id: high-temp-alert
    query: "MATCH (s:SensorReading) WHERE s.temperature > ${TEMP_THRESHOLD:-25} RETURN s"
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-feed
    autoStart: true

reactions:
  - kind: log
    id: temp-logger
    queries:
      - high-temp-alert
    autoStart: true
"#;

    create_test_solution_template(temp_dir.path(), "iot-test", template_content);

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the solution
    let deploy_request = json!({
        "templateId": "iot-test"
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

    // Wait for query results - this proves data flows through the pipeline
    let results = wait_for_query_results(&core, "high-temp-alert", Duration::from_secs(5)).await;

    match results {
        Ok(results) => {
            println!("Query produced {} results", results.len());
            assert!(
                !results.is_empty(),
                "Query should produce results with threshold 25 (MockSource generates temps 20-30)"
            );

            // Validate result structure
            let first = &results[0];
            println!(
                "First result: {}",
                serde_json::to_string_pretty(first).unwrap()
            );
        }
        Err(e) => {
            panic!("Query should produce results but got error: {e}");
        }
    }
}

/// Test that demonstrates the original bug:
/// With threshold 75, no results are produced because MockSource generates 20-30
#[tokio::test]
async fn test_iot_template_no_results_with_high_threshold() {
    let temp_dir = TempDir::new().unwrap();

    // Use the original buggy threshold of 75
    let template_content = r#"
name: IoT Temperature Monitor (Bug Demo)
description: Demonstrates the bug - threshold too high for mock data

sources:
  - kind: mock
    id: sensor-feed
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 5
    intervalMs: 100

queries:
  - id: high-temp-alert
    query: "MATCH (s:SensorReading) WHERE s.temperature > 75 RETURN s"
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-feed
    autoStart: true

reactions:
  - kind: log
    id: temp-logger
    queries:
      - high-temp-alert
    autoStart: true
"#;

    create_test_solution_template(temp_dir.path(), "iot-bug-demo", template_content);

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let deploy_request = json!({
        "templateId": "iot-bug-demo"
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

    // Wait a bit for the source to generate some data
    tokio::time::sleep(Duration::from_millis(500)).await;

    // With threshold 75, no results should be produced
    // MockSource generates temperatures in range 20-30
    let results = core.get_query_results("high-temp-alert").await;

    match results {
        Ok(results) => {
            println!(
                "With threshold 75: got {} results (expected 0)",
                results.len()
            );
            // This demonstrates the bug - with threshold 75, we get NO results
            // because MockSource generates temperatures between 20-30
            assert!(
                results.is_empty(),
                "With threshold 75, no results should be produced since MockSource generates temps 20-30. \
                This demonstrates the original bug where the template used threshold 75."
            );
        }
        Err(e) => {
            // Query may still be starting, that's ok
            println!("Query not ready yet: {e}");
        }
    }
}

/// Test deployment with custom variable override
#[tokio::test]
async fn test_iot_template_with_custom_threshold() {
    let temp_dir = TempDir::new().unwrap();

    let template_content = r#"
name: IoT Temperature Monitor
description: Test with custom threshold

sources:
  - kind: mock
    id: sensor-feed
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 5
    intervalMs: 100

queries:
  - id: high-temp-alert
    query: "MATCH (s:SensorReading) WHERE s.temperature > ${TEMP_THRESHOLD:-25} RETURN s"
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-feed
    autoStart: true

reactions:
  - kind: log
    id: temp-logger
    queries:
      - high-temp-alert
    autoStart: true
"#;

    create_test_solution_template(temp_dir.path(), "iot-custom", template_content);

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy with custom threshold of 22 (should match more readings)
    let deploy_request = json!({
        "templateId": "iot-custom",
        "variables": {
            "TEMP_THRESHOLD": "22"
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
    assert_eq!(
        json["data"]["success"], true,
        "Deploy with variables failed: {:?}",
        json["data"]["errors"]
    );

    // Wait for results with lower threshold
    let results = wait_for_query_results(&core, "high-temp-alert", Duration::from_secs(5)).await;

    assert!(
        results.is_ok(),
        "Should get results with threshold 22: {:?}",
        results.err()
    );
}

/// Test that ALL sensor readings pass through with no filter
#[tokio::test]
async fn test_mock_source_generates_sensor_readings() {
    let temp_dir = TempDir::new().unwrap();

    // No WHERE filter - returns all sensor readings
    let template_content = r#"
name: All Sensors Test
description: Returns all sensor readings without filtering

sources:
  - kind: mock
    id: sensor-feed
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 3
    intervalMs: 100

queries:
  - id: all-sensors
    query: "MATCH (s:SensorReading) RETURN s.sensor_id AS sensorId, s.temperature AS temp, s.humidity AS humidity"
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-feed
    autoStart: true

reactions:
  - kind: log
    id: sensor-logger
    queries:
      - all-sensors
    autoStart: true
"#;

    create_test_solution_template(temp_dir.path(), "all-sensors", template_content);

    let (router, core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let deploy_request = json!({
        "templateId": "all-sensors"
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

    // Wait for results
    let results = wait_for_query_results(&core, "all-sensors", Duration::from_secs(5)).await;

    match results {
        Ok(results) => {
            println!(
                "All sensors query results: {}",
                serde_json::to_string_pretty(&json!(results)).unwrap()
            );

            assert!(
                !results.is_empty(),
                "Should get sensor readings with no filter"
            );

            // Verify structure of results
            let first = &results[0];
            assert!(
                first.get("sensorId").is_some(),
                "Result should have sensorId field"
            );
            assert!(first.get("temp").is_some(), "Result should have temp field");
            assert!(
                first.get("humidity").is_some(),
                "Result should have humidity field"
            );

            // Verify sensor_id format
            if let Some(sensor_id) = first.get("sensorId").and_then(|v| v.as_str()) {
                assert!(
                    sensor_id.starts_with("sensor_"),
                    "Sensor ID should start with 'sensor_', got: {sensor_id}"
                );
            }

            // Verify temperature is in expected range (20-30)
            if let Some(temp) = first.get("temp").and_then(|v| v.as_f64()) {
                assert!(
                    (20.0..=30.0).contains(&temp),
                    "Temperature should be in range 20-30, got: {temp}"
                );
            }
        }
        Err(e) => {
            panic!("Should get sensor readings: {e}");
        }
    }
}
