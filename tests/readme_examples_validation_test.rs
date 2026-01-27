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

//! Tests that validate YAML configuration examples embedded in README.md.
//!
//! This ensures documentation examples remain valid as the configuration schema evolves.

use drasi_server::config::loader::load_config_file;
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

/// Extract all YAML code blocks from a markdown file.
fn extract_yaml_blocks(content: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut in_yaml_block = false;
    let mut current_block = String::new();
    let mut block_start_line = 0;

    for (line_num, line) in content.lines().enumerate() {
        if line.trim() == "```yaml" {
            in_yaml_block = true;
            current_block.clear();
            block_start_line = line_num + 1;
        } else if in_yaml_block && line.trim() == "```" {
            in_yaml_block = false;
            if !current_block.trim().is_empty() {
                blocks.push((block_start_line, current_block.clone()));
            }
        } else if in_yaml_block {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }

    blocks
}

/// Check if a YAML block looks like a complete server config.
/// Complete configs must have host/port at top level AND at least one component array.
fn is_complete_server_config(yaml: &str) -> bool {
    // Check for top-level host and port (must start at column 0)
    let has_host = yaml.lines().any(|l| l.starts_with("host:"));
    let has_port = yaml.lines().any(|l| l.starts_with("port:"));

    // Check for top-level component arrays (must start at column 0)
    let has_sources = yaml.lines().any(|l| l.starts_with("sources:"));
    let has_queries = yaml.lines().any(|l| l.starts_with("queries:"));
    let has_reactions = yaml.lines().any(|l| l.starts_with("reactions:"));
    let has_instances = yaml.lines().any(|l| l.starts_with("instances:"));

    // A complete config must have host+port AND at least sources, queries, or reactions
    // OR have instances (multi-instance config)
    let has_server_settings = has_host && has_port;
    let has_components = has_sources || has_queries || has_reactions;

    (has_server_settings && has_components) || has_instances
}

/// Check if a YAML block is an incomplete fragment (e.g., just showing postgres source config).
fn is_incomplete_fragment(yaml: &str) -> bool {
    // Check for postgres source without required fields
    if yaml.contains("kind: postgres") {
        let has_database = yaml.contains("database:");
        let has_user = yaml.contains("user:");
        // If it's a postgres source but missing required fields, it's a fragment
        if !has_database || !has_user {
            return true;
        }
    }
    false
}

/// Check if a YAML block contains placeholder comments that would break parsing.
fn has_placeholder_comments(yaml: &str) -> bool {
    yaml.contains("# ...")
}

#[test]
fn test_readme_yaml_blocks_are_valid_yaml() {
    let readme_content = fs::read_to_string("README.md").expect("Failed to read README.md");
    let yaml_blocks = extract_yaml_blocks(&readme_content);

    assert!(!yaml_blocks.is_empty(), "No YAML blocks found in README.md");

    let mut parse_failures: Vec<(usize, String)> = Vec::new();

    for (line_num, yaml) in &yaml_blocks {
        // Skip blocks with placeholder comments
        if has_placeholder_comments(yaml) {
            continue;
        }

        // Try to parse as generic YAML to check syntax
        match serde_yaml::from_str::<serde_yaml::Value>(yaml) {
            Ok(_) => {}
            Err(e) => {
                parse_failures.push((*line_num, format!("YAML parse error: {e}")));
            }
        }
    }

    if !parse_failures.is_empty() {
        let failure_messages: Vec<String> = parse_failures
            .iter()
            .map(|(line, err)| format!("  - Line {line}: {err}"))
            .collect();

        panic!(
            "The following YAML blocks in README.md have syntax errors:\n{}",
            failure_messages.join("\n")
        );
    }
}

#[test]
fn test_readme_complete_configs_validate() {
    let readme_content = fs::read_to_string("README.md").expect("Failed to read README.md");
    let yaml_blocks = extract_yaml_blocks(&readme_content);

    let mut validation_failures: Vec<(usize, String)> = Vec::new();
    let mut validated_count = 0;

    for (line_num, yaml) in &yaml_blocks {
        // Skip blocks with placeholder comments, incomplete fragments, or that aren't complete configs
        if has_placeholder_comments(yaml)
            || is_incomplete_fragment(yaml)
            || !is_complete_server_config(yaml)
        {
            continue;
        }

        // Write to temp file and validate
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(yaml.as_bytes())
            .expect("Failed to write temp file");

        match load_config_file(temp_file.path()) {
            Ok(_) => {
                validated_count += 1;
            }
            Err(e) => {
                validation_failures.push((*line_num, e.to_string()));
            }
        }
    }

    if !validation_failures.is_empty() {
        let failure_messages: Vec<String> = validation_failures
            .iter()
            .map(|(line, err)| format!("  - Line {line}: {err}"))
            .collect();

        panic!(
            "The following complete config examples in README.md failed validation:\n{}",
            failure_messages.join("\n")
        );
    }

    // Ensure we actually validated some configs
    assert!(
        validated_count > 0,
        "No complete config examples were found to validate in README.md"
    );
}

/// Test the specific "Minimal Configuration Example" from README.md
#[test]
fn test_readme_minimal_config_example() {
    let yaml = r#"
host: 0.0.0.0
port: 8080
logLevel: info

sources:
  - kind: mock
    id: test-source
    autoStart: true

queries:
  - id: my-query
    query: "MATCH (n:Node) RETURN n"
    sources:
      - sourceId: test-source

reactions:
  - kind: log
    id: log-output
    queries: [my-query]
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    temp_file
        .write_all(yaml.as_bytes())
        .expect("Failed to write temp file");

    load_config_file(temp_file.path()).expect("Minimal config example from README should be valid");
}

/// Test the "Server Settings Example" from README.md
#[test]
fn test_readme_server_settings_example() {
    let yaml = r#"
id: my-server
host: 0.0.0.0
port: 8080
logLevel: info
persistConfig: true
persistIndex: false

stateStore:
  kind: redb
  path: ./data/state.redb

sources: []
queries: []
reactions: []
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    temp_file
        .write_all(yaml.as_bytes())
        .expect("Failed to write temp file");

    load_config_file(temp_file.path())
        .expect("Server settings example from README should be valid");
}
