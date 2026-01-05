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
    "examples/configs/basic-mock-source/config.yaml",
    "examples/debug-platform-building-comfort/server-config.yaml",
    "examples/drasi-platform/server-config.yaml",
    "examples/getting-started/server-config.yaml",
    "examples/playground/server/playground.yaml",
    "examples/playground/app/examples/playground/server/playground.yaml",
    "examples/trading/server/trading-sources-only.yaml",
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

#[test]
fn test_basic_mock_source_config() {
    let path = "examples/configs/basic-mock-source/config.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_debug_platform_building_comfort_config() {
    let path = "examples/debug-platform-building-comfort/server-config.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

#[test]
fn test_drasi_platform_config() {
    let path = "examples/drasi-platform/server-config.yaml";
    load_config_file(path).unwrap_or_else(|e| panic!("Failed to validate {path}: {e}"));
}

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
