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
    use drasi_server_core::{Query, Reaction, Source};

    #[test]
    fn test_source_config_json_serialization() {
        let config = Source::mock("test-source")
            .auto_start(true)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-source");
        assert_eq!(json["source_type"], "mock");
        assert_eq!(json["auto_start"], true);
        // Mock source has typed config fields, not arbitrary properties
        assert!(json["data_type"].is_string());
        assert!(json["interval_ms"].is_number());

        // Test deserialization
        let deserialized: drasi_server_core::SourceConfig =
            serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, config.id);
    }

    #[test]
    fn test_query_config_json_serialization() {
        let config = Query::cypher("test-query")
            .query("MATCH (n) RETURN n")
            .from_source("source1")
            .from_source("source2")
            .auto_start(false)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-query");
        assert_eq!(json["query"], "MATCH (n) RETURN n");
        assert_eq!(json["source_subscriptions"].as_array().unwrap().len(), 2);

        // Test deserialization
        let deserialized: drasi_server_core::QueryConfig =
            serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.source_subscriptions.len(), 2);
    }

    #[test]
    fn test_reaction_config_json_serialization() {
        let config = Reaction::log("test-reaction")
            .subscribe_to("query1")
            .auto_start(true)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-reaction");
        assert_eq!(json["reaction_type"], "log");
        assert_eq!(json["queries"].as_array().unwrap().len(), 1);
        // Log reaction has typed config fields
        assert!(json["log_level"].is_string());
    }
}
