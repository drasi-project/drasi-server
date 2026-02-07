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
use drasi_server::models::sources::mock::DataTypeDto;
use drasi_server::models::{LogReactionConfigDto, MockSourceConfigDto};
use drasi_server::{QueryConfig, ReactionConfig, SourceConfig, StateStoreConfig};
use tempfile::TempDir;

/// Create a mock source config for testing
pub fn mock_source(id: impl Into<String>) -> SourceConfig {
    SourceConfig::Mock {
        id: id.into(),
        auto_start: true,
        bootstrap_provider: None,
        config: default_mock_config(),
    }
}

fn default_mock_config() -> MockSourceConfigDto {
    MockSourceConfigDto {
        data_type: DataTypeDto::Generic,
        interval_ms: drasi_server::models::ConfigValue::Static(5000),
    }
}

/// Create a mock source config with auto_start disabled
pub fn mock_source_manual(id: impl Into<String>) -> SourceConfig {
    SourceConfig::Mock {
        id: id.into(),
        auto_start: false,
        bootstrap_provider: None,
        config: default_mock_config(),
    }
}

/// Create a query config for testing
pub fn test_query(
    id: impl Into<String>,
    query: impl Into<String>,
    sources: Vec<String>,
) -> QueryConfig {
    let mut builder = Query::cypher(id).query(query).auto_start(true);

    for source in sources {
        builder = builder.from_source(source);
    }

    builder.build()
}

/// Create a log reaction config for testing
pub fn log_reaction(id: impl Into<String>, queries: Vec<String>) -> ReactionConfig {
    ReactionConfig::Log {
        id: id.into(),
        queries,
        auto_start: true,
        config: LogReactionConfigDto::default(),
    }
}

/// Create a log reaction config with auto_start disabled
pub fn log_reaction_manual(id: impl Into<String>, queries: Vec<String>) -> ReactionConfig {
    ReactionConfig::Log {
        id: id.into(),
        queries,
        auto_start: false,
        config: LogReactionConfigDto::default(),
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
