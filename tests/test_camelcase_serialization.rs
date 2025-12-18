// Test to verify that nested config fields are properly serialized as camelCase

use drasi_server::SourceConfig;
use drasi_source_mock::MockSourceConfig;
use drasi_source_postgres::PostgresSourceConfig;
use serde_json;

#[test]
fn test_mock_source_nested_fields_are_camelcase() {
    let source = SourceConfig::Mock {
        id: "test-mock".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: MockSourceConfig {
            data_type: "sensor".to_string(),
            interval_ms: 1000,
        },
    };
    
    let json_value = serde_json::to_value(&source).unwrap();
    
    // Verify nested fields from MockSourceConfig are in camelCase
    assert!(json_value.get("dataType").is_some(), "dataType should exist");
    assert!(json_value.get("intervalMs").is_some(), "intervalMs should exist");
    assert!(json_value.get("data_type").is_none(), "data_type should NOT exist");
    assert!(json_value.get("interval_ms").is_none(), "interval_ms should NOT exist");
    
    // Verify the values are correct
    assert_eq!(json_value["dataType"], "sensor");
    assert_eq!(json_value["intervalMs"], 1000);
    
    println!("✅ MockSourceConfig nested fields are properly camelCase");
}

#[test]
fn test_postgres_source_nested_fields_are_camelcase() {
    let config = PostgresSourceConfig {
        host: "localhost".to_string(),
        port: 5432,
        database: "testdb".to_string(),
        user: "testuser".to_string(),
        password: "testpass".to_string(),
        tables: vec![],
        slot_name: "test_slot".to_string(),
        publication_name: "test_pub".to_string(),
        ssl_mode: drasi_source_postgres::SslMode::Disable,
        table_keys: vec![],
    };
    
    let json_value = serde_json::to_value(&config).unwrap();
    
    // Verify fields are in camelCase
    assert!(json_value.get("slotName").is_some(), "slotName should exist");
    assert!(json_value.get("publicationName").is_some(), "publicationName should exist");
    assert!(json_value.get("tableKeys").is_some(), "tableKeys should exist");
    assert!(json_value.get("sslMode").is_some(), "sslMode should exist");
    
    assert!(json_value.get("slot_name").is_none(), "slot_name should NOT exist");
    assert!(json_value.get("publication_name").is_none(), "publication_name should NOT exist");
    assert!(json_value.get("table_keys").is_none(), "table_keys should NOT exist");
    assert!(json_value.get("ssl_mode").is_none(), "ssl_mode should NOT exist");
    
    // Verify the values are correct
    assert_eq!(json_value["slotName"], "test_slot");
    assert_eq!(json_value["publicationName"], "test_pub");
    
    println!("✅ PostgresSourceConfig fields are properly camelCase");
}
