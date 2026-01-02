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
#[allow(clippy::unwrap_used)]
mod api_query_joins_tests {
    use crate::api::shared::handlers::*;
    use crate::persistence::ConfigPersistence;
    use axum::{Extension, Json};
    use drasi_lib::{
        config::{QueryJoinConfig, QueryJoinKeyConfig},
        DrasiLib, Query, QueryConfig,
    };
    use std::sync::Arc;

    async fn create_test_environment() -> (Arc<DrasiLib>, Arc<bool>, Option<Arc<ConfigPersistence>>)
    {
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
        let config_persistence: Option<Arc<ConfigPersistence>> = None;

        (core, read_only, config_persistence)
    }

    #[tokio::test]
    async fn test_create_query_with_single_join_via_api() {
        let (core, read_only, config_persistence) = create_test_environment().await;

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

        let query_config = Query::cypher("vehicle-driver-query")
            .query(
                "MATCH (d:Driver)-[:VEHICLE_TO_DRIVER]->(v:Vehicle) RETURN d.name, v.licensePlate",
            )
            .from_source("vehicles")
            .from_source("drivers")
            .auto_start(false)
            .with_joins(vec![join_config.clone()])
            .build();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Json(query_config.clone()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        // Response should be successful
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], true);
    }

    #[tokio::test]
    async fn test_create_query_with_multiple_joins_via_api() {
        let (core, read_only, config_persistence) = create_test_environment().await;

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

        let query_config = Query::cypher("full-order-query")
            .query("MATCH (o:Order)-[:ORDER_TO_RESTAURANT]->(r:Restaurant), (o)-[:ORDER_TO_DRIVER]->(d:Driver) RETURN o.id, r.name, d.name")
            .from_source("orders")
            .from_source("restaurants")
            .from_source("drivers")
            .auto_start(false)
            .with_joins(vec![restaurant_join.clone(), driver_join.clone()])
            .build();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Json(query_config.clone()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        // Response should be successful
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], true);
    }

    #[tokio::test]
    async fn test_query_with_no_joins_via_api() {
        let (core, read_only, config_persistence) = create_test_environment().await;

        // Create a query without joins
        let query_config = Query::cypher("simple-query")
            .query("MATCH (n:Node) RETURN n")
            .from_source("source1")
            .auto_start(false)
            .build();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Json(query_config.clone()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        // Response should be successful
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], true);
    }

    #[tokio::test]
    async fn test_query_with_empty_joins_array_via_api() {
        let (core, read_only, config_persistence) = create_test_environment().await;

        // Create a query with empty joins array
        let query_config = Query::cypher("empty-joins-query")
            .query("MATCH (n) RETURN n")
            .from_source("source1")
            .auto_start(false)
            .with_joins(vec![]) // Empty joins array
            .build();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Json(query_config.clone()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        // Response should be successful
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], true);
    }

    #[tokio::test]
    async fn test_get_query_returns_joins_via_api() {
        let (core, read_only, config_persistence) = create_test_environment().await;

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

        let query_config = Query::cypher("product-category-query")
            .query("MATCH (p:Product)-[:PRODUCT_CATEGORY]->(c:Category) RETURN p.name, c.name")
            .from_source("products")
            .from_source("categories")
            .auto_start(false)
            .with_joins(vec![join_config.clone()])
            .build();

        // Create the query
        let _ = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Json(query_config.clone()),
        )
        .await
        .unwrap();

        // Call the get_query API handler
        let get_result = get_query(
            Extension(core.clone()),
            axum::extract::Path("product-category-query".to_string()),
        )
        .await;

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

        let query_config = Query::cypher("test-query")
            .query("MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b")
            .from_source("source1")
            .from_source("source2")
            .auto_start(true)
            .with_joins(vec![join_config])
            .build();

        // Serialize to JSON
        let json = serde_json::to_value(&query_config).unwrap();

        // Verify JSON structure
        assert_eq!(json["id"], "test-query");
        assert_eq!(
            json["query"],
            "MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b"
        );
        assert_eq!(json["sources"].as_array().unwrap().len(), 2);
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
        let (core, _, config_persistence) = create_test_environment().await;

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

        let query_config = Query::cypher("readonly-test-query")
            .query("MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b")
            .from_source("source1")
            .auto_start(false)
            .with_joins(vec![join_config])
            .build();

        // Try to create query in read-only mode
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Json(query_config),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        // Should fail due to read-only mode
        let json_response = serde_json::to_value(&response.0).unwrap();
        assert_eq!(json_response["success"], false);
        assert!(json_response["error"].is_string());
        assert!(json_response["error"]
            .as_str()
            .unwrap()
            .contains("read-only mode"));
    }
}
