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

//! Query configuration DTOs with camelCase serialization.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};

/// Query configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryConfigDto {
    pub id: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
    pub query: ConfigValue<String>,
    #[serde(default = "default_query_language")]
    pub query_language: ConfigValue<String>,
    #[serde(default)]
    pub middleware: Vec<String>,
    #[serde(default)]
    pub sources: Vec<SourceSubscriptionConfigDto>,
    #[serde(default = "default_enable_bootstrap")]
    pub enable_bootstrap: bool,
    #[serde(default = "default_bootstrap_buffer_size")]
    pub bootstrap_buffer_size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joins: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_queue_capacity: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dispatch_buffer_capacity: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dispatch_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_backend: Option<serde_json::Value>,
}

/// Source subscription configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSubscriptionConfigDto {
    pub source_id: ConfigValue<String>,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub relations: Vec<String>,
    #[serde(default)]
    pub pipeline: Vec<String>,
}

fn default_auto_start() -> bool {
    false
}

fn default_query_language() -> ConfigValue<String> {
    ConfigValue::Static("GQL".to_string())
}

fn default_enable_bootstrap() -> bool {
    true
}

fn default_bootstrap_buffer_size() -> usize {
    10000
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_query_language_defaults_to_gql() {
        // Test that when queryLanguage is omitted, it defaults to GQL
        let json = r#"{
            "id": "test-query",
            "query": "MATCH (n) RETURN n",
            "sources": []
        }"#;

        let dto: QueryConfigDto = serde_json::from_str(json).expect("Failed to parse JSON");
        assert_eq!(
            dto.query_language,
            ConfigValue::Static("GQL".to_string()),
            "Default query language should be GQL"
        );
    }

    #[test]
    fn test_query_language_can_be_explicitly_set_to_cypher() {
        // Test that Cypher can still be explicitly set
        let json = r#"{
            "id": "test-query",
            "query": "MATCH (n) RETURN n",
            "queryLanguage": "Cypher",
            "sources": []
        }"#;

        let dto: QueryConfigDto = serde_json::from_str(json).expect("Failed to parse JSON");
        assert_eq!(
            dto.query_language,
            ConfigValue::Static("Cypher".to_string()),
            "Should accept explicit Cypher setting"
        );
    }

    #[test]
    fn test_query_language_can_be_explicitly_set_to_gql() {
        // Test that GQL can be explicitly set
        let json = r#"{
            "id": "test-query",
            "query": "MATCH (n) RETURN n",
            "queryLanguage": "GQL",
            "sources": []
        }"#;

        let dto: QueryConfigDto = serde_json::from_str(json).expect("Failed to parse JSON");
        assert_eq!(
            dto.query_language,
            ConfigValue::Static("GQL".to_string()),
            "Should accept explicit GQL setting"
        );
    }

    #[test]
    fn test_other_defaults_unchanged() {
        // Ensure other defaults haven't changed
        let json = r#"{
            "id": "test-query",
            "query": "MATCH (n) RETURN n",
            "sources": []
        }"#;

        let dto: QueryConfigDto = serde_json::from_str(json).expect("Failed to parse JSON");
        assert_eq!(dto.auto_start, false, "auto_start should default to false");
        assert_eq!(
            dto.enable_bootstrap, true,
            "enable_bootstrap should default to true"
        );
        assert_eq!(
            dto.bootstrap_buffer_size, 10000,
            "bootstrap_buffer_size should default to 10000"
        );
    }
}
