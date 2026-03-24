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

//! Integration tests that run `drasi-server validate` against every example config file.
//!
//! These tests exercise the full CLI validate command (structure validation,
//! env var checking, and plugin-aware schema validation when plugins are available).

use std::path::Path;
use std::process::Command;

/// All example config files that must pass `drasi-server validate`.
const EXAMPLE_CONFIGS: &[&str] = &[
    // Top-level config directory
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

fn get_binary_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{manifest_dir}/target/debug/drasi-server")
}

fn run_validate(config_path: &str) -> (bool, String, String) {
    let output = Command::new(get_binary_path())
        .args(["validate", "--config", config_path])
        // Provide dummy values for env vars that example configs reference
        // without defaults. These configs are designed to show env var usage
        // and intentionally require certain vars to be set.
        .env("DB_HOST", "localhost")
        .env("DB_NAME", "testdb")
        .env("DB_USER", "testuser")
        .env("DB_PASSWORD", "testpass")
        .env("WEBHOOK_URL", "http://localhost:3000/hook")
        .env("WEBHOOK_TOKEN", "test-token")
        .output()
        .expect("Failed to execute validate command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

/// Run `drasi-server validate` against every example config and ensure
/// the exit code is 0 (success). Warnings about missing plugins are expected
/// and acceptable — only hard errors cause failure.
#[test]
fn test_validate_all_example_configs_via_cli() {
    let mut failures: Vec<(String, String)> = Vec::new();

    for config_path in EXAMPLE_CONFIGS {
        if !Path::new(config_path).exists() {
            failures.push((config_path.to_string(), "File does not exist".to_string()));
            continue;
        }

        let (success, stdout, stderr) = run_validate(config_path);
        if !success {
            failures.push((
                config_path.to_string(),
                format!("Exit code non-zero.\nstdout:\n{stdout}\nstderr:\n{stderr}"),
            ));
        }
    }

    if !failures.is_empty() {
        let msgs: Vec<String> = failures
            .iter()
            .map(|(path, err)| format!("  - {path}:\n    {err}"))
            .collect();
        panic!(
            "`drasi-server validate` failed for {} config(s):\n{}",
            failures.len(),
            msgs.join("\n")
        );
    }
}

/// Verify that the validate command output contains the expected sections
/// for a known-good config file.
#[test]
fn test_validate_output_format() {
    let config = "examples/configs/01-fundamentals/hello-world.yaml";
    if !Path::new(config).exists() {
        eprintln!("Skipping: {config} not found");
        return;
    }

    let (success, stdout, _stderr) = run_validate(config);
    assert!(success, "validate should succeed for {config}");
    assert!(
        stdout.contains("Structure:"),
        "Output should contain Structure section"
    );
    assert!(
        stdout.contains("[OK] YAML syntax valid"),
        "Output should confirm YAML syntax"
    );
    assert!(
        stdout.contains("Environment references:"),
        "Output should contain Environment references section"
    );
    assert!(
        stdout.contains("Plugins"),
        "Output should contain Plugins section"
    );
    assert!(
        stdout.contains("Config validation:"),
        "Output should contain Config validation section"
    );
    assert!(stdout.contains("Summary:"), "Output should contain Summary");
}

// =============================================================================
// Negative Tests — configs that should FAIL validation
// =============================================================================

/// Run validate without providing dummy env vars (for testing missing-env-var detection).
fn run_validate_raw(config_content: &str) -> (bool, String) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("test-config.yaml");
    std::fs::write(&config_path, config_content).expect("write temp config");

    let output = Command::new(get_binary_path())
        .args(["validate", "--config", config_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute validate command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), stdout)
}

/// Config with a required env var that has no default should fail validation.
#[test]
fn test_validate_rejects_missing_required_env_var() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - kind: mock
    id: my-source
    password: "${THIS_VAR_DEFINITELY_DOES_NOT_EXIST}"
queries: []
reactions: []
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(
        !success,
        "validate should fail when required env var is missing"
    );
    assert!(
        stdout.contains("[ERR]") && stdout.contains("THIS_VAR_DEFINITELY_DOES_NOT_EXIST"),
        "Output should report the missing env var.\nGot:\n{stdout}"
    );
}

/// Config with an env var that HAS a default should pass.
#[test]
fn test_validate_accepts_env_var_with_default() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - kind: mock
    id: my-source
    someField: "${NONEXISTENT_VAR:-fallback_value}"
queries: []
reactions: []
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(
        success,
        "validate should pass when env var has a default.\nGot:\n{stdout}"
    );
}

/// Config with a ConfigValue::EnvironmentVariable JSON pattern (no default) should fail.
#[test]
fn test_validate_rejects_config_value_env_var_missing() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - kind: mock
    id: my-source
    password:
      kind: EnvironmentVariable
      name: ABSOLUTELY_NOT_SET_VAR_12345
queries: []
reactions: []
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(
        !success,
        "validate should fail for ConfigValue::EnvironmentVariable with missing var"
    );
    assert!(
        stdout.contains("ABSOLUTELY_NOT_SET_VAR_12345"),
        "Output should name the missing var.\nGot:\n{stdout}"
    );
}

/// Config with a ConfigValue::EnvironmentVariable that has a default should pass.
#[test]
fn test_validate_accepts_config_value_env_var_with_default() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - kind: mock
    id: my-source
    password:
      kind: EnvironmentVariable
      name: ALSO_NOT_SET_VAR_67890
      default: "fallback"
queries: []
reactions: []
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(
        success,
        "validate should pass when ConfigValue has a default.\nGot:\n{stdout}"
    );
}

/// Handlebars template expressions `${{...}}` should NOT be treated as env var refs.
#[test]
fn test_validate_ignores_handlebars_template_expressions() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - kind: mock
    id: my-source
queries: []
reactions:
  - kind: log
    id: my-log
    queries:
      - some-query
    routes:
      some-query:
        added:
          template: "Order ${{after.orderId}} total: ${{after.total}}"
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(
        success,
        "validate should not flag Handlebars templates as env var errors.\nGot:\n{stdout}"
    );
    assert!(
        !stdout.contains("[ERR]"),
        "There should be no errors.\nGot:\n{stdout}"
    );
}

/// Invalid YAML syntax should fail at the structure phase.
#[test]
fn test_validate_rejects_invalid_yaml() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: [invalid yaml
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(!success, "validate should fail for invalid YAML");
    assert!(
        stdout.contains("[ERR]"),
        "Output should contain an error.\nGot:\n{stdout}"
    );
}

/// Missing required 'kind' field on a source should fail.
#[test]
fn test_validate_rejects_source_missing_kind() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - id: my-source
queries: []
reactions: []
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(
        !success,
        "validate should fail when source is missing 'kind'"
    );
    assert!(
        stdout.contains("[ERR]"),
        "Output should contain an error.\nGot:\n{stdout}"
    );
}

/// Missing required 'id' field on a source should fail.
#[test]
fn test_validate_rejects_source_missing_id() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - kind: mock
queries: []
reactions: []
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(!success, "validate should fail when source is missing 'id'");
    assert!(
        stdout.contains("[ERR]"),
        "Output should contain an error.\nGot:\n{stdout}"
    );
}

/// Multiple env var errors should all be reported (not fail-fast).
#[test]
fn test_validate_reports_multiple_env_var_errors() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources:
  - kind: mock
    id: my-source
    field_a: "${MISSING_VAR_AAA}"
    field_b: "${MISSING_VAR_BBB}"
queries: []
reactions: []
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(!success, "validate should fail with missing env vars");
    assert!(
        stdout.contains("MISSING_VAR_AAA") && stdout.contains("MISSING_VAR_BBB"),
        "Both missing vars should be reported.\nGot:\n{stdout}"
    );
}

/// Reaction missing required 'queries' list should fail.
#[test]
fn test_validate_rejects_reaction_missing_queries() {
    let config = r#"
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
sources: []
queries: []
reactions:
  - kind: log
    id: my-log
"#;
    let (success, stdout) = run_validate_raw(config);
    assert!(
        !success,
        "validate should fail when reaction has no queries"
    );
    assert!(
        stdout.contains("[ERR]"),
        "Output should contain an error.\nGot:\n{stdout}"
    );
}
