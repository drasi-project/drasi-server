use axum::Extension;
use drasi_server::api::handlers::create_query;
use drasi_server_core::{
    config::{QueryJoinConfig, QueryJoinKeyConfig},
    DrasiServerCore, Query, QueryConfig,
};
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

#[tokio::test]
async fn test_create_query_with_joins_via_handler() {
    // Create a minimal DrasiServerCore using the builder
    let core = DrasiServerCore::builder()
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

    // Invoke handler
    let response = create_query(
        Extension(core.clone()),
        Extension(read_only.clone()),
        Extension(config_persistence),
        axum::Json(cfg.clone()),
    )
    .await
    .expect("handler should return Ok");

    // Verify the API response is successful
    let json_response = serde_json::to_value(&response.0).unwrap();
    assert_eq!(json_response["success"], true);
}
