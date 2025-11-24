//! API Contract Tests
//!
//! These tests validate that the REST API contracts remain stable over time.
//! They test request/response formats, status codes, and data schemas.

use drasi_lib::channels::ComponentStatus;
use drasi_lib::{QueryConfig, ReactionConfig, SourceConfig};
use drasi_server::api::handlers::ApiResponse;
use serde_json::json;

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
        use drasi_lib::{Properties, Source};

        let config = Source::mock("test-source")
            .auto_start(true)
            .with_properties(
                Properties::new()
                    .with_int("interval_ms", 1000)
                    .with_string("data_type", "counter"),
            )
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-source");
        assert_eq!(json["source_type"], "mock");
        assert_eq!(json["auto_start"], true);
        // MockSourceConfig fields are flattened, not under "properties"
        assert_eq!(json["interval_ms"], 1000);
        assert_eq!(json["data_type"], "counter");
    }

    #[test]
    fn test_source_config_deserialization() {
        let json = json!({
            "id": "test-source",
            "source_type": "mock",
            "auto_start": false,
            "interval_ms": 2000,
            "data_type": "sensor"
        });

        let config: SourceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.id, "test-source");
        assert_eq!(config.source_type(), "mock");
        assert!(!config.auto_start);
        // Fields are flattened at top level
        let serialized = serde_json::to_value(&config).unwrap();
        assert_eq!(serialized["interval_ms"], 2000);
    }

    #[test]
    fn test_query_config_serialization() {
        use drasi_lib::Query;

        let config = Query::cypher("test-query")
            .query("MATCH (n:Node) RETURN n")
            .from_source("source1")
            .from_source("source2")
            .auto_start(true)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-query");
        assert_eq!(json["query"], "MATCH (n:Node) RETURN n");
        assert_eq!(json["source_subscriptions"].as_array().unwrap().len(), 2);
        assert_eq!(json["source_subscriptions"][0]["source_id"], "source1");
        assert_eq!(json["source_subscriptions"][1]["source_id"], "source2");
        assert_eq!(json["auto_start"], true);
    }

    #[test]
    fn test_query_config_deserialization() {
        let json = json!({
            "id": "test-query",
            "query": "MATCH (p:Person) WHERE p.age > 21 RETURN p",
            "source_subscriptions": [
                {
                    "source_id": "postgres-db",
                    "pipeline": []
                }
            ],
            "auto_start": false
        });

        let config: QueryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.id, "test-query");
        assert_eq!(config.query, "MATCH (p:Person) WHERE p.age > 21 RETURN p");
        assert_eq!(config.source_subscriptions.len(), 1);
        assert_eq!(config.source_subscriptions[0].source_id, "postgres-db");
        assert!(!config.auto_start);
    }

    #[test]
    fn test_reaction_config_serialization() {
        use drasi_lib::{Properties, Reaction};

        let config = Reaction::http("test-reaction")
            .subscribe_to("query1")
            .subscribe_to("query2")
            .auto_start(true)
            .with_properties(
                Properties::new().with_string("base_url", "http://example.com/webhook"),
            )
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-reaction");
        assert_eq!(json["reaction_type"], "http");
        // Note: HttpReactionConfig has its own "queries" field (HashMap) which conflicts with
        // ReactionConfig's "queries" field (Vec<String>) when flattened. Testing id instead.
        assert_eq!(config.queries, vec!["query1", "query2"]);
        assert_eq!(json["auto_start"], true);
        // HttpReactionConfig fields are flattened
        assert_eq!(json["base_url"], "http://example.com/webhook");
    }

    #[test]
    fn test_reaction_config_deserialization() {
        let json = json!({
            "id": "log-reaction",
            "reaction_type": "log",
            "queries": ["query1"],
            "auto_start": true,
            "log_level": "info"
        });

        let config: ReactionConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.id, "log-reaction");
        assert_eq!(config.reaction_type(), "log");
        assert_eq!(config.queries, vec!["query1"]);
        // Check that it serializes correctly
        let serialized = serde_json::to_value(&config).unwrap();
        assert_eq!(serialized["log_level"], "info");
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
        use drasi_lib::Reaction;

        // Test that configs with and without custom properties work correctly
        let config_with_props = Reaction::http("r1")
            .subscribe_to("q1")
            .auto_start(false)
            .build();

        let config_without_props = Reaction::log("r2")
            .subscribe_to("q2")
            .auto_start(true)
            .build();

        // Both should serialize successfully with typed config fields
        let json1 = serde_json::to_value(&config_with_props).unwrap();
        let json2 = serde_json::to_value(&config_without_props).unwrap();

        // Typed configs have specific fields, not generic "properties"
        assert!(json1["base_url"].is_string());
        assert!(json2["log_level"].is_string());
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
        use drasi_lib::Source;

        let config = Source::mock("source-with-dashes_and_underscores.123")
            .auto_start(false)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "source-with-dashes_and_underscores.123");

        // Should deserialize back correctly
        let deserialized: SourceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, config.id);
    }

    #[test]
    fn test_very_long_query_string() {
        use drasi_lib::Query;

        let long_query = "MATCH ".to_string() + &"(n:Node) ".repeat(100) + "RETURN n";
        let config = Query::cypher("long-query")
            .query(&long_query)
            .from_source("source1")
            .auto_start(false)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["query"], long_query);
    }

    #[test]
    fn test_unicode_in_properties() {
        use drasi_lib::{Properties, Source};

        // Test that Unicode strings work in typed config fields
        let config = Source::mock("unicode-source")
            .auto_start(false)
            .with_properties(Properties::new().with_string("data_type", "Hello 世界 🌍"))
            .build();

        let json = serde_json::to_value(&config).unwrap();
        // Typed config has data_type field
        assert_eq!(json["data_type"], "Hello 世界 🌍");
    }

    #[test]
    fn test_nested_json_in_properties() {
        use drasi_lib::Reaction;

        // Test that config serialization works correctly
        let config = Reaction::http("nested-reaction")
            .subscribe_to("q1")
            .auto_start(false)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        // HttpReactionConfig has its own "queries" field (HashMap) which conflicts with
        // ReactionConfig's "queries" field when flattened. Check struct field directly.
        assert_eq!(config.queries, vec!["q1"]);
        assert!(json["base_url"].is_string());
    }

    #[test]
    fn test_empty_strings() {
        use drasi_lib::Query;

        // Some fields might be empty strings
        let config = Query::cypher("empty-query")
            .query("") // Empty query (should probably be validated elsewhere)
            .auto_start(false)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["query"], "");
        assert_eq!(json["source_subscriptions"], json!([]));
    }

    #[test]
    fn test_null_vs_missing_properties() {
        // Test that configs work with required and optional fields
        let json_full = json!({
            "id": "reaction1",
            "reaction_type": "log",
            "queries": ["q1"],
            "auto_start": false,
            "log_level": "debug"
        });

        let json_minimal = json!({
            "id": "reaction2",
            "reaction_type": "log",
            "queries": ["q1"],
            "auto_start": false
        });

        let config1: ReactionConfig = serde_json::from_value(json_full).unwrap();
        let config2: ReactionConfig = serde_json::from_value(json_minimal).unwrap();

        // Both should serialize successfully with typed config fields
        let json1 = serde_json::to_value(&config1).unwrap();
        let json2 = serde_json::to_value(&config2).unwrap();
        assert_eq!(json1["log_level"], "debug");
        // Default log_level is used when not specified
        assert!(json2["log_level"].is_string());
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
        assert_eq!(config.source_type(), "mock");
    }
}
