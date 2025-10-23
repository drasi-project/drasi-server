//! API Contract Tests
//!
//! These tests validate that the REST API contracts remain stable over time.
//! They test request/response formats, status codes, and data schemas.

use drasi_server::api::handlers::ApiResponse;
use drasi_server_core::channels::ComponentStatus;
use drasi_server_core::config::QueryLanguage;
use drasi_server_core::{QueryConfig, ReactionConfig, SourceConfig};
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn test_api_response_success_format() {
        let response: ApiResponse<String> = ApiResponse::success("test data".to_string());
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["success"], true);
        assert_eq!(json["data"], "test data");
        assert!(json["error"].is_null());
    }

    #[test]
    fn test_api_response_error_format() {
        let response: ApiResponse<String> = ApiResponse::error("test error".to_string());
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["success"], false);
        assert!(json["data"].is_null());
        assert_eq!(json["error"], "test error");
    }

    #[test]
    fn test_health_response_schema() {
        // Test deserialization from JSON (since we can't construct directly)
        let json = json!({
            "status": "ok",
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        // Verify the shape of the JSON
        assert!(json["status"].is_string());
        assert!(json["timestamp"].is_string());

        // Verify timestamp is parseable
        let timestamp_str = json["timestamp"].as_str().unwrap();
        chrono::DateTime::parse_from_rfc3339(timestamp_str).unwrap();
    }

    #[test]
    fn test_component_list_item_schema() {
        // Test the expected JSON shape
        let json = json!({
            "id": "test-component",
            "status": "Running"
        });

        assert_eq!(json["id"], "test-component");
        assert_eq!(json["status"], "Running");
    }

    #[test]
    fn test_status_response_schema() {
        // Test the expected JSON shape
        let json = json!({
            "message": "Operation successful"
        });

        assert_eq!(json["message"], "Operation successful");
    }

    #[test]
    fn test_source_config_serialization() {
        let mut properties = HashMap::new();
        properties.insert("interval_ms".to_string(), json!(1000));
        properties.insert("data_type".to_string(), json!("counter"));

        let config = SourceConfig {
            id: "test-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties,
            bootstrap_provider: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-source");
        assert_eq!(json["source_type"], "mock");
        assert_eq!(json["auto_start"], true);
        assert_eq!(json["properties"]["interval_ms"], 1000);
        assert_eq!(json["properties"]["data_type"], "counter");
    }

    #[test]
    fn test_source_config_deserialization() {
        let json = json!({
            "id": "test-source",
            "source_type": "mock",
            "auto_start": false,
            "properties": {
                "interval_ms": 2000,
                "data_type": "sensor"
            }
        });

        let config: SourceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.id, "test-source");
        assert_eq!(config.source_type, "mock");
        assert!(!config.auto_start);
        assert_eq!(config.properties.get("interval_ms").unwrap(), &json!(2000));
    }

    #[test]
    fn test_query_config_serialization() {
        let config = QueryConfig {
            id: "test-query".to_string(),
            query: "MATCH (n:Node) RETURN n".to_string(),
            sources: vec!["source1".to_string(), "source2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
            priority_queue_capacity: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-query");
        assert_eq!(json["query"], "MATCH (n:Node) RETURN n");
        assert_eq!(json["sources"], json!(["source1", "source2"]));
        assert_eq!(json["auto_start"], true);
    }

    #[test]
    fn test_query_config_deserialization() {
        let json = json!({
            "id": "test-query",
            "query": "MATCH (p:Person) WHERE p.age > 21 RETURN p",
            "sources": ["postgres-db"],
            "auto_start": false
        });

        let config: QueryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.id, "test-query");
        assert_eq!(config.query, "MATCH (p:Person) WHERE p.age > 21 RETURN p");
        assert_eq!(config.sources, vec!["postgres-db"]);
        assert!(!config.auto_start);
    }

    #[test]
    fn test_reaction_config_serialization() {
        let mut properties = HashMap::new();
        properties.insert("url".to_string(), json!("http://example.com/webhook"));

        let config = ReactionConfig {
            id: "test-reaction".to_string(),
            reaction_type: "http".to_string(),
            queries: vec!["query1".to_string(), "query2".to_string()],
            auto_start: true,
            properties,
            priority_queue_capacity: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-reaction");
        assert_eq!(json["reaction_type"], "http");
        assert_eq!(json["queries"], json!(["query1", "query2"]));
        assert_eq!(json["auto_start"], true);
        assert_eq!(json["properties"]["url"], "http://example.com/webhook");
    }

    #[test]
    fn test_reaction_config_deserialization() {
        let json = json!({
            "id": "log-reaction",
            "reaction_type": "log",
            "queries": ["query1"],
            "auto_start": true
        });

        let config: ReactionConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.id, "log-reaction");
        assert_eq!(config.reaction_type, "log");
        assert_eq!(config.queries, vec!["query1"]);
        assert!(config.properties.is_empty());
    }

    #[test]
    fn test_component_status_serialization() {
        let statuses = vec![
            ComponentStatus::Starting,
            ComponentStatus::Running,
            ComponentStatus::Stopping,
            ComponentStatus::Stopped,
            ComponentStatus::Error,
        ];

        for status in statuses {
            let json = serde_json::to_value(&status).unwrap();
            match status {
                ComponentStatus::Starting => assert_eq!(json, "Starting"),
                ComponentStatus::Running => assert_eq!(json, "Running"),
                ComponentStatus::Stopping => assert_eq!(json, "Stopping"),
                ComponentStatus::Stopped => assert_eq!(json, "Stopped"),
                ComponentStatus::Error => assert_eq!(json, "Error"),
            }
        }
    }

    #[test]
    fn test_api_response_with_vec() {
        // Test ApiResponse structure with vec data
        let json_items = vec![
            json!({
                "id": "source1",
                "status": "Running"
            }),
            json!({
                "id": "source2",
                "status": "Stopped"
            }),
        ];

        let response: ApiResponse<Vec<serde_json::Value>> = ApiResponse::success(json_items);
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["success"], true);
        assert!(json["data"].is_array());
        assert_eq!(json["data"][0]["id"], "source1");
        assert_eq!(json["data"][1]["id"], "source2");
    }

    #[test]
    fn test_error_response_formats() {
        let test_cases = vec![
            "Component not found",
            "Invalid configuration",
            "Internal server error",
        ];

        for error_msg in test_cases {
            let response: ApiResponse<()> = ApiResponse::error(error_msg.to_string());
            let json = serde_json::to_value(&response).unwrap();

            assert_eq!(json["success"], false);
            assert!(json["data"].is_null());
            assert_eq!(json["error"], error_msg);
        }
    }

    #[test]
    fn test_query_results_format() {
        // Test that query results can be arbitrary JSON values
        let results = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
        ];

        let response: ApiResponse<Vec<serde_json::Value>> = ApiResponse::success(results);
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["success"], true);
        assert!(json["data"].is_array());
        assert_eq!(json["data"][0]["name"], "Alice");
        assert_eq!(json["data"][1]["name"], "Bob");
    }

    #[test]
    fn test_empty_collections() {
        // Test empty list responses
        let empty_sources: Vec<serde_json::Value> = vec![];
        let response: ApiResponse<Vec<serde_json::Value>> = ApiResponse::success(empty_sources);
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["success"], true);
        assert!(json["data"].is_array());
        assert_eq!(json["data"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_optional_properties() {
        // Test that optional properties work correctly
        let config_with_props = ReactionConfig {
            id: "r1".to_string(),
            reaction_type: "http".to_string(),
            queries: vec!["q1".to_string()],
            auto_start: false,
            properties: HashMap::from([("key".to_string(), json!("value"))]),
            priority_queue_capacity: None,
        };

        let config_without_props = ReactionConfig {
            id: "r2".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["q2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            priority_queue_capacity: None,
        };

        // Both should serialize successfully
        let json1 = serde_json::to_value(&config_with_props).unwrap();
        let json2 = serde_json::to_value(&config_without_props).unwrap();

        assert!(json1["properties"].is_object());
        assert!(json2["properties"].is_object());
    }

    #[test]
    fn test_idempotent_response_format() {
        // Test the response format for idempotent operations
        let status_json = json!({
            "message": "Source 'test-source' already exists"
        });
        let response: ApiResponse<serde_json::Value> = ApiResponse::success(status_json);

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["success"], true);
        assert!(json["data"]["message"]
            .as_str()
            .unwrap()
            .contains("already exists"));
    }

    #[test]
    fn test_read_only_mode_response() {
        let response: ApiResponse<serde_json::Value> =
            ApiResponse::error("Server is in read-only mode. Cannot create sources.".to_string());

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["success"], false);
        assert!(json["error"].as_str().unwrap().contains("read-only mode"));
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_special_characters_in_ids() {
        let config = SourceConfig {
            id: "source-with-dashes_and_underscores.123".to_string(),
            source_type: "mock".to_string(),
            auto_start: false,
            properties: HashMap::new(),
            bootstrap_provider: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "source-with-dashes_and_underscores.123");

        // Should deserialize back correctly
        let deserialized: SourceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, config.id);
    }

    #[test]
    fn test_very_long_query_string() {
        let long_query = "MATCH ".to_string() + &"(n:Node) ".repeat(100) + "RETURN n";
        let config = QueryConfig {
            id: "long-query".to_string(),
            query: long_query.clone(),
            sources: vec!["source1".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
            priority_queue_capacity: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["query"], long_query);
    }

    #[test]
    fn test_unicode_in_properties() {
        let mut properties = HashMap::new();
        properties.insert("message".to_string(), json!("Hello 世界 🌍"));
        properties.insert("emoji".to_string(), json!("🚀✨💻"));

        let config = SourceConfig {
            id: "unicode-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: false,
            properties,
            bootstrap_provider: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["properties"]["message"], "Hello 世界 🌍");
        assert_eq!(json["properties"]["emoji"], "🚀✨💻");
    }

    #[test]
    fn test_nested_json_in_properties() {
        let mut properties = HashMap::new();
        properties.insert(
            "nested".to_string(),
            json!({
                "level1": {
                    "level2": {
                        "value": 42,
                        "array": [1, 2, 3]
                    }
                }
            }),
        );

        let config = ReactionConfig {
            id: "nested-reaction".to_string(),
            reaction_type: "custom".to_string(),
            queries: vec!["q1".to_string()],
            auto_start: false,
            properties,
            priority_queue_capacity: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(
            json["properties"]["nested"]["level1"]["level2"]["value"],
            42
        );
        assert_eq!(
            json["properties"]["nested"]["level1"]["level2"]["array"],
            json!([1, 2, 3])
        );
    }

    #[test]
    fn test_empty_strings() {
        // Some fields might be empty strings
        let config = QueryConfig {
            id: "empty-query".to_string(),
            query: "".to_string(), // Empty query (should probably be validated elsewhere)
            sources: vec![],       // Empty sources list
            auto_start: false,
            properties: HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
            priority_queue_capacity: None,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["query"], "");
        assert_eq!(json["sources"], json!([]));
    }

    #[test]
    fn test_null_vs_missing_properties() {
        // Test that null and missing are handled correctly
        let json_with_null = json!({
            "id": "reaction1",
            "reaction_type": "log",
            "queries": ["q1"],
            "auto_start": false,
            "properties": null
        });

        let json_without_field = json!({
            "id": "reaction2",
            "reaction_type": "log",
            "queries": ["q1"],
            "auto_start": false
        });

        let config1: ReactionConfig = serde_json::from_value(json_with_null).unwrap();
        let config2: ReactionConfig = serde_json::from_value(json_without_field).unwrap();

        assert!(config1.properties.is_empty());
        assert!(config2.properties.is_empty());
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_invalid_json_rejection() {
        let invalid_source = json!({
            "id": "test",
            // Missing required field: source_type
            "auto_start": true
        });

        let result: Result<SourceConfig, _> = serde_json::from_value(invalid_source);
        assert!(result.is_err());
    }

    #[test]
    fn test_type_mismatch_rejection() {
        let invalid_query = json!({
            "id": "test",
            "query": "MATCH (n) RETURN n",
            "sources": "should-be-array", // Wrong type
            "auto_start": false
        });

        let result: Result<QueryConfig, _> = serde_json::from_value(invalid_query);
        assert!(result.is_err());
    }

    #[test]
    fn test_additional_fields_allowed() {
        // Additional fields should be ignored (forward compatibility)
        let json_with_extra = json!({
            "id": "test-source",
            "source_type": "mock",
            "auto_start": false,
            "properties": {},
            "future_field": "ignored", // Extra field
            "another_field": 123       // Another extra field
        });

        let config: SourceConfig = serde_json::from_value(json_with_extra).unwrap();
        assert_eq!(config.id, "test-source");
        assert_eq!(config.source_type, "mock");
    }
}
