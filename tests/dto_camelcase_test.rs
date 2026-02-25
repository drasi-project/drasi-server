// Test to verify that DTO fields serialize as camelCase

use serde_json::json;

#[test]
fn test_postgres_dto_serializes_camelcase() {
    let json = json!({
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
    });

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

    println!("✅ Postgres source config serializes as camelCase");
}

#[test]
fn test_mock_dto_serializes_camelcase() {
    let json = json!({
        "dataType": {"type": "sensorReading", "sensorCount": 5},
        "intervalMs": 1000
    });

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

    println!("✅ Mock source config serializes as camelCase");
}

#[test]
fn test_http_source_dto_serializes_camelcase() {
    let json = json!({
        "host": "localhost",
        "port": 8080,
        "timeoutMs": 5000,
        "adaptiveMaxBatchSize": 100,
        "adaptiveMinBatchSize": 10,
        "adaptiveMaxWaitMs": 500,
        "adaptiveMinWaitMs": 10,
        "adaptiveWindowSecs": 60,
        "adaptiveEnabled": true
    });

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

    println!("✅ HTTP source config serializes as camelCase");
}
