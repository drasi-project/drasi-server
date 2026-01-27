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

//! Integration tests for the `drasi-server validate` command.
//!
//! These tests verify that the validate command:
//! - Correctly validates valid configuration files
//! - Rejects invalid configuration files with helpful errors
//! - Handles missing files appropriately
//! - Works with various configuration scenarios

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Get the path to the drasi-server binary
fn get_binary_path() -> String {
    // In tests, the binary is built in target/debug
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{manifest_dir}/target/debug/drasi-server")
}

/// Helper to run validate command and capture output
fn run_validate(config_path: &str) -> (bool, String, String) {
    let output = Command::new(get_binary_path())
        .args(["validate", "--config", config_path])
        .output()
        .expect("Failed to execute validate command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

/// Helper to run validate command with --show-resolved flag
fn run_validate_show_resolved(config_path: &str) -> (bool, String, String) {
    let output = Command::new(get_binary_path())
        .args(["validate", "--config", config_path, "--show-resolved"])
        .output()
        .expect("Failed to execute validate command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

// =============================================================================
// Valid Configuration Tests
// =============================================================================

#[test]
fn test_validate_minimal_valid_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(success, "Minimal valid config should pass validation");
    assert!(
        stdout.contains("[OK]"),
        "Output should contain success indicator"
    );
    assert!(
        stdout.contains("Configuration file is valid"),
        "Output should confirm validity"
    );
}

#[test]
fn test_validate_config_with_mock_source() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources:
  - kind: mock
    id: test-source
    autoStart: true
    dataType: sensor
    intervalMs: 5000
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(success, "Config with mock source should pass validation");
    assert!(stdout.contains("[OK]"));
    assert!(stdout.contains("Sources: 1"));
}

#[test]
fn test_validate_config_with_query() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources:
  - kind: mock
    id: test-source
    autoStart: true
queries:
  - id: my-query
    query: "MATCH (n) RETURN n"
    queryLanguage: Cypher
    autoStart: true
    sources:
      - sourceId: test-source
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(success, "Config with query should pass validation");
    assert!(stdout.contains("Queries: 1"));
}

#[test]
fn test_validate_config_with_log_reaction() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources: []
queries: []
reactions:
  - kind: log
    id: test-log
    queries: [my-query]
    autoStart: true
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(success, "Config with log reaction should pass validation");
    assert!(stdout.contains("Reactions: 1"));
}

#[test]
fn test_validate_config_with_bootstrap_provider() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources:
  - kind: mock
    id: test-source
    autoStart: true
    bootstrapProvider:
      kind: scriptfile
      filePaths:
        - /data/init.jsonl
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(
        success,
        "Config with bootstrap provider should pass validation"
    );
    assert!(stdout.contains("[OK]"));
}

#[test]
fn test_validate_config_with_state_store() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
stateStore:
  kind: redb
  path: ./data/state.redb
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(success, "Config with state store should pass validation");
    assert!(stdout.contains("[OK]"));
}

// =============================================================================
// Invalid Configuration Tests
// =============================================================================

#[test]
fn test_validate_rejects_snake_case_fields() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
log_level: info
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(
        !success,
        "Config with snake_case log_level should fail validation"
    );
    assert!(
        stdout.contains("[ERROR]") || stdout.contains("invalid"),
        "Output should indicate error"
    );
}

#[test]
fn test_validate_rejects_unknown_fields() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
unknownField: someValue
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(!success, "Config with unknown field should fail validation");
    assert!(
        stdout.contains("unknownField") || stdout.contains("unknown field"),
        "Error should mention the unknown field"
    );
}

#[test]
fn test_validate_rejects_invalid_port() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 0
logLevel: info
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(!success, "Config with port 0 should fail validation");
    assert!(
        stdout.contains("port") || stdout.contains("Port"),
        "Error should mention port"
    );
}

#[test]
fn test_validate_rejects_invalid_log_level() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: invalid_level
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(!success, "Config with invalid log level should fail");
    assert!(
        stdout.contains("log") || stdout.contains("level") || stdout.contains("invalid_level"),
        "Error should mention log level issue"
    );
}

#[test]
fn test_validate_rejects_bootstrap_provider_type_instead_of_kind() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources:
  - kind: mock
    id: test-source
    autoStart: true
    bootstrapProvider:
      type: postgres
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(
        !success,
        "Bootstrap provider with 'type' instead of 'kind' should fail"
    );
    // The error might mention 'type' or 'kind' or 'unknown'
    assert!(
        stdout.contains("type") || stdout.contains("kind") || stdout.contains("unknown"),
        "Error should indicate the field naming issue"
    );
}

#[test]
fn test_validate_rejects_source_with_snake_case_auto_start() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources:
  - kind: mock
    id: test-source
    auto_start: true
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(
        !success,
        "Source with snake_case auto_start should fail validation"
    );
    assert!(
        stdout.contains("auto_start") || stdout.contains("unknown"),
        "Error should mention auto_start"
    );
}

// =============================================================================
// Missing File Tests
// =============================================================================

#[test]
fn test_validate_missing_file() {
    let (success, stdout, _stderr) = run_validate("/nonexistent/path/config.yaml");

    assert!(!success, "Validation of missing file should fail");
    assert!(
        stdout.contains("not found")
            || stdout.contains("No such file")
            || stdout.contains("does not exist")
            || stdout.contains("error")
            || stdout.contains("ERROR"),
        "Error should indicate file not found"
    );
}

// =============================================================================
// Environment Variable Tests
// =============================================================================

#[test]
fn test_validate_config_with_env_var_syntax() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: ${SERVER_HOST:-0.0.0.0}
port: ${SERVER_PORT:-8080}
logLevel: info
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(
        success,
        "Config with env var syntax should pass validation. Output: {stdout}"
    );
    assert!(stdout.contains("[OK]"));
}

// =============================================================================
// Show Resolved Tests
// =============================================================================

#[test]
fn test_validate_show_resolved_expands_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: ${DRASI_HOST:-localhost}
port: 8080
logLevel: info
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate_show_resolved(config_path.to_str().unwrap());

    assert!(success, "Show resolved should succeed for valid config");
    // The resolved output should show the default value
    assert!(
        stdout.contains("localhost") || stdout.contains("Resolved"),
        "Output should show resolved values or indicate resolution"
    );
}

// =============================================================================
// Summary Output Tests
// =============================================================================

#[test]
fn test_validate_shows_correct_summary_counts() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources:
  - kind: mock
    id: source1
    autoStart: true
  - kind: mock
    id: source2
    autoStart: true
queries:
  - id: query1
    query: "MATCH (n) RETURN n"
    sources:
      - sourceId: source1
  - id: query2
    query: "MATCH (m) RETURN m"
    sources:
      - sourceId: source2
  - id: query3
    query: "MATCH (x) RETURN x"
    sources:
      - sourceId: source1
reactions:
  - kind: log
    id: reaction1
    queries: [query1]
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(success, "Config should be valid");
    assert!(stdout.contains("Sources: 2"), "Should show 2 sources");
    assert!(stdout.contains("Queries: 3"), "Should show 3 queries");
    assert!(stdout.contains("Reactions: 1"), "Should show 1 reaction");
}

// =============================================================================
// Multi-Instance Configuration Tests
// =============================================================================

#[test]
fn test_validate_multi_instance_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
host: 0.0.0.0
port: 8080
logLevel: info
instances:
  - id: instance1
    sources:
      - kind: mock
        id: source1
        autoStart: true
    queries: []
    reactions: []
  - id: instance2
    sources:
      - kind: mock
        id: source2
        autoStart: true
    queries: []
    reactions: []
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(success, "Multi-instance config should pass validation");
    assert!(
        stdout.contains("Instances: 2"),
        "Should show 2 instances. Output: {stdout}"
    );
}

// =============================================================================
// Template Validation Tests
// =============================================================================

#[test]
fn test_validate_accepts_valid_handlebars_template() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources: []
queries: []
reactions:
  - kind: log
    id: test-log
    queries: [my-query]
    autoStart: true
    defaultTemplate:
      added:
        template: "Added: {{json this}}"
      updated:
        template: "Updated: {{before}} -> {{after}}"
      deleted:
        template: "Deleted: {{json this}}"
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(
        success,
        "Config with valid Handlebars templates should pass. Output: {stdout}"
    );
}

#[test]
fn test_validate_rejects_invalid_handlebars_template() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    let config = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
sources: []
queries: []
reactions:
  - kind: log
    id: test-log
    queries: [my-query]
    autoStart: true
    defaultTemplate:
      added:
        template: "Added: {{unclosed"
"#;
    fs::write(&config_path, config).unwrap();

    let (success, stdout, _stderr) = run_validate(config_path.to_str().unwrap());

    assert!(
        !success,
        "Config with invalid Handlebars template should fail"
    );
    assert!(
        stdout.contains("template") || stdout.contains("Template") || stdout.contains("unclosed"),
        "Error should mention template issue"
    );
}
