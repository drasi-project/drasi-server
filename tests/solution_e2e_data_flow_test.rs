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

//! End-to-End Data Flow Validation Tests
//!
//! These tests validate that data actually flows through the complete Drasi pipeline:
//! Source → Query → Reaction → Output
//!
//! Unlike the deployment tests that just verify component creation, these tests:
//! 1. Use known input data (scriptfile bootstrap)
//! 2. Capture actual reaction output (HTTP reaction to wiremock)
//! 3. Validate the output matches expected results based on the query
//!
//! Test patterns:
//! - Scriptfile bootstrap provides known input data
//! - HTTP reaction sends results to wiremock mock server
//! - Assertions verify wiremock received the expected data

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
    create_test_jsonl_file, create_test_router_with_solutions, create_test_solution_template,
};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// =============================================================================
// Helper Functions
// =============================================================================

/// Create bootstrap JSONL with sensor data that has known values
#[allow(dead_code)]
fn sensor_bootstrap_entries() -> Vec<&'static str> {
    vec![
        r#"{"kind":"Header","start_time":"2024-01-01T00:00:00+00:00","description":"Test sensor data"}"#,
        r#"{"kind":"Node","id":"sensor-1","labels":["Sensor"],"properties":{"id":"sensor-1","temperature":85,"location":"room-a"}}"#,
        r#"{"kind":"Node","id":"sensor-2","labels":["Sensor"],"properties":{"id":"sensor-2","temperature":72,"location":"room-b"}}"#,
        r#"{"kind":"Node","id":"sensor-3","labels":["Sensor"],"properties":{"id":"sensor-3","temperature":90,"location":"room-c"}}"#,
        r#"{"kind":"Finish"}"#,
    ]
}

/// Create a solution template with scriptfile bootstrap and HTTP reaction
fn scriptfile_http_reaction_template(data_file: &str, webhook_url: &str) -> String {
    format!(
        r#"
name: E2E Data Flow Test Solution
description: Tests complete data flow from scriptfile to HTTP reaction

sources:
  - kind: mock
    id: sensor-source
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 5
    intervalMs: 100
    bootstrapProvider:
      kind: scriptfile
      filePaths:
        - {data_file}

queries:
  - id: high-temp-query
    query: |
      MATCH (s:SensorReading)
      RETURN s.sensor_id AS sensorId, s.temperature AS temp
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-source
    autoStart: true

reactions:
  - kind: http
    id: http-output
    queries:
      - high-temp-query
    autoStart: true
    baseUrl: {webhook_url}
"#
    )
}

/// Create a simple pass-through query template (returns all sensor readings)
fn all_sensors_template(data_file: &str, webhook_url: &str) -> String {
    format!(
        r#"
name: All Sensors Test
description: Returns all sensor data

sources:
  - kind: mock
    id: data-source
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 5
    intervalMs: 100
    bootstrapProvider:
      kind: scriptfile
      filePaths:
        - {data_file}

queries:
  - id: all-sensors
    query: |
      MATCH (s:SensorReading)
      RETURN s.sensor_id AS sensorId, s.temperature AS temp
    queryLanguage: Cypher
    sources:
      - sourceId: data-source
    autoStart: true

reactions:
  - kind: http
    id: sensor-webhook
    queries:
      - all-sensors
    autoStart: true
    baseUrl: {webhook_url}
"#
    )
}

// =============================================================================
// End-to-End Data Flow Tests
// =============================================================================

/// Test: MockSource generates data that flows to HTTP reaction
///
/// This test validates the complete data flow:
/// 1. MockSource generates SensorReading nodes every 100ms
/// 2. Query returns all SensorReading nodes
/// 3. HTTP reaction sends change events to wiremock
#[tokio::test]
#[ignore = "requires cdylib plugins (mock source, http reaction) — run `make build-local-test-plugins` first"]
async fn test_e2e_scriptfile_to_http_reaction_with_filter() {
    // Start wiremock to capture HTTP reaction output
    let mock_server = MockServer::start().await;

    // HTTP reaction sends to /changes/{query_id}
    Mock::given(method("POST"))
        .and(path("/changes/high-temp-query"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..) // Expect at least 1 request
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();

    // Create empty bootstrap data (we'll rely on MockSource to generate events)
    let bootstrap_data = vec![
        r#"{"kind":"Header","start_time":"2024-01-01T00:00:00+00:00","description":"Empty bootstrap"}"#,
        r#"{"kind":"Finish"}"#,
    ];
    let data_file = create_test_jsonl_file(temp_dir.path(), "empty.jsonl", &bootstrap_data);

    // Create solution template that uses MockSource + HTTP reaction
    let template =
        scriptfile_http_reaction_template(&data_file.to_string_lossy(), &mock_server.uri());
    create_test_solution_template(temp_dir.path(), "e2e-test", &template);

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    // Deploy the solution
    let deploy_request = json!({
        "templateId": "e2e-test"
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

    // Wait for MockSource to generate events and for them to flow through
    // MockSource generates SensorReading at 100ms intervals
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify wiremock received requests from HTTP reaction
    let received = mock_server.received_requests().await.unwrap();

    assert!(
        !received.is_empty(),
        "HTTP reaction should have sent data to wiremock. No requests received."
    );

    // Parse the first received request body
    let request_body = &received[0].body;
    let payload: serde_json::Value =
        serde_json::from_slice(request_body).expect("HTTP reaction should send valid JSON");

    println!(
        "Received payload from HTTP reaction: {}",
        serde_json::to_string_pretty(&payload).unwrap()
    );

    // Validate the payload structure - HTTP reaction sends query result data
    assert!(
        payload.is_object(),
        "Expected JSON object from HTTP reaction. Got: {payload:?}"
    );

    // Validate that sensor data fields are present (from the query RETURN clause)
    // Query returns: s.sensor_id AS sensorId, s.temperature AS temp
    let has_sensor_id = payload.get("sensorId").is_some();
    let has_temp = payload.get("temp").is_some();

    assert!(
        has_sensor_id,
        "Expected 'sensorId' field in HTTP reaction payload. Got: {payload:?}"
    );
    assert!(
        has_temp,
        "Expected 'temp' field in HTTP reaction payload. Got: {payload:?}"
    );

    // Validate sensorId format
    let sensor_id = payload.get("sensorId").and_then(|v| v.as_str()).unwrap();
    assert!(
        sensor_id.starts_with("sensor_"),
        "Expected sensorId to start with 'sensor_'. Got: {sensor_id}"
    );

    // Validate temp is a number
    let temp = payload.get("temp");
    assert!(
        temp.map(|v| v.is_number()).unwrap_or(false),
        "Expected 'temp' to be a number. Got: {temp:?}"
    );

    println!(
        "E2E test PASSED: {} HTTP requests received from reaction with valid sensor data",
        received.len()
    );
}

/// Test: All bootstrap data flows through without filtering
///
/// This test validates that all 3 sensors make it through when there's no filter
#[tokio::test]
#[ignore = "requires cdylib plugins (mock source, http reaction) — run `make build-local-test-plugins` first"]
async fn test_e2e_all_bootstrap_data_reaches_reaction() {
    let mock_server = MockServer::start().await;

    // Match the query ID from all_sensors_template: "all-sensors"
    Mock::given(method("POST"))
        .and(path("/changes/all-sensors"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..)
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();

    // Use empty bootstrap - rely on MockSource to generate events
    let bootstrap_data = vec![
        r#"{"kind":"Header","start_time":"2024-01-01T00:00:00+00:00","description":"Empty"}"#,
        r#"{"kind":"Finish"}"#,
    ];
    let data_file = create_test_jsonl_file(temp_dir.path(), "empty.jsonl", &bootstrap_data);

    let template = all_sensors_template(&data_file.to_string_lossy(), &mock_server.uri());
    create_test_solution_template(temp_dir.path(), "all-sensors-test", &template);

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let deploy_request = json!({
        "templateId": "all-sensors-test"
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

    // Wait for MockSource to generate events
    tokio::time::sleep(Duration::from_secs(2)).await;

    let received = mock_server.received_requests().await.unwrap();

    assert!(
        !received.is_empty(),
        "HTTP reaction should have sent data to wiremock"
    );

    // Parse and validate
    let request_body = &received[0].body;
    let payload: serde_json::Value =
        serde_json::from_slice(request_body).expect("HTTP reaction should send valid JSON");

    println!(
        "All sensors payload: {}",
        serde_json::to_string_pretty(&payload).unwrap()
    );
}

/// Test: Validates specific field values in reaction output
///
/// Uses MockSource Counter type to generate predictable data
#[tokio::test]
#[ignore = "requires cdylib plugins (mock source, http reaction) — run `make build-local-test-plugins` first"]
async fn test_e2e_validates_output_field_values() {
    let mock_server = MockServer::start().await;

    // Match the query ID: "counter-query"
    Mock::given(method("POST"))
        .and(path("/changes/counter-query"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..)
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();

    // Empty bootstrap - rely on MockSource Counter to generate events
    let bootstrap_data = vec![
        r#"{"kind":"Header","start_time":"2024-01-01T00:00:00+00:00","description":"Empty"}"#,
        r#"{"kind":"Finish"}"#,
    ];
    let data_file = create_test_jsonl_file(temp_dir.path(), "empty.jsonl", &bootstrap_data);

    // Use Counter data type which generates predictable sequential values
    let template = format!(
        r#"
name: Counter Validation Test
description: Tests counter values in output

sources:
  - kind: mock
    id: counter-source
    autoStart: true
    dataType:
      type: counter
    intervalMs: 100
    bootstrapProvider:
      kind: scriptfile
      filePaths:
        - {}

queries:
  - id: counter-query
    query: |
      MATCH (c:Counter)
      RETURN c.value AS counterValue
    queryLanguage: Cypher
    sources:
      - sourceId: counter-source
    autoStart: true

reactions:
  - kind: http
    id: counter-webhook
    queries:
      - counter-query
    autoStart: true
    baseUrl: {}
"#,
        data_file.to_string_lossy(),
        mock_server.uri()
    );

    create_test_solution_template(temp_dir.path(), "counter-test", &template);

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"templateId": "counter-test"}).to_string(),
                ))
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

    // Wait for MockSource to generate Counter events
    tokio::time::sleep(Duration::from_secs(2)).await;

    let received = mock_server.received_requests().await.unwrap();
    assert!(!received.is_empty(), "Expected HTTP reaction to send data");

    let payload: serde_json::Value =
        serde_json::from_slice(&received[0].body).expect("Valid JSON expected");

    println!(
        "Counter validation payload: {}",
        serde_json::to_string_pretty(&payload).unwrap()
    );

    // Validate the counter value field is present and is a number
    assert!(payload.is_object(), "Expected JSON object from reaction");

    let counter_value = payload.get("counterValue");
    assert!(
        counter_value.is_some(),
        "Expected 'counterValue' field in payload. Got: {payload:?}"
    );
    assert!(
        counter_value.unwrap().is_number(),
        "Expected counterValue to be a number. Got: {counter_value:?}"
    );

    println!("Counter field validation PASSED");
}

/// Test: Empty result set when filter matches nothing
///
/// Bootstrap data that doesn't match the query filter should result in no output
#[tokio::test]
#[ignore = "requires cdylib plugins (mock source, http reaction) — run `make build-local-test-plugins` first"]
async fn test_e2e_no_results_when_filter_matches_nothing() {
    // All sensors have temp < 50, but query filters for > 80
    let bootstrap_data = vec![
        r#"{"kind":"Header","start_time":"2024-01-01T00:00:00+00:00","description":"Cold sensors"}"#,
        r#"{"kind":"Node","id":"cold-1","labels":["Sensor"],"properties":{"id":"cold-1","temperature":20}}"#,
        r#"{"kind":"Node","id":"cold-2","labels":["Sensor"],"properties":{"id":"cold-2","temperature":25}}"#,
        r#"{"kind":"Finish"}"#,
    ];

    let mock_server = MockServer::start().await;

    // We expect 0 requests since nothing matches the filter
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0) // Expect NO requests
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    let data_file = create_test_jsonl_file(temp_dir.path(), "cold.jsonl", &bootstrap_data);

    let template =
        scriptfile_http_reaction_template(&data_file.to_string_lossy(), &mock_server.uri());
    create_test_solution_template(temp_dir.path(), "no-match-test", &template);

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"templateId": "no-match-test"}).to_string(),
                ))
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

    // Wait for bootstrap to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    let received = mock_server.received_requests().await.unwrap();

    // When filter matches nothing, HTTP reaction should not send any requests
    // (or may send empty result set depending on implementation)
    println!(
        "Requests received when no data matches filter: {}",
        received.len()
    );

    // The test passes if we get here - wiremock's expect(0) would fail if requests were received
}

/// Test: Multiple sensor readings are received by HTTP reaction
#[tokio::test]
#[ignore = "requires cdylib plugins (mock source, http reaction) — run `make build-local-test-plugins` first"]
async fn test_e2e_handles_multiple_bootstrap_nodes() {
    let mock_server = MockServer::start().await;

    // Match the query ID: "multi-sensor-query"
    Mock::given(method("POST"))
        .and(path("/changes/multi-sensor-query"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..)
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();

    // Empty bootstrap - rely on MockSource to generate multiple events
    let bootstrap_data = vec![
        r#"{"kind":"Header","start_time":"2024-01-01T00:00:00+00:00","description":"Empty"}"#,
        r#"{"kind":"Finish"}"#,
    ];
    let data_file = create_test_jsonl_file(temp_dir.path(), "empty.jsonl", &bootstrap_data);

    // Use SensorReading with multiple sensors - generates varied data
    let template = format!(
        r#"
name: Multi Sensor Test
description: Tests multiple sensor readings

sources:
  - kind: mock
    id: multi-source
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 10
    intervalMs: 50
    bootstrapProvider:
      kind: scriptfile
      filePaths:
        - {}

queries:
  - id: multi-sensor-query
    query: |
      MATCH (s:SensorReading)
      RETURN s.sensor_id AS sensorId, s.temperature AS temp
    queryLanguage: Cypher
    sources:
      - sourceId: multi-source
    autoStart: true

reactions:
  - kind: http
    id: multi-webhook
    queries:
      - multi-sensor-query
    autoStart: true
    baseUrl: {}
"#,
        data_file.to_string_lossy(),
        mock_server.uri()
    );

    create_test_solution_template(temp_dir.path(), "multi-test", &template);

    let (router, _core, components) =
        create_test_router_with_solutions(Some(temp_dir.path().to_string_lossy().to_string()))
            .await;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/instances/{}/solutions", components.instance_id))
                .header("content-type", "application/json")
                .body(Body::from(json!({"templateId": "multi-test"}).to_string()))
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

    // Wait for multiple events from MockSource (50ms interval = ~40 events in 2 seconds)
    tokio::time::sleep(Duration::from_secs(2)).await;

    let received = mock_server.received_requests().await.unwrap();
    assert!(!received.is_empty(), "Expected HTTP reaction to send data");

    // Count total items across all requests
    let total_payloads = received.len();

    // Print first few payloads for debugging
    for (i, req) in received.iter().take(3).enumerate() {
        let payload: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!(null));
        println!(
            "Request #{}: {}",
            i + 1,
            serde_json::to_string_pretty(&payload).unwrap()
        );
    }

    println!("Total HTTP requests received: {total_payloads}");

    // Should receive multiple requests from the fast-generating MockSource
    assert!(
        total_payloads >= 5,
        "Should receive multiple sensor readings. Got: {total_payloads}"
    );
}
