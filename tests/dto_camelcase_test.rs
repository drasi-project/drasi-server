// Test to verify that DTO fields serialize as camelCase.
//
// These tests construct real Rust DTO types and verify that serde
// serializes their fields using camelCase, catching regressions in
// the `#[serde(rename_all = "camelCase")]` annotations.

use drasi_lib::config::QueryLanguage;
use drasi_server::api::models::observability::{
    ComponentEventDto, ComponentStatusDto, ComponentTypeDto, LogLevelDto, LogMessageDto,
};
use drasi_server::api::models::queries::query::{QueryConfigDto, SourceSubscriptionConfigDto};

#[test]
fn test_query_config_dto_serializes_camelcase() {
    let dto = QueryConfigDto {
        id: "test-query".to_string(),
        auto_start: true,
        query: "MATCH (n) RETURN n".to_string(),
        query_language: QueryLanguage::Cypher,
        middleware: vec![],
        sources: vec![SourceSubscriptionConfigDto {
            source_id: "my-source".to_string(),
            nodes: vec![],
            relations: vec![],
            pipeline: vec![],
        }],
        enable_bootstrap: true,
        bootstrap_buffer_size: 500,
        joins: None,
        priority_queue_capacity: Some(1000),
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
    };

    let json = serde_json::to_value(&dto).unwrap();

    // camelCase fields must exist
    assert!(json.get("autoStart").is_some(), "autoStart should exist");
    assert!(
        json.get("queryLanguage").is_some(),
        "queryLanguage should exist"
    );
    assert!(
        json.get("enableBootstrap").is_some(),
        "enableBootstrap should exist"
    );
    assert!(
        json.get("bootstrapBufferSize").is_some(),
        "bootstrapBufferSize should exist"
    );
    assert!(
        json.get("priorityQueueCapacity").is_some(),
        "priorityQueueCapacity should exist"
    );

    // snake_case equivalents must NOT exist
    assert!(
        json.get("auto_start").is_none(),
        "auto_start should NOT exist"
    );
    assert!(
        json.get("query_language").is_none(),
        "query_language should NOT exist"
    );
    assert!(
        json.get("enable_bootstrap").is_none(),
        "enable_bootstrap should NOT exist"
    );

    // Nested source subscription DTO
    let source = json["sources"].as_array().unwrap().first().unwrap();
    assert!(source.get("sourceId").is_some(), "sourceId should exist");
    assert!(
        source.get("source_id").is_none(),
        "source_id should NOT exist"
    );
}

#[test]
fn test_component_event_dto_serializes_camelcase() {
    let dto = ComponentEventDto {
        component_id: "my-source".to_string(),
        component_type: ComponentTypeDto::Source,
        status: ComponentStatusDto::Running,
        timestamp: chrono::Utc::now(),
        message: Some("started".to_string()),
    };

    let json = serde_json::to_value(&dto).unwrap();

    assert!(
        json.get("componentId").is_some(),
        "componentId should exist"
    );
    assert!(
        json.get("componentType").is_some(),
        "componentType should exist"
    );
    assert!(
        json.get("component_id").is_none(),
        "component_id should NOT exist"
    );
    assert!(
        json.get("component_type").is_none(),
        "component_type should NOT exist"
    );
}

#[test]
fn test_log_message_dto_serializes_camelcase() {
    let dto = LogMessageDto {
        timestamp: chrono::Utc::now(),
        level: LogLevelDto::Info,
        message: "test".to_string(),
        component_id: "my-source".to_string(),
        component_type: ComponentTypeDto::Source,
    };

    let json = serde_json::to_value(&dto).unwrap();

    assert!(
        json.get("componentId").is_some(),
        "componentId should exist"
    );
    assert!(
        json.get("componentType").is_some(),
        "componentType should exist"
    );
    assert!(
        json.get("component_id").is_none(),
        "component_id should NOT exist"
    );
    assert!(
        json.get("component_type").is_none(),
        "component_type should NOT exist"
    );
}
