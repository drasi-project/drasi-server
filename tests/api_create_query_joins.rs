use drasi_server_core::{
    QueryConfig, QueryManager, ComponentStatus, QueryResult, ComponentEvent,
    channels::BootstrapRequest,
    config::{QueryJoinConfig, QueryJoinKeyConfig},
    routers::{DataRouter, BootstrapRouter}
};
use axum::Extension;
use drasi_server::api::handlers::create_query;
use std::sync::Arc;

// Helper to build a minimal QueryConfig with joins
fn build_query_config() -> QueryConfig {
    QueryConfig {
        id: "watchlist-joined-query-test".to_string(),
        query: "MATCH (s:stocks)<-[:HAS_PRICE]-(sp:stock_prices) RETURN s.symbol AS symbol".to_string(),
        sources: vec!["postgres-stocks".to_string(), "price-feed".to_string()],
        auto_start: false,
        properties: Default::default(),
        joins: Some(vec![QueryJoinConfig {
            id: "HAS_PRICE".to_string(),
            keys: vec![
                QueryJoinKeyConfig { label: "stocks".to_string(), property: "symbol".to_string() },
                QueryJoinKeyConfig { label: "stock_prices".to_string(), property: "symbol".to_string() },
            ],
        }]),
    }
}

#[tokio::test]
async fn test_create_query_with_joins_via_handler() {
    // Channels required to instantiate managers
    let (result_tx, mut _result_rx) = tokio::sync::mpsc::channel::<QueryResult>(10);
    let (event_tx, mut _event_rx) = tokio::sync::mpsc::channel::<ComponentEvent>(10);
    let (bootstrap_req_tx, mut _bootstrap_req_rx) = tokio::sync::mpsc::channel::<BootstrapRequest>(10);

    let query_manager = Arc::new(QueryManager::new(result_tx, event_tx, bootstrap_req_tx));
    let data_router = Arc::new(DataRouter::new());
    let bootstrap_router = Arc::new(BootstrapRouter::new());
    let read_only = Arc::new(false);

    let cfg = build_query_config();

    // Invoke handler
    let _response = create_query(
        Extension(query_manager.clone()),
        Extension(data_router.clone()),
        Extension(bootstrap_router.clone()),
        Extension(read_only.clone()),
        axum::Json(cfg.clone())
    ).await.expect("handler should return Ok");

    // Retrieve stored config via QueryManager runtime
    let runtime = query_manager.get_query(cfg.id.clone()).await.expect("query runtime exists");
    assert_eq!(runtime.joins.as_ref().unwrap().len(), 1, "joins should be preserved");
    assert_eq!(runtime.joins.as_ref().unwrap()[0].id, "HAS_PRICE");
    assert_eq!(runtime.status, ComponentStatus::Stopped, "query not auto-started");
}