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

//! Test configuration helpers for the new builder API

use drasi_lib::{Query, Reaction, Source};
use drasi_server::{QueryConfig, ReactionConfig, SourceConfig};
use serde_json::Value;
use std::collections::HashMap;

/// Create a mock source config for testing
pub fn mock_source(id: impl Into<String>) -> SourceConfig {
    Source::mock(id).auto_start(true).build()
}

/// Create a mock source config with properties
pub fn mock_source_with_props(
    id: impl Into<String>,
    properties: HashMap<String, Value>,
) -> SourceConfig {
    let mut builder = Source::mock(id).auto_start(true);

    for (key, value) in properties {
        builder = builder.with_property_value(key, value);
    }

    builder.build()
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
    let mut builder = Reaction::log(id).auto_start(true);

    for query in queries {
        builder = builder.subscribe_to(query);
    }

    builder.build()
}

/// Create a log reaction config with properties
pub fn log_reaction_with_props(
    id: impl Into<String>,
    queries: Vec<String>,
    properties: HashMap<String, Value>,
) -> ReactionConfig {
    let mut builder = Reaction::log(id).auto_start(true);

    for query in queries {
        builder = builder.subscribe_to(query);
    }

    for (key, value) in properties {
        builder = builder.with_property_value(key, value);
    }

    builder.build()
}

/// Create an HTTP reaction config for testing
pub fn http_reaction(id: impl Into<String>, queries: Vec<String>) -> ReactionConfig {
    let mut builder = Reaction::http(id).auto_start(true);

    for query in queries {
        builder = builder.subscribe_to(query);
    }

    builder.build()
}

/// Create an HTTP reaction config with properties
pub fn http_reaction_with_props(
    id: impl Into<String>,
    queries: Vec<String>,
    properties: HashMap<String, Value>,
) -> ReactionConfig {
    let mut builder = Reaction::http(id).auto_start(true);

    for query in queries {
        builder = builder.subscribe_to(query);
    }

    for (key, value) in properties {
        builder = builder.with_property_value(key, value);
    }

    builder.build()
}
