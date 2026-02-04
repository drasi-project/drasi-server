//! OpenAPI Integration Tests
//!
//! Verifies that the OpenAPI spec correctly documents all endpoints and schemas.

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_server::api::v1::routes::build_v1_router;
use drasi_server::api::v1::openapi::ApiDocV1;
use drasi_server::instance_registry::InstanceRegistry;
use std::sync::Arc;
use tower::ServiceExt;
use utoipa::OpenApi;

async fn create_test_router() -> Router {
    let registry = InstanceRegistry::new();
    let read_only = Arc::new(false);
    let config_persistence = None;
    
    build_v1_router(registry, read_only, config_persistence)
}

#[test]
fn test_openapi_has_create_instance_endpoint() {
    let openapi = ApiDocV1::openapi();
    let json = serde_json::to_value(&openapi).unwrap();
    
    // Check that POST /api/v1/instances exists
    let instances_path = &json["paths"]["/api/v1/instances"];
    assert!(instances_path["post"].is_object(), "POST /api/v1/instances should exist");
    
    // Check that the request body has schema
    let request_body = &instances_path["post"]["requestBody"];
    assert!(request_body.is_object(), "POST /api/v1/instances should have a requestBody");
    
    let content = &request_body["content"]["application/json"]["schema"];
    
    // Schema can be inline or a $ref - either is acceptable
    if let Some(schema_ref) = content["$ref"].as_str() {
        println!("Schema ref: {}", schema_ref);
        assert!(
            schema_ref.contains("CreateInstanceRequest"),
            "Schema should reference CreateInstanceRequest, got: {}",
            schema_ref
        );
    } else {
        // Inline schema - verify it has the required properties
        println!("Inline schema: {}", serde_json::to_string_pretty(&content).unwrap());
        assert!(content["properties"]["id"].is_object(), "Inline schema should have 'id' property");
        assert!(
            content["properties"]["persistIndex"].is_object(),
            "Inline schema should have 'persistIndex' property"
        );
        assert!(
            content["properties"]["defaultPriorityQueueCapacity"].is_object(),
            "Inline schema should have 'defaultPriorityQueueCapacity' property"
        );
        assert!(
            content["properties"]["defaultDispatchBufferCapacity"].is_object(),
            "Inline schema should have 'defaultDispatchBufferCapacity' property"
        );
    }
}

#[test]
fn test_openapi_create_instance_request_has_all_fields() {
    let openapi = ApiDocV1::openapi();
    let json = serde_json::to_value(&openapi).unwrap();
    
    // Find the CreateInstanceRequest schema
    let schemas = &json["components"]["schemas"];
    
    // Find the schema (might be named differently)
    let mut create_instance_schema = None;
    if let Some(obj) = schemas.as_object() {
        for (key, value) in obj {
            if key.contains("CreateInstanceRequest") {
                create_instance_schema = Some((key.clone(), value.clone()));
                break;
            }
        }
    }
    
    let (schema_name, schema) = create_instance_schema
        .expect("CreateInstanceRequest schema should exist in components/schemas");
    
    println!("Found schema: {}", schema_name);
    println!("Schema: {}", serde_json::to_string_pretty(&schema).unwrap());
    
    let properties = &schema["properties"];
    assert!(properties.is_object(), "Schema should have properties");
    
    // Check for required fields
    assert!(properties["id"].is_object(), "Schema should have 'id' property");
    
    // Check for optional fields (camelCase due to #[serde(rename_all = "camelCase")])
    assert!(
        properties["persistIndex"].is_object(),
        "Schema should have 'persistIndex' property. Properties: {:?}",
        properties.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
    assert!(
        properties["defaultPriorityQueueCapacity"].is_object(),
        "Schema should have 'defaultPriorityQueueCapacity' property"
    );
    assert!(
        properties["defaultDispatchBufferCapacity"].is_object(),
        "Schema should have 'defaultDispatchBufferCapacity' property"
    );
}

#[tokio::test]
async fn test_create_instance_accepts_full_request() {
    let router = create_test_router().await;
    
    // Create instance with all fields
    let request_body = serde_json::json!({
        "id": "test-instance",
        "persistIndex": false,
        "defaultPriorityQueueCapacity": 10000,
        "defaultDispatchBufferCapacity": 1000
    });
    
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/instances")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    println!("Response status: {}", status);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    println!("Response body: {}", serde_json::to_string_pretty(&json).unwrap());
    
    assert_eq!(status, StatusCode::OK, "Create instance should succeed");
    assert_eq!(json["success"], true, "Response should indicate success");
}
