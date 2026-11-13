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

//! Test configuration helpers for creating config types.
//!
//! These helpers construct SourceConfig, ReactionConfig, and QueryConfig
//! for use in tests.

use drasi_lib::Query;
use drasi_server::{QueryConfig, ReactionConfig, SourceConfig, StateStoreConfig};
use tempfile::TempDir;

/// Create a mock source config for testing
pub fn mock_source(id: impl Into<String>) -> SourceConfig {
    SourceConfig {
        kind: "mock".to_string(),
        id: id.into(),
        auto_start: true,
        identity_provider: None,
        bootstrap_provider: None,
        config: serde_json::json!({"dataType": {"type": "generic"}, "intervalMs": 5000}),
    }
}

/// Create a mock source config with auto_start disabled
pub fn mock_source_manual(id: impl Into<String>) -> SourceConfig {
    SourceConfig {
        kind: "mock".to_string(),
        id: id.into(),
        auto_start: false,
        identity_provider: None,
        bootstrap_provider: None,
        config: serde_json::json!({"dataType": {"type": "generic"}, "intervalMs": 5000}),
    }
}

/// Create a query config for testing
pub fn test_query(
    id: impl Into<String>,
    query: impl Into<String>,
    inputs: Vec<String>,
) -> QueryConfig {
    let mut builder =
        Query::cypher(drasi_lib::ComponentName::try_new(id.into()).expect("valid test query name"))
            .query(query)
            .auto_start(true);

    for source in inputs {
        builder = builder.from_input_by_name(
            drasi_lib::ComponentName::try_new(source).expect("valid test input name"),
        );
    }

    builder.build().expect("valid test query")
}

/// Create a log reaction config for testing
pub fn log_reaction(id: impl Into<String>, queries: Vec<String>) -> ReactionConfig {
    ReactionConfig {
        kind: "log".to_string(),
        id: id.into(),
        queries: queries
            .into_iter()
            .map(|name| {
                drasi_lib::config::ComponentReference::by_name(
                    drasi_lib::ComponentName::try_new(name).expect("valid test query name"),
                )
            })
            .collect(),
        auto_start: true,
        identity_provider: None,
        config: serde_json::json!({"routes": {}}),
    }
}

/// Create a log reaction config with auto_start disabled
pub fn log_reaction_manual(id: impl Into<String>, queries: Vec<String>) -> ReactionConfig {
    ReactionConfig {
        kind: "log".to_string(),
        id: id.into(),
        queries: queries
            .into_iter()
            .map(|name| {
                drasi_lib::config::ComponentReference::by_name(
                    drasi_lib::ComponentName::try_new(name).expect("valid test query name"),
                )
            })
            .collect(),
        auto_start: false,
        identity_provider: None,
        config: serde_json::json!({"routes": {}}),
    }
}

// =============================================================================
// State Store Helpers
// =============================================================================

/// Create a REDB state store config for testing with a specified path
pub fn redb_state_store(path: impl Into<String>) -> StateStoreConfig {
    StateStoreConfig::redb(path)
}

/// Create a REDB state store config using a TempDir for isolation
///
/// Returns the StateStoreConfig and the TempDir (which must be kept alive
/// for the duration of the test to prevent cleanup).
pub fn temp_redb_state_store(filename: &str) -> (StateStoreConfig, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let path = temp_dir.path().join(filename);
    let config = StateStoreConfig::redb(path.to_string_lossy().to_string());
    (config, temp_dir)
}
