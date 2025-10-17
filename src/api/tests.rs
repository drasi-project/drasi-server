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

#[cfg(test)]
mod handler_tests {
    use super::super::*;
    use drasi_server_core::channels::ComponentStatus;

    #[tokio::test]
    async fn test_api_response_constructors() {
        // Test success constructor
        let response = ApiResponse::success("test data".to_string());
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"], "test data");
        assert!(json["error"].is_null());

        // Test error constructor
        let error_response: ApiResponse<String> =
            ApiResponse::error("Something went wrong".to_string());
        let json = serde_json::to_value(&error_response).unwrap();
        assert_eq!(json["success"], false);
        assert!(json["data"].is_null());
        assert_eq!(json["error"], "Something went wrong");
    }

    #[tokio::test]
    async fn test_component_status_serialization() {
        // Test that ComponentStatus can be serialized
        let status = ComponentStatus::Running;
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json, "Running");

        let stopped = ComponentStatus::Stopped;
        let json = serde_json::to_value(&stopped).unwrap();
        assert_eq!(json, "Stopped");
    }
}

#[cfg(test)]
mod serialization_tests {
    use drasi_server_core::{config::QueryLanguage, QueryConfig, ReactionConfig, SourceConfig};
    use serde_json::json;

    #[test]
    fn test_source_config_json_serialization() {
        let config = SourceConfig {
            id: "test-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            properties: std::collections::HashMap::from([("key".to_string(), json!("value"))]),
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-source");
        assert_eq!(json["source_type"], "mock");
        assert_eq!(json["auto_start"], true);
        assert_eq!(json["properties"]["key"], "value");

        // Test deserialization
        let deserialized: SourceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, config.id);
    }

    #[test]
    fn test_query_config_json_serialization() {
        let config = QueryConfig {
            id: "test-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["source1".to_string(), "source2".to_string()],
            auto_start: false,
            properties: std::collections::HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-query");
        assert_eq!(json["query"], "MATCH (n) RETURN n");
        assert_eq!(json["sources"].as_array().unwrap().len(), 2);

        // Test deserialization
        let deserialized: QueryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.sources.len(), 2);
    }

    #[test]
    fn test_reaction_config_json_serialization() {
        let config = ReactionConfig {
            id: "test-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["query1".to_string()],
            auto_start: true,
            properties: std::collections::HashMap::from([("log_level".to_string(), json!("info"))]),
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-reaction");
        assert_eq!(json["reaction_type"], "log");
        assert_eq!(json["queries"].as_array().unwrap().len(), 1);
        assert_eq!(json["properties"]["log_level"], "info");
    }
}
