// Test to verify that DTO fields serialize as camelCase

use drasi_server::api::models::*;

#[test]
fn test_postgres_dto_serializes_camelcase() {
    let dto = PostgresSourceConfigDto {
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
    };

    let json = serde_json::to_value(&dto).unwrap();

    // Verify fields are in camelCase
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

    println!("✅ PostgresSourceConfigDto serializes as camelCase");
}

#[test]
fn test_mock_dto_serializes_camelcase() {
    let dto = MockSourceConfigDto {
        data_type: ConfigValue::Static("sensor".to_string()),
        interval_ms: ConfigValue::Static(1000),
    };

    let json = serde_json::to_value(&dto).unwrap();

    // Verify fields are in camelCase
    assert!(json.get("dataType").is_some(), "dataType should exist");
    assert!(json.get("intervalMs").is_some(), "intervalMs should exist");

    // Verify snake_case versions don't exist
    assert!(
        json.get("data_type").is_none(),
        "data_type should NOT exist"
    );
    assert!(
        json.get("interval_ms").is_none(),
        "interval_ms should NOT exist"
    );

    println!("✅ MockSourceConfigDto serializes as camelCase");
}

#[test]
fn test_http_source_dto_serializes_camelcase() {
    let dto = HttpSourceConfigDto {
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
        webhooks: None,
    };

    let json = serde_json::to_value(&dto).unwrap();

    // Verify fields are in camelCase
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

    println!("✅ HttpSourceConfigDto serializes as camelCase");
}
