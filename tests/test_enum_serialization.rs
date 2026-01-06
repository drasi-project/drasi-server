// Test to verify that SourceConfig and ReactionConfig enums serialize as camelCase
// This tests the full enum wrappers with flattened DTO config structs

use drasi_server::api::models::*;
use serde_json;

#[test]
fn test_source_config_mock_serializes_camelcase() {
    let source = SourceConfig::Mock {
        id: "test-mock".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: MockSourceConfigDto {
            data_type: ConfigValue::Static("sensor".to_string()),
            interval_ms: ConfigValue::Static(1000),
        },
    };

    let json = serde_json::to_value(&source).unwrap();

    // Verify enum fields are camelCase (from enum-level rename_all)
    assert!(json.get("id").is_some(), "id should exist");
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(json.get("auto_start").is_none(), "auto_start should NOT exist");

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
    assert_eq!(json["dataType"], "sensor");
    assert_eq!(json["intervalMs"], 1000);

    println!("✅ SourceConfig::Mock serializes as camelCase");
}

#[test]
fn test_source_config_postgres_serializes_camelcase() {
    let source = SourceConfig::Postgres {
        id: "test-postgres".to_string(),
        auto_start: false,
        bootstrap_provider: None,
        config: PostgresSourceConfigDto {
            host: ConfigValue::Static("localhost".to_string()),
            port: ConfigValue::Static(5432),
            database: ConfigValue::Static("testdb".to_string()),
            user: ConfigValue::Static("testuser".to_string()),
            password: ConfigValue::Static("testpass".to_string()),
            tables: vec![],
            slot_name: "test_slot".to_string(),
            publication_name: "test_pub".to_string(),
            ssl_mode: ConfigValue::Static(SslModeDto::Disable),
            table_keys: vec![],
        },
    };

    let json = serde_json::to_value(&source).unwrap();

    // Verify enum fields are camelCase (from enum-level rename_all)
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(json.get("auto_start").is_none(), "auto_start should NOT exist");

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
    assert!(
        json.get("ssl_mode").is_none(),
        "ssl_mode should NOT exist"
    );

    println!("✅ SourceConfig::Postgres serializes as camelCase");
}

#[test]
fn test_source_config_http_serializes_camelcase() {
    let source = SourceConfig::Http {
        id: "test-http".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: HttpSourceConfigDto {
            host: ConfigValue::Static("localhost".to_string()),
            port: ConfigValue::Static(8080),
            endpoint: None,
            timeout_ms: ConfigValue::Static(5000),
            adaptive_max_batch_size: Some(ConfigValue::Static(100)),
            adaptive_min_batch_size: Some(ConfigValue::Static(10)),
            adaptive_max_wait_ms: Some(ConfigValue::Static(500)),
            adaptive_min_wait_ms: Some(ConfigValue::Static(10)),
            adaptive_window_secs: Some(ConfigValue::Static(60)),
            adaptive_enabled: Some(ConfigValue::Static(true)),
        },
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

    println!("✅ SourceConfig::Http serializes as camelCase");
}

#[test]
fn test_reaction_config_log_serializes_camelcase() {
    let reaction = ReactionConfig::Log {
        id: "test-log".to_string(),
        queries: vec!["query1".to_string()],
        auto_start: true,
        config: LogReactionConfigDto {
            routes: std::collections::HashMap::new(),
            default_template: None,
        },
    };

    let json = serde_json::to_value(&reaction).unwrap();

    // Verify enum fields are camelCase
    assert!(json.get("id").is_some(), "id should exist");
    assert!(json.get("queries").is_some(), "queries should exist");
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(json.get("auto_start").is_none(), "auto_start should NOT exist");

    println!("✅ ReactionConfig::Log serializes as camelCase");
}

#[test]
fn test_reaction_config_http_serializes_camelcase() {
    let reaction = ReactionConfig::Http {
        id: "test-http-reaction".to_string(),
        queries: vec!["query1".to_string()],
        auto_start: false,
        config: HttpReactionConfigDto {
            base_url: ConfigValue::Static("http://localhost:8080".to_string()),
            token: None,
            timeout_ms: ConfigValue::Static(5000),
            routes: Default::default(),
        },
    };

    let json = serde_json::to_value(&reaction).unwrap();

    // Verify enum fields are camelCase
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(json.get("auto_start").is_none(), "auto_start should NOT exist");

    // Verify flattened HTTP reaction config fields are also camelCase
    assert!(json.get("baseUrl").is_some(), "baseUrl should exist");
    assert!(json.get("timeoutMs").is_some(), "timeoutMs should exist");

    // Verify snake_case versions don't exist
    assert!(json.get("base_url").is_none(), "base_url should NOT exist");
    assert!(
        json.get("timeout_ms").is_none(),
        "timeout_ms should NOT exist"
    );

    println!("✅ ReactionConfig::Http serializes as camelCase");
}

#[test]
fn test_reaction_config_grpc_serializes_camelcase() {
    let reaction = ReactionConfig::Grpc {
        id: "test-grpc-reaction".to_string(),
        queries: vec!["query1".to_string()],
        auto_start: true,
        config: GrpcReactionConfigDto {
            endpoint: ConfigValue::Static("localhost:50051".to_string()),
            timeout_ms: ConfigValue::Static(3000),
            batch_size: ConfigValue::Static(50),
            batch_flush_timeout_ms: ConfigValue::Static(1000),
            max_retries: ConfigValue::Static(3),
            connection_retry_attempts: ConfigValue::Static(5),
            initial_connection_timeout_ms: ConfigValue::Static(10000),
            metadata: Default::default(),
        },
    };

    let json = serde_json::to_value(&reaction).unwrap();

    // Verify enum fields are camelCase
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(json.get("auto_start").is_none(), "auto_start should NOT exist");

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

    println!("✅ ReactionConfig::Grpc serializes as camelCase");
}
