// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(test)]
mod api_query_joins_tests {
    use crate::api::handlers::*;
    use drasi_server_core::{
        channels::EventChannels,
        QueryConfig, QueryManager, SourceManager,
        config::{QueryJoinConfig, QueryJoinKeyConfig},
        routers::{DataRouter, BootstrapRouter}
    };
    use axum::{Extension, Json};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    async fn create_test_environment() -> (
        Arc<QueryManager>,
        Arc<SourceManager>,
        Arc<DataRouter>,
        Arc<BootstrapRouter>,
        Arc<bool>,
    ) {
        let (channels, _receivers) = EventChannels::new();
        
        let source_manager = Arc::new(SourceManager::new(
            channels.source_change_tx.clone(),
            channels.component_event_tx.clone(),
        ));
        
        let query_manager = Arc::new(QueryManager::new(
            channels.query_result_tx.clone(),
            channels.component_event_tx.clone(),
            channels.bootstrap_request_tx.clone(),
        ));
        
        let data_router = Arc::new(DataRouter::new());
        
        let bootstrap_router = Arc::new(BootstrapRouter::new());
        
        let read_only = Arc::new(false);
        
        (query_manager, source_manager, data_router, bootstrap_router, read_only)
    }

    #[tokio::test]
    async fn test_create_query_with_single_join_via_api() {
        let (query_manager, _source_manager, data_router, bootstrap_router, read_only) = 
            create_test_environment().await;

        // Create a query config with a single join
        let join_config = QueryJoinConfig {
            id: "VEHICLE_TO_DRIVER".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "Vehicle".to_string(),
                    property: "licensePlate".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "Driver".to_string(),
                    property: "vehicleLicensePlate".to_string(),
                },
            ],
        };

        let query_config = QueryConfig {
            id: "vehicle-driver-query".to_string(),
            query: "MATCH (d:Driver)-[:VEHICLE_TO_DRIVER]->(v:Vehicle) RETURN d.name, v.licensePlate".to_string(),
            sources: vec!["vehicles".to_string(), "drivers".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: Some(vec![join_config.clone()]),
        };

        // Call the API handler
        let result = create_query(
            Extension(query_manager.clone()),
            Extension(data_router),
            Extension(bootstrap_router),
            Extension(read_only),
            Json(query_config.clone()),
        ).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        // Response should be successful
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], true);

        // Verify the query was added with joins
        let retrieved = query_manager.get_query_config("vehicle-driver-query").await;
        assert!(retrieved.is_some());
        
        let retrieved_config = retrieved.unwrap();
        assert_eq!(retrieved_config.id, "vehicle-driver-query");
        assert!(retrieved_config.joins.is_some());
        
        let joins = retrieved_config.joins.unwrap();
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].id, "VEHICLE_TO_DRIVER");
        assert_eq!(joins[0].keys.len(), 2);
        assert_eq!(joins[0].keys[0].label, "Vehicle");
        assert_eq!(joins[0].keys[0].property, "licensePlate");
        assert_eq!(joins[0].keys[1].label, "Driver");
        assert_eq!(joins[0].keys[1].property, "vehicleLicensePlate");
    }

    #[tokio::test]
    async fn test_create_query_with_multiple_joins_via_api() {
        let (query_manager, _source_manager, data_router, bootstrap_router, read_only) = 
            create_test_environment().await;

        // Create multiple joins
        let restaurant_join = QueryJoinConfig {
            id: "ORDER_TO_RESTAURANT".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "Order".to_string(),
                    property: "restaurantId".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "Restaurant".to_string(),
                    property: "id".to_string(),
                },
            ],
        };

        let driver_join = QueryJoinConfig {
            id: "ORDER_TO_DRIVER".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "Order".to_string(),
                    property: "driverId".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "Driver".to_string(),
                    property: "id".to_string(),
                },
            ],
        };

        let query_config = QueryConfig {
            id: "full-order-query".to_string(),
            query: "MATCH (o:Order)-[:ORDER_TO_RESTAURANT]->(r:Restaurant), (o)-[:ORDER_TO_DRIVER]->(d:Driver) RETURN o.id, r.name, d.name".to_string(),
            sources: vec!["orders".to_string(), "restaurants".to_string(), "drivers".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: Some(vec![restaurant_join.clone(), driver_join.clone()]),
        };

        // Call the API handler
        let result = create_query(
            Extension(query_manager.clone()),
            Extension(data_router),
            Extension(bootstrap_router),
            Extension(read_only),
            Json(query_config.clone()),
        ).await;

        assert!(result.is_ok());
        
        // Verify the query was added with multiple joins
        let retrieved = query_manager.get_query_config("full-order-query").await;
        assert!(retrieved.is_some());
        
        let retrieved_config = retrieved.unwrap();
        assert!(retrieved_config.joins.is_some());
        
        let joins = retrieved_config.joins.unwrap();
        assert_eq!(joins.len(), 2);
        
        // Verify first join
        let first_join = joins.iter().find(|j| j.id == "ORDER_TO_RESTAURANT").unwrap();
        assert_eq!(first_join.keys[0].label, "Order");
        assert_eq!(first_join.keys[0].property, "restaurantId");
        assert_eq!(first_join.keys[1].label, "Restaurant");
        assert_eq!(first_join.keys[1].property, "id");
        
        // Verify second join
        let second_join = joins.iter().find(|j| j.id == "ORDER_TO_DRIVER").unwrap();
        assert_eq!(second_join.keys[0].label, "Order");
        assert_eq!(second_join.keys[0].property, "driverId");
        assert_eq!(second_join.keys[1].label, "Driver");
        assert_eq!(second_join.keys[1].property, "id");
    }

    #[tokio::test]
    async fn test_update_query_preserves_joins_via_api() {
        let (query_manager, _source_manager, data_router, bootstrap_router, read_only) = 
            create_test_environment().await;

        // Create initial query with joins
        let join_config = QueryJoinConfig {
            id: "USER_POST".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "Post".to_string(),
                    property: "authorId".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "User".to_string(),
                    property: "userId".to_string(),
                },
            ],
        };

        let initial_config = QueryConfig {
            id: "user-posts-query".to_string(),
            query: "MATCH (p:Post)-[:USER_POST]->(u:User) RETURN p.title, u.name".to_string(),
            sources: vec!["posts".to_string(), "users".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: Some(vec![join_config.clone()]),
        };

        // Create the query
        let _ = create_query(
            Extension(query_manager.clone()),
            Extension(data_router.clone()),
            Extension(bootstrap_router.clone()),
            Extension(read_only.clone()),
            Json(initial_config.clone()),
        ).await.unwrap();

        // Update the query (change the query string but keep joins)
        let updated_config = QueryConfig {
            id: "user-posts-query".to_string(),
            query: "MATCH (p:Post)-[:USER_POST]->(u:User) WHERE p.published = true RETURN p.title, u.name".to_string(),
            sources: vec!["posts".to_string(), "users".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: Some(vec![join_config.clone()]),
        };

        // Call the update API handler
        let update_result = update_query(
            Extension(query_manager.clone()),
            Extension(Arc::new(false)), // not read-only
            axum::extract::Path("user-posts-query".to_string()),
            Json(updated_config.clone()),
        ).await;

        assert!(update_result.is_ok());

        // Verify the joins are preserved
        let retrieved = query_manager.get_query_config("user-posts-query").await;
        assert!(retrieved.is_some());
        
        let retrieved_config = retrieved.unwrap();
        assert!(retrieved_config.joins.is_some());
        
        let joins = retrieved_config.joins.unwrap();
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].id, "USER_POST");
        
        // Verify the query string was updated
        assert!(retrieved_config.query.contains("WHERE p.published = true"));
    }

    #[tokio::test]
    async fn test_query_with_no_joins_via_api() {
        let (query_manager, _source_manager, data_router, bootstrap_router, read_only) = 
            create_test_environment().await;

        // Create a query without joins
        let query_config = QueryConfig {
            id: "simple-query".to_string(),
            query: "MATCH (n:Node) RETURN n".to_string(),
            sources: vec!["source1".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: None,
        };

        // Call the API handler
        let result = create_query(
            Extension(query_manager.clone()),
            Extension(data_router),
            Extension(bootstrap_router),
            Extension(read_only),
            Json(query_config.clone()),
        ).await;

        assert!(result.is_ok());

        // Verify the query was added without joins
        let retrieved = query_manager.get_query_config("simple-query").await;
        assert!(retrieved.is_some());
        
        let retrieved_config = retrieved.unwrap();
        assert_eq!(retrieved_config.id, "simple-query");
        assert!(retrieved_config.joins.is_none());
    }

    #[tokio::test]
    async fn test_query_with_empty_joins_array_via_api() {
        let (query_manager, _source_manager, data_router, bootstrap_router, read_only) = 
            create_test_environment().await;

        // Create a query with empty joins array
        let query_config = QueryConfig {
            id: "empty-joins-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["source1".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: Some(vec![]), // Empty joins array
        };

        // Call the API handler
        let result = create_query(
            Extension(query_manager.clone()),
            Extension(data_router),
            Extension(bootstrap_router),
            Extension(read_only),
            Json(query_config.clone()),
        ).await;

        assert!(result.is_ok());

        // Verify the query was added
        let retrieved = query_manager.get_query_config("empty-joins-query").await;
        assert!(retrieved.is_some());
        
        let retrieved_config = retrieved.unwrap();
        assert!(retrieved_config.joins.is_some());
        assert_eq!(retrieved_config.joins.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_get_query_returns_joins_via_api() {
        let (query_manager, _source_manager, data_router, bootstrap_router, read_only) = 
            create_test_environment().await;

        // Create a query with joins
        let join_config = QueryJoinConfig {
            id: "PRODUCT_CATEGORY".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "Product".to_string(),
                    property: "categoryId".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "Category".to_string(),
                    property: "id".to_string(),
                },
            ],
        };

        let query_config = QueryConfig {
            id: "product-category-query".to_string(),
            query: "MATCH (p:Product)-[:PRODUCT_CATEGORY]->(c:Category) RETURN p.name, c.name".to_string(),
            sources: vec!["products".to_string(), "categories".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: Some(vec![join_config.clone()]),
        };

        // Create the query
        let _ = create_query(
            Extension(query_manager.clone()),
            Extension(data_router),
            Extension(bootstrap_router),
            Extension(read_only),
            Json(query_config.clone()),
        ).await.unwrap();

        // Call the get_query API handler
        let get_result = get_query(
            Extension(query_manager.clone()),
            axum::extract::Path("product-category-query".to_string()),
        ).await;

        assert!(get_result.is_ok());
        
        let response = get_result.unwrap();
        // Verify the response contains joins
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], true);
        assert!(json_response["data"].is_object());
        
        let data = &json_response["data"];
        // The structure is data.id, data.query, data.sources, data.joins
        assert_eq!(data["id"], "product-category-query");
        assert!(data["joins"].is_array());
        
        let joins = data["joins"].as_array().unwrap();
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0]["id"], "PRODUCT_CATEGORY");
    }

    #[tokio::test]
    async fn test_json_serialization_of_query_with_joins() {
        // Test that query config with joins can be properly serialized/deserialized
        let join_config = QueryJoinConfig {
            id: "TEST_JOIN".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "NodeA".to_string(),
                    property: "propA".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "NodeB".to_string(),
                    property: "propB".to_string(),
                },
            ],
        };

        let query_config = QueryConfig {
            id: "test-query".to_string(),
            query: "MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b".to_string(),
            sources: vec!["source1".to_string(), "source2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: Some(vec![join_config]),
        };

        // Serialize to JSON
        let json = serde_json::to_value(&query_config).unwrap();
        
        // Verify JSON structure
        assert_eq!(json["id"], "test-query");
        assert_eq!(json["query"], "MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b");
        assert_eq!(json["sources"], json!(["source1", "source2"]));
        assert_eq!(json["auto_start"], true);
        
        assert!(json["joins"].is_array());
        let joins_array = json["joins"].as_array().unwrap();
        assert_eq!(joins_array.len(), 1);
        
        let first_join = &joins_array[0];
        assert_eq!(first_join["id"], "TEST_JOIN");
        assert_eq!(first_join["keys"].as_array().unwrap().len(), 2);
        assert_eq!(first_join["keys"][0]["label"], "NodeA");
        assert_eq!(first_join["keys"][0]["property"], "propA");
        assert_eq!(first_join["keys"][1]["label"], "NodeB");
        assert_eq!(first_join["keys"][1]["property"], "propB");
        
        // Deserialize back
        let deserialized: QueryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, query_config.id);
        assert!(deserialized.joins.is_some());
        assert_eq!(deserialized.joins.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_read_only_mode_blocks_query_creation_with_joins() {
        let (query_manager, _source_manager, data_router, bootstrap_router, _) = 
            create_test_environment().await;
        
        let read_only = Arc::new(true); // Set read-only mode

        let join_config = QueryJoinConfig {
            id: "TEST_JOIN".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "NodeA".to_string(),
                    property: "prop".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "NodeB".to_string(),
                    property: "prop".to_string(),
                },
            ],
        };

        let query_config = QueryConfig {
            id: "readonly-test-query".to_string(),
            query: "MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b".to_string(),
            sources: vec!["source1".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: Some(vec![join_config]),
        };

        // Try to create query in read-only mode
        let result = create_query(
            Extension(query_manager.clone()),
            Extension(data_router),
            Extension(bootstrap_router),
            Extension(read_only),
            Json(query_config),
        ).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        // Should fail due to read-only mode
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], false);
        assert!(json_response["error"].is_string());
        assert!(json_response["error"].as_str().unwrap().contains("read-only mode"));

        // Verify the query was NOT created
        let retrieved = query_manager.get_query_config("readonly-test-query").await;
        assert!(retrieved.is_none());
    }
}