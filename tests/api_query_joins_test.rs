#![allow(clippy::unwrap_used)]

use axum::Extension;
use drasi_lib::{
    config::{QueryJoinConfig, QueryJoinKeyConfig},
    DrasiLib, Query, QueryConfig,
};
use drasi_server::api::models::query::{QueryConfigDto, SourceSubscriptionConfigDto};
use drasi_server::api::models::ConfigValue;
use drasi_server::api::shared::handlers::create_query;
use std::sync::Arc;

// Helper to build a minimal QueryConfig with joins
fn build_query_config() -> QueryConfig {
    Query::cypher("watchlist-joined-query-test")
        .query("MATCH (s:stocks)<-[:HAS_PRICE]-(sp:stock_prices) RETURN s.symbol AS symbol")
        .from_source("postgres-stocks")
        .from_source("price-feed")
        .auto_start(false)
        .with_joins(vec![QueryJoinConfig {
            id: "HAS_PRICE".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "stocks".to_string(),
                    property: "symbol".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "stock_prices".to_string(),
                    property: "symbol".to_string(),
                },
            ],
        }])
        .build()
}

// Helper to convert QueryConfig to QueryConfigDto for API calls
fn query_config_to_dto(config: QueryConfig) -> QueryConfigDto {
    QueryConfigDto {
        id: config.id,
        auto_start: config.auto_start,
        query: ConfigValue::Static(config.query),
        query_language: ConfigValue::Static(format!("{:?}", config.query_language)),
        middleware: vec![], // Simplified for testing
        sources: config.sources.iter().map(|s| SourceSubscriptionConfigDto {
            source_id: ConfigValue::Static(s.source_id.clone()),
            nodes: s.nodes.clone(),
            relations: s.relations.clone(),
            pipeline: s.pipeline.clone(),
        }).collect(),
        enable_bootstrap: config.enable_bootstrap,
        bootstrap_buffer_size: config.bootstrap_buffer_size,
        joins: config.joins.map(|j| serde_json::to_value(j).unwrap()),
        priority_queue_capacity: config.priority_queue_capacity,
        dispatch_buffer_capacity: config.dispatch_buffer_capacity,
        dispatch_mode: config.dispatch_mode.map(|d| format!("{:?}", d)),
        storage_backend: config.storage_backend.map(|s| serde_json::to_value(s).unwrap()),
    }
}

#[tokio::test]
async fn test_create_query_with_joins_via_handler() {
    // Create a minimal DrasiLib using the builder
    let core = DrasiLib::builder()
        .with_id("test-server")
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start the core
    core.start().await.expect("Failed to start core");

    let read_only = Arc::new(false);
    let config_persistence: Option<Arc<drasi_server::persistence::ConfigPersistence>> = None;

    let cfg = build_query_config();
    let cfg_dto = query_config_to_dto(cfg);

    // Invoke handler
    let response = create_query(
        Extension(core.clone()),
        Extension(read_only.clone()),
        Extension(config_persistence),
        Extension("test-server".to_string()),
        axum::Json(cfg_dto),
    )
    .await
    .expect("handler should return Ok");

    // Verify the API response is successful
    let json_response = serde_json::to_value(&response.0).unwrap();
    assert_eq!(json_response["success"], true);
}
