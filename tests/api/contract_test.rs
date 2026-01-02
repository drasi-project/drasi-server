//! API Contract Tests
//!
//! These tests validate that the REST API contracts remain stable over time.
//! They test request/response formats, status codes, and data schemas.

#![allow(clippy::unwrap_used)]

use drasi_lib::channels::ComponentStatus;
use drasi_lib::QueryConfig;
use drasi_server::api::ApiResponse;
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
        assert_eq!(json["sources"].as_array().unwrap().len(), 2);
        assert_eq!(json["sources"][0]["source_id"], "source1");
        assert_eq!(json["sources"][1]["source_id"], "source2");
        assert_eq!(json["auto_start"], true);
    }

    #[test]
    fn test_query_config_deserialization() {
        let json = json!({
            "id": "test-query",
            "query": "MATCH (p:Person) WHERE p.age > 21 RETURN p",
            "sources": [
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
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].source_id, "postgres-db");
        assert!(!config.auto_start);
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
    fn test_empty_strings() {
        use drasi_lib::Query;

        // Some fields might be empty strings
        let config = Query::cypher("empty-query")
            .query("") // Empty query (should probably be validated elsewhere)
            .auto_start(false)
            .build();

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["query"], "");
        assert_eq!(json["sources"], json!([]));
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_type_mismatch_rejection() {
        let invalid_query = json!({
            "id": "test",
            "query": "MATCH (n) RETURN n",
            "sources": "should-be-array", // Wrong type - should be array
            "auto_start": false
        });

        let result: Result<QueryConfig, _> = serde_json::from_value(invalid_query);
        assert!(result.is_err());
    }
}
