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
    use crate::api::models::query::QueryConfigDto;
    use crate::api::shared::error::error_codes;
    use crate::api::shared::extractor::ConfigBody;
    use crate::api::shared::handlers::*;
    use crate::persistence::ConfigPersistence;
    use async_trait::async_trait;
    use axum::Extension;
    use drasi_lib::{
        channels::{
            dispatcher::{ChangeDispatcher, ChannelChangeDispatcher},
            ComponentStatus, SubscriptionResponse,
        },
        config::{QueryJoinConfig, QueryJoinKeyConfig, SourceSubscriptionSettings},
        context::SourceRuntimeContext,
        sources::Source,
        ComponentName, DrasiLib, Query, QueryConfig,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    fn name(value: &str) -> ComponentName {
        ComponentName::try_new(value).unwrap()
    }

    /// Minimal stub source for graph-backed query dependencies.
    struct StubSource {
        id: drasi_lib::ComponentId,
    }

    impl StubSource {
        fn new(id: &str) -> Self {
            let _ = id;
            Self {
                id: drasi_lib::ComponentId::generate(),
            }
        }
    }

    #[async_trait]
    impl Source for StubSource {
        fn id(&self) -> &drasi_lib::ComponentId {
            &self.id
        }
        fn type_name(&self) -> &str {
            "stub"
        }
        fn properties(&self) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
        fn auto_start(&self) -> bool {
            false
        }
        async fn start(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn status(&self) -> ComponentStatus {
            ComponentStatus::Stopped
        }
        async fn subscribe(
            &self,
            settings: SourceSubscriptionSettings,
        ) -> anyhow::Result<SubscriptionResponse> {
            let dispatcher =
                ChannelChangeDispatcher::<drasi_lib::channels::SourceEventWrapper>::new(10);
            let receiver = dispatcher.create_receiver().await?;
            Ok(SubscriptionResponse {
                query_id: settings.query_id,
                source_id: self.id.clone(),
                receiver,
                bootstrap_receiver: None,
                position_handle: None,
                bootstrap_result_receiver: None,
            })
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        async fn bind_runtime_context(&self, _context: SourceRuntimeContext) {}
    }

    async fn create_test_environment() -> (Arc<DrasiLib>, Arc<bool>, Option<Arc<ConfigPersistence>>)
    {
        // Register stub sources for all sources referenced by queries in these tests.
        // Supply each referenced source before starting dependent queries.
        let core = crate::query_test_support::builder()
            .with_id("test-server")
            .with_source(
                ComponentName::try_new("source1").unwrap(),
                StubSource::new("source1"),
            )
            .with_source(
                ComponentName::try_new("source2").unwrap(),
                StubSource::new("source2"),
            )
            .with_source(
                ComponentName::try_new("vehicles").unwrap(),
                StubSource::new("vehicles"),
            )
            .with_source(
                ComponentName::try_new("drivers").unwrap(),
                StubSource::new("drivers"),
            )
            .with_source(
                ComponentName::try_new("orders").unwrap(),
                StubSource::new("orders"),
            )
            .with_source(
                ComponentName::try_new("restaurants").unwrap(),
                StubSource::new("restaurants"),
            )
            .with_source(
                ComponentName::try_new("products").unwrap(),
                StubSource::new("products"),
            )
            .with_source(
                ComponentName::try_new("categories").unwrap(),
                StubSource::new("categories"),
            )
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

    // Helper function to convert QueryConfig to QueryConfigDto for testing
    fn query_config_to_dto(config: QueryConfig) -> QueryConfigDto {
        use crate::api::models::query::QueryResultConfigDto;

        QueryConfigDto {
            id: config.id,
            name: config.name,
            auto_start: config.auto_start,
            query: config.query,
            query_language: config.query_language,
            middleware: vec![], // Simplified for testing - middleware is complex
            inputs: config.inputs.into_iter().map(Into::into).collect(),
            result: QueryResultConfigDto::default(),
            enable_bootstrap: config.enable_bootstrap,
            joins: config.joins.map(|j| serde_json::to_value(j).unwrap()),
            priority_queue_capacity: config.priority_queue_capacity,
            dispatch_buffer_capacity: config.dispatch_buffer_capacity,
            dispatch_mode: config.dispatch_mode.map(|d| format!("{d:?}")),
            storage_backend: config
                .storage_backend
                .map(|s| serde_json::to_value(s).unwrap()),
            outbox_capacity: config.outbox_capacity,
            bootstrap_timeout_secs: config.bootstrap_timeout_secs,
        }
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

        let query_config = Query::cypher(name("vehicle-driver-query"))
            .query(
                "MATCH (d:Driver)-[:VEHICLE_TO_DRIVER]->(v:Vehicle) RETURN d.name, v.licensePlate",
            )
            .from_input_by_name(name("vehicles"))
            .from_input_by_name(name("drivers"))
            .auto_start(false)
            .with_joins(vec![join_config.clone()])
            .build()
            .unwrap();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Extension("test-server".to_string()),
            Extension(crate::query_test_support::registry()),
            ConfigBody(query_config_to_dto(query_config.clone())),
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

        let query_config = Query::cypher(name("full-order-query"))
            .query("MATCH (o:Order)-[:ORDER_TO_RESTAURANT]->(r:Restaurant), (o)-[:ORDER_TO_DRIVER]->(d:Driver) RETURN o.id, r.name, d.name")
            .from_input_by_name(name("orders"))
            .from_input_by_name(name("restaurants"))
            .from_input_by_name(name("drivers"))
            .auto_start(false)
            .with_joins(vec![restaurant_join.clone(), driver_join.clone()])
            .build()
            .unwrap();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Extension("test-server".to_string()),
            Extension(crate::query_test_support::registry()),
            ConfigBody(query_config_to_dto(query_config.clone())),
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
        let query_config = Query::cypher(name("simple-query"))
            .query("MATCH (n:Node) RETURN n")
            .from_input_by_name(name("source1"))
            .auto_start(false)
            .build()
            .unwrap();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Extension("test-server".to_string()),
            Extension(crate::query_test_support::registry()),
            ConfigBody(query_config_to_dto(query_config.clone())),
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
        let query_config = Query::cypher(name("empty-joins-query"))
            .query("MATCH (n) RETURN n")
            .from_input_by_name(name("source1"))
            .auto_start(false)
            .with_joins(vec![]) // Empty joins array
            .build()
            .unwrap();

        // Call the API handler
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Extension("test-server".to_string()),
            Extension(crate::query_test_support::registry()),
            ConfigBody(query_config_to_dto(query_config.clone())),
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

        let query_config = Query::cypher(name("product-category-query"))
            .query("MATCH (p:Product)-[:PRODUCT_CATEGORY]->(c:Category) RETURN p.name, c.name")
            .from_input_by_name(name("products"))
            .from_input_by_name(name("categories"))
            .auto_start(false)
            .with_joins(vec![join_config.clone()])
            .build()
            .unwrap();

        // Create the query
        let _ = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence.clone()),
            Extension("test-server".to_string()),
            Extension(crate::query_test_support::registry()),
            ConfigBody(query_config_to_dto(query_config.clone())),
        )
        .await
        .unwrap();

        // Call the get_query API handler
        let get_result = get_query(
            Extension(core.clone()),
            Extension(config_persistence),
            Extension("test-server".to_string()),
            Extension(crate::api::shared::handlers::ApiPrefix(
                "/api/v1".to_string(),
            )),
            axum::extract::Query(ComponentViewQuery::new(Some("full".to_string()))),
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
        // The structure is data.id, data.config.query, data.config.sources, data.config.joins
        assert!(uuid::Uuid::parse_str(data["id"].as_str().unwrap()).is_ok());
        assert_eq!(data["name"], "product-category-query");
        let joins_value = data["config"]["joins"].clone();
        let joins = if joins_value.is_array() {
            joins_value.as_array().unwrap().clone()
        } else if joins_value.is_object() {
            vec![joins_value]
        } else {
            vec![]
        };

        assert!(!joins.is_empty());
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

        let query_config = Query::cypher(name("test-query"))
            .query("MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b")
            .from_input_by_name(name("source1"))
            .from_input_by_name(name("source2"))
            .auto_start(true)
            .with_joins(vec![join_config])
            .build()
            .unwrap();

        // Serialize to JSON
        let json = serde_json::to_value(&query_config).unwrap();

        // Verify JSON structure
        assert!(json["id"].is_null());
        assert_eq!(json["name"], "test-query");
        assert_eq!(
            json["query"],
            "MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b"
        );
        assert_eq!(json["inputs"].as_array().unwrap().len(), 2);
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

        let query_config = Query::cypher(name("readonly-test-query"))
            .query("MATCH (a:NodeA)-[:TEST_JOIN]->(b:NodeB) RETURN a, b")
            .from_input_by_name(name("source1"))
            .auto_start(false)
            .with_joins(vec![join_config])
            .build()
            .unwrap();

        // Try to create query in read-only mode
        let result = create_query(
            Extension(core.clone()),
            Extension(read_only),
            Extension(config_persistence),
            Extension("test-server".to_string()),
            Extension(crate::query_test_support::registry()),
            ConfigBody(query_config_to_dto(query_config)),
        )
        .await;

        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected Err(ErrorResponse) for read-only mode"),
        };
        // Should fail due to read-only mode
        assert_eq!(err.code, error_codes::CONFIG_READ_ONLY);
        assert!(err.message.contains("read-only mode"));
    }
}
