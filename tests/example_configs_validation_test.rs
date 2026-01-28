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

//! Tests that validate all example configuration files in the examples/ directory.
//!
//! These tests ensure that example configs remain valid as the configuration schema evolves.

use drasi_server::config::loader::load_config_file;
use std::path::Path;

/// List of example config files to validate.
/// These paths are relative to the project root.
const EXAMPLE_CONFIGS: &[&str] = &[
    // Top-level config directory (Docker and quick-start templates)
    "config/server-minimal.yaml",
    "config/server-docker.yaml",
    "config/server-with-env-vars.yaml",
    // Integration test configs
    "tests/integration/getting-started/config.yaml",
    // Solution examples
    "examples/getting-started/server-config.yaml",
    "examples/playground/server/playground.yaml",
    "examples/playground/app/examples/playground/server/playground.yaml",
    "examples/trading/server/trading-sources-only.yaml",
    // 01-fundamentals
    "examples/configs/01-fundamentals/hello-world.yaml",
    "examples/configs/01-fundamentals/mock-with-logging.yaml",
    "examples/configs/01-fundamentals/first-continuous-query.yaml",
    // 02-sources
    "examples/configs/02-sources/http-webhook-receiver.yaml",
    "examples/configs/02-sources/grpc-streaming-source.yaml",
    "examples/configs/02-sources/postgres-cdc-complete.yaml",
    // 03-reactions
    "examples/configs/03-reactions/log-with-templates.yaml",
    "examples/configs/03-reactions/http-webhook-sender.yaml",
    "examples/configs/03-reactions/sse-browser-streaming.yaml",
    "examples/configs/03-reactions/grpc-streaming-reaction.yaml",
    "examples/configs/03-reactions/profiler-performance.yaml",
    // 04-query-patterns
    "examples/configs/04-query-patterns/filter-and-projection.yaml",
    "examples/configs/04-query-patterns/aggregation-queries.yaml",
    "examples/configs/04-query-patterns/multi-source-queries.yaml",
    "examples/configs/04-query-patterns/time-based-triggers.yaml",
    // 05-advanced-features
    "examples/configs/05-advanced-features/adaptive-batching.yaml",
    "examples/configs/05-advanced-features/multi-instance.yaml",
    "examples/configs/05-advanced-features/persistent-storage.yaml",
    "examples/configs/05-advanced-features/capacity-tuning.yaml",
    "examples/configs/05-advanced-features/read-only-deployment.yaml",
    // 06-real-world-scenarios
    "examples/configs/06-real-world-scenarios/iot-sensor-alerts.yaml",
    "examples/configs/06-real-world-scenarios/order-exception-handling.yaml",
    "examples/configs/06-real-world-scenarios/absence-of-change.yaml",
    "examples/configs/06-real-world-scenarios/real-time-dashboard.yaml",
];

#[test]
fn test_all_example_configs_are_valid() {
    let mut failures: Vec<(String, String)> = Vec::new();

    for config_path in EXAMPLE_CONFIGS {
        let path = Path::new(config_path);

        if !path.exists() {
            failures.push((
                config_path.to_string(),
                format!("File does not exist: {config_path}"),
            ));
            continue;
        }

        match load_config_file(path) {
            Ok(_) => {
                // Config is valid
            }
            Err(e) => {
                failures.push((config_path.to_string(), e.to_string()));
            }
        }
    }

    if !failures.is_empty() {
        let failure_messages: Vec<String> = failures
            .iter()
            .map(|(path, err)| format!("  - {path}: {err}"))
            .collect();

        panic!(
            "The following example config files failed validation:\n{}",
            failure_messages.join("\n")
        );
    }
}

// Individual tests for each config file provide better granularity in test output

// ==================== Top-level Config Directory ====================

#[test]
fn test_config_server_minimal() {
    let path = "config/server-minimal.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_config_server_docker() {
    let path = "config/server-docker.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_config_server_with_env_vars() {
    let path = "config/server-with-env-vars.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== Integration Test Configs ====================

#[test]
fn test_integration_getting_started_config() {
    let path = "tests/integration/getting-started/config.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== Existing Examples ====================

#[test]
fn test_getting_started_config() {
    let path = "examples/getting-started/server-config.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_playground_server_config() {
    let path = "examples/playground/server/playground.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_playground_app_config() {
    let path = "examples/playground/app/examples/playground/server/playground.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_trading_sources_only_config() {
    let path = "examples/trading/server/trading-sources-only.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== 01-fundamentals ====================

#[test]
fn test_01_hello_world() {
    let path = "examples/configs/01-fundamentals/hello-world.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_01_mock_with_logging() {
    let path = "examples/configs/01-fundamentals/mock-with-logging.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_01_first_continuous_query() {
    let path = "examples/configs/01-fundamentals/first-continuous-query.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== 02-sources ====================

#[test]
fn test_02_http_webhook_receiver() {
    let path = "examples/configs/02-sources/http-webhook-receiver.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_02_grpc_streaming_source() {
    let path = "examples/configs/02-sources/grpc-streaming-source.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_02_postgres_cdc_complete() {
    let path = "examples/configs/02-sources/postgres-cdc-complete.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== 03-reactions ====================

#[test]
fn test_03_log_with_templates() {
    let path = "examples/configs/03-reactions/log-with-templates.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_03_http_webhook_sender() {
    let path = "examples/configs/03-reactions/http-webhook-sender.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_03_sse_browser_streaming() {
    let path = "examples/configs/03-reactions/sse-browser-streaming.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_03_grpc_streaming_reaction() {
    let path = "examples/configs/03-reactions/grpc-streaming-reaction.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_03_profiler_performance() {
    let path = "examples/configs/03-reactions/profiler-performance.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== 04-query-patterns ====================

#[test]
fn test_04_filter_and_projection() {
    let path = "examples/configs/04-query-patterns/filter-and-projection.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_04_aggregation_queries() {
    let path = "examples/configs/04-query-patterns/aggregation-queries.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_04_multi_source_queries() {
    let path = "examples/configs/04-query-patterns/multi-source-queries.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_04_time_based_triggers() {
    let path = "examples/configs/04-query-patterns/time-based-triggers.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== 05-advanced-features ====================

#[test]
fn test_05_adaptive_batching() {
    let path = "examples/configs/05-advanced-features/adaptive-batching.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_05_multi_instance() {
    let path = "examples/configs/05-advanced-features/multi-instance.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_05_persistent_storage() {
    let path = "examples/configs/05-advanced-features/persistent-storage.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_05_capacity_tuning() {
    let path = "examples/configs/05-advanced-features/capacity-tuning.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_05_read_only_deployment() {
    let path = "examples/configs/05-advanced-features/read-only-deployment.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

// ==================== 06-real-world-scenarios ====================

#[test]
fn test_06_iot_sensor_alerts() {
    let path = "examples/configs/06-real-world-scenarios/iot-sensor-alerts.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_06_order_exception_handling() {
    let path = "examples/configs/06-real-world-scenarios/order-exception-handling.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_06_absence_of_change() {
    let path = "examples/configs/06-real-world-scenarios/absence-of-change.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_06_real_time_dashboard() {
    let path = "examples/configs/06-real-world-scenarios/real-time-dashboard.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}
