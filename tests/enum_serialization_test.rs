// Test to verify that SourceConfig and ReactionConfig structs serialize as camelCase
// This tests the struct wrappers with serde_json::Value config fields

use drasi_server::api::models::*;
use serde_json::json;

#[test]
fn test_source_config_mock_serializes_camelcase() {
    let source = SourceConfig {
        kind: "mock".to_string(),
        id: "test-mock".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: json!({"dataType": {"type": "sensorReading", "sensorCount": 5}, "intervalMs": 1000}),
    };

    let json = serde_json::to_value(&source).unwrap();

    // Verify enum fields are camelCase (with rename_all_fields on enum)
    assert!(json.get("id").is_some(), "id should exist");
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(
        json.get("auto_start").is_none(),
        "auto_start should NOT exist"
    );

    // Verify flattened config fields are also camelCase (from DTO struct-level rename_all)
    assert!(json.get("dataType").is_some(), "dataType should exist");
    assert!(json.get("intervalMs").is_some(), "intervalMs should exist");

    // Verify snake_case versions of FLATTENED fields don't exist
    assert!(
        json.get("data_type").is_none(),
        "data_type should NOT exist"
    );
    assert!(
        json.get("interval_ms").is_none(),
        "interval_ms should NOT exist"
    );

    // Verify values are correct
    assert_eq!(json["id"], "test-mock");
    assert_eq!(json["autoStart"], true);
    // DataType is now an object for SensorReading variant
    let data_type = &json["dataType"];
    assert_eq!(data_type["type"], "sensorReading");
    assert_eq!(data_type["sensorCount"], 5);
    assert_eq!(json["intervalMs"], 1000);

    println!("✅ SourceConfig mock serializes correctly");
}

#[test]
fn test_source_config_postgres_serializes_camelcase() {
    let source = SourceConfig {
        kind: "postgres".to_string(),
        id: "test-postgres".to_string(),
        auto_start: false,
        bootstrap_provider: None,
        config: json!({
            "host": "localhost",
            "port": 5432,
            "database": "testdb",
            "user": "testuser",
            "password": "testpass",
            "tables": [],
            "slotName": "test_slot",
            "publicationName": "test_pub",
            "sslMode": "disable",
            "tableKeys": []
        }),
    };

    let json = serde_json::to_value(&source).unwrap();

    // Verify enum fields are camelCase (with rename_all_fields on enum)
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(
        json.get("auto_start").is_none(),
        "auto_start should NOT exist"
    );

    // Verify flattened Postgres config fields are also camelCase
    assert!(json.get("slotName").is_some(), "slotName should exist");
    assert!(
        json.get("publicationName").is_some(),
        "publicationName should exist"
    );
    assert!(json.get("tableKeys").is_some(), "tableKeys should exist");
    assert!(json.get("sslMode").is_some(), "sslMode should exist");

    // Verify snake_case versions don't exist
    assert!(
        json.get("slot_name").is_none(),
        "slot_name should NOT exist"
    );
    assert!(
        json.get("publication_name").is_none(),
        "publication_name should NOT exist"
    );
    assert!(
        json.get("table_keys").is_none(),
        "table_keys should NOT exist"
    );
    assert!(json.get("ssl_mode").is_none(), "ssl_mode should NOT exist");

    println!("✅ SourceConfig postgres serializes as camelCase");
}

#[test]
fn test_source_config_http_serializes_camelcase() {
    let source = SourceConfig {
        kind: "http".to_string(),
        id: "test-http".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: json!({
            "host": "localhost",
            "port": 8080,
            "timeoutMs": 5000,
            "adaptiveMaxBatchSize": 100,
            "adaptiveMinBatchSize": 10,
            "adaptiveMaxWaitMs": 500,
            "adaptiveMinWaitMs": 10,
            "adaptiveWindowSecs": 60,
            "adaptiveEnabled": true
        }),
    };

    let json = serde_json::to_value(&source).unwrap();

    // Verify flattened HTTP config fields are camelCase
    assert!(json.get("timeoutMs").is_some(), "timeoutMs should exist");
    assert!(
        json.get("adaptiveMaxBatchSize").is_some(),
        "adaptiveMaxBatchSize should exist"
    );
    assert!(
        json.get("adaptiveMinBatchSize").is_some(),
        "adaptiveMinBatchSize should exist"
    );

    // Verify snake_case versions don't exist
    assert!(
        json.get("timeout_ms").is_none(),
        "timeout_ms should NOT exist"
    );
    assert!(
        json.get("adaptive_max_batch_size").is_none(),
        "adaptive_max_batch_size should NOT exist"
    );

    println!("✅ SourceConfig http serializes as camelCase");
}

#[test]
fn test_reaction_config_log_serializes_camelcase() {
    let reaction = ReactionConfig {
        kind: "log".to_string(),
        id: "test-log".to_string(),
        queries: vec!["query1".to_string()],
        auto_start: true,
        config: json!({"routes": {}}),
    };

    let json = serde_json::to_value(&reaction).unwrap();

    // Verify enum fields are camelCase
    assert!(json.get("id").is_some(), "id should exist");
    assert!(json.get("queries").is_some(), "queries should exist");
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(
        json.get("auto_start").is_none(),
        "auto_start should NOT exist"
    );

    println!("✅ ReactionConfig log serializes as camelCase");
}

#[test]
fn test_reaction_config_http_serializes_camelcase() {
    let reaction = ReactionConfig {
        kind: "http".to_string(),
        id: "test-http-reaction".to_string(),
        queries: vec!["query1".to_string()],
        auto_start: false,
        config: json!({
            "baseUrl": "http://localhost:8080",
            "timeoutMs": 5000,
            "routes": {}
        }),
    };

    let json = serde_json::to_value(&reaction).unwrap();

    // Verify enum fields are camelCase
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(
        json.get("auto_start").is_none(),
        "auto_start should NOT exist"
    );

    // Verify flattened HTTP reaction config fields are also camelCase
    assert!(json.get("baseUrl").is_some(), "baseUrl should exist");
    assert!(json.get("timeoutMs").is_some(), "timeoutMs should exist");

    // Verify snake_case versions don't exist
    assert!(json.get("base_url").is_none(), "base_url should NOT exist");
    assert!(
        json.get("timeout_ms").is_none(),
        "timeout_ms should NOT exist"
    );

    println!("✅ ReactionConfig http serializes as camelCase");
}

#[test]
fn test_reaction_config_grpc_serializes_camelcase() {
    let reaction = ReactionConfig {
        kind: "grpc".to_string(),
        id: "test-grpc-reaction".to_string(),
        queries: vec!["query1".to_string()],
        auto_start: true,
        config: json!({
            "endpoint": "localhost:50051",
            "timeoutMs": 3000,
            "batchSize": 50,
            "batchFlushTimeoutMs": 1000,
            "maxRetries": 3,
            "connectionRetryAttempts": 5,
            "initialConnectionTimeoutMs": 10000,
            "metadata": {}
        }),
    };

    let json = serde_json::to_value(&reaction).unwrap();

    // Verify enum fields are camelCase
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(
        json.get("auto_start").is_none(),
        "auto_start should NOT exist"
    );

    // Verify flattened gRPC reaction config fields are also camelCase
    assert!(json.get("timeoutMs").is_some(), "timeoutMs should exist");
    assert!(json.get("batchSize").is_some(), "batchSize should exist");
    assert!(json.get("maxRetries").is_some(), "maxRetries should exist");
    assert!(
        json.get("batchFlushTimeoutMs").is_some(),
        "batchFlushTimeoutMs should exist"
    );
    assert!(
        json.get("connectionRetryAttempts").is_some(),
        "connectionRetryAttempts should exist"
    );
    assert!(
        json.get("initialConnectionTimeoutMs").is_some(),
        "initialConnectionTimeoutMs should exist"
    );

    // Verify snake_case versions don't exist
    assert!(
        json.get("timeout_ms").is_none(),
        "timeout_ms should NOT exist"
    );
    assert!(
        json.get("batch_size").is_none(),
        "batch_size should NOT exist"
    );
    assert!(
        json.get("max_retries").is_none(),
        "max_retries should NOT exist"
    );
    assert!(
        json.get("batch_flush_timeout_ms").is_none(),
        "batch_flush_timeout_ms should NOT exist"
    );
    assert!(
        json.get("connection_retry_attempts").is_none(),
        "connection_retry_attempts should NOT exist"
    );
    assert!(
        json.get("initial_connection_timeout_ms").is_none(),
        "initial_connection_timeout_ms should NOT exist"
    );

    println!("✅ ReactionConfig grpc serializes as camelCase");
}
