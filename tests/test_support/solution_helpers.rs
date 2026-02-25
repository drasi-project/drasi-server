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

//! Solution testing helpers for integration tests.
//!
//! This module provides utilities for:
//! - Creating test solution templates
//! - Setting up multi-instance test routers
//! - Polling reaction logs for expected output
//! - Creating mock HTTP servers for HTTP source/reaction testing

use axum::Router;
use drasi_lib::DrasiLib;
use drasi_server::api::v1::handlers;
use drasi_server::api::v1::routes::build_v1_router;
use drasi_server::instance_registry::InstanceRegistry;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, Instant};

use super::mock_components::{create_mock_reaction, create_mock_source, MockReaction, MockSource};

/// Registry of test components for a single instance
pub struct TestInstanceComponents {
    pub instance_id: String,
    pub source: MockSource,
    pub reaction: MockReaction,
}

/// Create a test solution template file in the specified directory
pub fn create_test_solution_template(dir: &Path, name: &str, content: &str) {
    let path = dir.join(format!("{name}.yaml"));
    std::fs::write(path, content).expect("Failed to write test solution template");
}

/// Create a simple mock-to-log solution template
pub fn simple_mock_log_template() -> &'static str {
    r#"
name: Simple Mock Log Pipeline
description: Basic pipeline with mock source and log reaction for testing
version: "1.0.0"
author: Test Suite
defaultInstanceId: simple-pipeline

sources:
  - kind: mock
    id: solution-source
    autoStart: true
    dataType:
      type: generic
    intervalMs: 100

queries:
  - id: solution-query
    query: "MATCH (n) RETURN n"
    queryLanguage: Cypher
    sources:
      - sourceId: solution-source
    autoStart: true

reactions:
  - kind: log
    id: solution-logger
    queries:
      - solution-query
    autoStart: true
"#
}

/// Create a solution template with variables
pub fn template_with_variables() -> &'static str {
    r#"
name: Configurable Pipeline
description: Pipeline with configurable threshold
version: "1.0.0"

sources:
  - kind: mock
    id: sensor-source
    autoStart: true
    dataType:
      type: sensorReading
    intervalMs: ${INTERVAL_MS:-1000}  # Sensor polling interval

queries:
  - id: threshold-query
    query: "MATCH (s:SensorReading) WHERE s.temperature > ${TEMP_THRESHOLD:-75} RETURN s"
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-source
    autoStart: true

reactions:
  - kind: log
    id: alert-logger
    queries:
      - threshold-query
    autoStart: true
"#
}

/// Create a solution template with scriptfile bootstrap
pub fn scriptfile_bootstrap_template(data_file_path: &str) -> String {
    format!(
        r#"
name: Scriptfile Bootstrap Solution
description: Solution using scriptfile bootstrap for initial data

sources:
  - kind: mock
    id: bootstrap-source
    autoStart: true
    dataType:
      type: generic
    intervalMs: 1000
    bootstrapProvider:
      kind: scriptfile
      filePaths:
        - {data_file_path}

queries:
  - id: bootstrap-query
    query: "MATCH (n:TestNode) RETURN n.id AS id, n.value AS value"
    queryLanguage: Cypher
    sources:
      - sourceId: bootstrap-source
    autoStart: true

reactions:
  - kind: log
    id: bootstrap-logger
    queries:
      - bootstrap-query
    autoStart: true
"#
    )
}

/// Create a solution template with multiple sources for join testing
pub fn multi_source_join_template() -> &'static str {
    r#"
name: Multi-Source Join Solution
description: Solution with query joining two sources

sources:
  - kind: mock
    id: sensor-source
    autoStart: true
    dataType:
      type: sensorReading
    intervalMs: 100

  - kind: mock
    id: location-source
    autoStart: true
    dataType:
      type: generic
    intervalMs: 100

queries:
  - id: joined-query
    query: |
      MATCH (s:SensorReading)-[:LOCATED_AT]->(l:Location)
      RETURN s.sensor_id AS sensorId, l.name AS locationName
    queryLanguage: Cypher
    sources:
      - sourceId: sensor-source
      - sourceId: location-source
    joins:
      - id: LOCATED_AT
        keys:
          - label: SensorReading
            property: location_id
          - label: Location
            property: id
    autoStart: true

reactions:
  - kind: log
    id: joined-logger
    queries:
      - joined-query
    autoStart: true
"#
}

/// Create a test router with a single DrasiLib instance and solutions directory
pub async fn create_test_router_with_solutions(
    solutions_dir: Option<String>,
) -> (Router, Arc<DrasiLib>, TestInstanceComponents) {
    let instance_id = "test-instance";

    // Create mock components
    let test_source = create_mock_source("test-source");
    let test_reaction = create_mock_reaction("test-reaction", vec!["test-query".to_string()]);

    // Build DrasiLib
    let core = DrasiLib::builder()
        .with_id(instance_id)
        .with_source(test_source.clone())
        .with_reaction(test_reaction.clone())
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);
    core.start().await.expect("Failed to start core");

    let read_only = Arc::new(false);
    let config_persistence = None;

    // Create registry
    let mut instances_map = indexmap::IndexMap::new();
    instances_map.insert(instance_id.to_string(), core.clone());
    let registry = InstanceRegistry::from_map(instances_map);

    // Build router with solutions directory
    let v1_router = build_v1_router(registry, read_only, config_persistence, solutions_dir);

    let router = Router::new()
        .route("/health", axum::routing::get(handlers::health_check))
        .merge(v1_router);

    let components = TestInstanceComponents {
        instance_id: instance_id.to_string(),
        source: test_source,
        reaction: test_reaction,
    };

    (router, core, components)
}

/// Create a test router with multiple DrasiLib instances for multi-instance testing
pub async fn create_multi_instance_test_router(
    instance_ids: Vec<&str>,
    solutions_dir: Option<String>,
) -> (Router, Vec<(String, Arc<DrasiLib>, TestInstanceComponents)>) {
    let mut instances_data = Vec::new();
    let mut instances_map = indexmap::IndexMap::new();

    for instance_id in instance_ids {
        // Create mock components with unique IDs per instance
        let source_id = format!("{instance_id}-source");
        let reaction_id = format!("{instance_id}-reaction");
        let query_id = format!("{instance_id}-query");

        let test_source = create_mock_source(&source_id);
        let test_reaction = create_mock_reaction(&reaction_id, vec![query_id]);

        // Build DrasiLib
        let core = DrasiLib::builder()
            .with_id(instance_id)
            .with_source(test_source.clone())
            .with_reaction(test_reaction.clone())
            .build()
            .await
            .expect("Failed to build test core");

        let core = Arc::new(core);
        core.start().await.expect("Failed to start core");

        instances_map.insert(instance_id.to_string(), core.clone());

        let components = TestInstanceComponents {
            instance_id: instance_id.to_string(),
            source: test_source,
            reaction: test_reaction,
        };

        instances_data.push((instance_id.to_string(), core, components));
    }

    let read_only = Arc::new(false);
    let config_persistence = None;
    let registry = InstanceRegistry::from_map(instances_map);

    let v1_router = build_v1_router(registry, read_only, config_persistence, solutions_dir);

    let router = Router::new()
        .route("/health", axum::routing::get(handlers::health_check))
        .merge(v1_router);

    (router, instances_data)
}

/// Wait for a specific message to appear in reaction logs
///
/// Polls the DrasiLib reaction logs until the expected message appears or timeout.
pub async fn wait_for_reaction_log(
    core: &Arc<DrasiLib>,
    reaction_id: &str,
    expected_substring: &str,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;

    loop {
        let (history, _) = core
            .subscribe_reaction_logs(reaction_id)
            .await
            .map_err(|e| format!("Failed to get reaction logs: {e}"))?;

        for entry in &history {
            if entry.message.contains(expected_substring) {
                return Ok(entry.message.clone());
            }
        }

        if Instant::now() >= deadline {
            let all_messages: Vec<_> = history.iter().map(|e| e.message.clone()).collect();
            return Err(format!(
                "Timeout waiting for '{expected_substring}' in reaction logs. Found messages: {all_messages:?}",
            ));
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Wait for any log entry to appear in reaction logs
///
/// Polls until at least one log entry exists.
pub async fn wait_for_any_reaction_log(
    core: &Arc<DrasiLib>,
    reaction_id: &str,
    timeout: Duration,
) -> Result<Vec<String>, String> {
    let deadline = Instant::now() + timeout;

    loop {
        let (history, _) = core
            .subscribe_reaction_logs(reaction_id)
            .await
            .map_err(|e| format!("Failed to get reaction logs: {e}"))?;

        if !history.is_empty() {
            return Ok(history.iter().map(|e| e.message.clone()).collect());
        }

        if Instant::now() >= deadline {
            return Err("Timeout waiting for any reaction log entry".to_string());
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Create a JSONL test data file for scriptfile bootstrap testing
pub fn create_test_jsonl_file(dir: &Path, filename: &str, entries: &[&str]) -> std::path::PathBuf {
    let path = dir.join(filename);
    let content = entries.join("\n");
    std::fs::write(&path, content).expect("Failed to write JSONL test file");
    path
}

/// Wait for query results to appear (data flow validation).
///
/// Polls `core.get_query_results()` until results are non-empty or timeout.
/// This validates that data flowed from source through query processing.
///
/// Returns the query results on success, or an error with diagnostic info on timeout.
pub async fn wait_for_query_results(
    core: &Arc<DrasiLib>,
    query_id: &str,
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, String> {
    use drasi_lib::channels::ComponentStatus;

    let deadline = Instant::now() + timeout;

    // First wait for the query to be running
    loop {
        match core.get_query_status(query_id).await {
            Ok(ComponentStatus::Running) => {
                break;
            }
            Ok(status) => {
                // Still starting, continue waiting
                if Instant::now() >= deadline {
                    return Err(format!(
                        "Timeout waiting for query '{query_id}' to start. Status: {status:?}"
                    ));
                }
            }
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(format!("Query '{query_id}' not found: {e}"));
                }
            }
        }
        sleep(Duration::from_millis(50)).await;
    }

    // Now poll for results
    loop {
        match core.get_query_results(query_id).await {
            Ok(results) if !results.is_empty() => {
                return Ok(results);
            }
            Ok(_) => {
                // Empty results, continue polling
            }
            Err(e) => {
                // Query might have stopped or other error
                if Instant::now() >= deadline {
                    return Err(format!("Failed to get query results: {e}"));
                }
            }
        }

        if Instant::now() >= deadline {
            // Try to get diagnostic info
            let status = core.get_query_status(query_id).await;
            return Err(format!(
                "Timeout waiting for results from query '{query_id}'. Status: {status:?}"
            ));
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Wait for query results with a minimum count.
///
/// Polls until at least `min_count` results are available.
pub async fn wait_for_query_results_count(
    core: &Arc<DrasiLib>,
    query_id: &str,
    min_count: usize,
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, String> {
    let deadline = Instant::now() + timeout;

    loop {
        match core.get_query_results(query_id).await {
            Ok(results) if results.len() >= min_count => {
                return Ok(results);
            }
            Ok(results) => {
                // Not enough results yet
                if Instant::now() >= deadline {
                    return Err(format!(
                        "Timeout waiting for {} results from query '{}'. Got {} results.",
                        min_count,
                        query_id,
                        results.len()
                    ));
                }
            }
            Err(e) => {
                return Err(format!("Failed to get query results: {e}"));
            }
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Sample JSONL entries for bootstrap testing.
/// Uses the scriptfile bootstrap format with explicit kind and labels.
pub fn sample_bootstrap_jsonl_entries() -> Vec<&'static str> {
    vec![
        r#"{"kind":"Header","start_time":"2024-01-01T00:00:00+00:00","description":"Test bootstrap data"}"#,
        r#"{"kind":"Node","id":"node-1","labels":["TestNode"],"properties":{"id":"node-1","value":100}}"#,
        r#"{"kind":"Node","id":"node-2","labels":["TestNode"],"properties":{"id":"node-2","value":200}}"#,
        r#"{"kind":"Node","id":"node-3","labels":["TestNode"],"properties":{"id":"node-3","value":300}}"#,
        r#"{"kind":"Finish"}"#,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_test_solution_template() {
        let temp_dir = TempDir::new().unwrap();
        create_test_solution_template(temp_dir.path(), "test-solution", simple_mock_log_template());

        let path = temp_dir.path().join("test-solution.yaml");
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Simple Mock Log Pipeline"));
    }

    #[test]
    fn test_template_with_variables_has_variables() {
        let template = template_with_variables();
        assert!(template.contains("${INTERVAL_MS:-1000}"));
        assert!(template.contains("${TEMP_THRESHOLD:-75}"));
    }

    #[test]
    fn test_scriptfile_bootstrap_template() {
        let template = scriptfile_bootstrap_template("/data/test.jsonl");
        assert!(template.contains("scriptfile"));
        assert!(template.contains("/data/test.jsonl"));
    }

    #[test]
    fn test_multi_source_join_template_has_joins() {
        let template = multi_source_join_template();
        assert!(template.contains("joins:"));
        assert!(template.contains("LOCATED_AT"));
        assert!(template.contains("sensor-source"));
        assert!(template.contains("location-source"));
    }

    #[test]
    fn test_create_jsonl_file() {
        let temp_dir = TempDir::new().unwrap();
        let entries = sample_bootstrap_jsonl_entries();
        let path = create_test_jsonl_file(temp_dir.path(), "test.jsonl", &entries);

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("node-1"));
        assert!(content.contains("node-2"));
        assert!(content.contains("node-3"));
    }
}
