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

use drasi_server::{DrasiServerConfig, QueryConfig, ReactionConfig, ServerSettings, SourceConfig};
use drasi_server_core::config::QueryLanguage;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create example names for linking components
    let source_name = "vehicle-location-source".to_string();
    let query_name = "available-drivers-query".to_string();
    let reaction_name = "driver-availability-logger".to_string();

    // Create a basic server configuration with real Drasi patterns
    let config = DrasiServerConfig {
        server: ServerSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            disable_persistence: false,
        },
        core_config: drasi_server_core::config::DrasiServerCoreConfig {
            server_core: drasi_server_core::config::DrasiServerCoreSettings {
                id: uuid::Uuid::new_v4().to_string(),
                priority_queue_capacity: None,
                dispatch_buffer_capacity: None,
            },
            sources: vec![
                SourceConfig {
                    id: source_name.clone(),
                    source_type: "mock".to_string(),
                    auto_start: true,
                    properties: {
                        let mut props = HashMap::new();
                        props.insert(
                            "data_type".to_string(),
                            serde_json::json!("vehicle_location"),
                        );
                        props.insert("interval_seconds".to_string(), serde_json::json!(5));
                        props.insert(
                            "description".to_string(),
                            serde_json::json!("Mock vehicle location data"),
                        );
                        props
                    },
                    bootstrap_provider: None,
                    dispatch_buffer_capacity: None,
                    dispatch_mode: None,
                },
                SourceConfig {
                    id: "order-status-source".to_string(),
                    source_type: "mock".to_string(),
                    auto_start: true,
                    properties: {
                        let mut props = HashMap::new();
                        props.insert("data_type".to_string(), serde_json::json!("order_status"));
                        props.insert("interval_seconds".to_string(), serde_json::json!(3));
                        props.insert(
                            "description".to_string(),
                            serde_json::json!("Mock order status updates"),
                        );
                        props
                    },
                    bootstrap_provider: None,
                    dispatch_buffer_capacity: None,
                    dispatch_mode: None,
                },
            ],
            queries: vec![
                QueryConfig {
                    id: query_name.clone(),
                    query: r#"
                    MATCH (d:Driver {status: 'available'})
                    WHERE d.latitude IS NOT NULL AND d.longitude IS NOT NULL
                    RETURN elementId(d) AS driverId, d.driver_name AS driverName,
                           d.latitude AS lat, d.longitude AS lng, d.status AS status
                "#
                    .to_string(),
                    query_language: QueryLanguage::default(),
                    auto_start: true,
                    sources: vec![source_name.clone()],
                    properties: {
                        let mut props = HashMap::new();
                        props.insert(
                            "description".to_string(),
                            serde_json::json!("Find all available drivers with location data"),
                        );
                        props
                    },
                    joins: None,
                    enable_bootstrap: true,
                    bootstrap_buffer_size: 10000,
                    priority_queue_capacity: None,
                    dispatch_buffer_capacity: None,
                    dispatch_mode: None,
                },
                QueryConfig {
                    id: "pending-orders-query".to_string(),
                    query: r#"
                    MATCH (o:Order)
                    WHERE o.status IN ['pending', 'preparing', 'ready']
                    RETURN elementId(o) AS orderId, o.status AS status,
                           o.restaurant AS restaurant, o.delivery_address AS address
                "#
                    .to_string(),
                    query_language: QueryLanguage::default(),
                    auto_start: true,
                    sources: vec![source_name.clone()],
                    properties: {
                        let mut props = HashMap::new();
                        props.insert(
                            "description".to_string(),
                            serde_json::json!("Track orders that need processing"),
                        );
                        props
                    },
                    joins: None,
                    enable_bootstrap: true,
                    bootstrap_buffer_size: 10000,
                    priority_queue_capacity: None,
                    dispatch_buffer_capacity: None,
                    dispatch_mode: None,
                },
            ],
            reactions: vec![
                ReactionConfig {
                    id: reaction_name.clone(),
                    reaction_type: "log".to_string(),
                    auto_start: true,
                    queries: vec![query_name.clone()],
                    priority_queue_capacity: None,
                    properties: {
                        let mut props = HashMap::new();
                        props.insert("log_level".to_string(), serde_json::json!("info"));
                        props.insert(
                            "description".to_string(),
                            serde_json::json!("Log driver availability changes"),
                        );
                        props
                    },
                },
                ReactionConfig {
                    id: "order-notification-handler".to_string(),
                    reaction_type: "http".to_string(),
                    auto_start: true,
                    queries: vec![query_name.clone()],
                    priority_queue_capacity: None,
                    properties: {
                        let mut props = HashMap::new();
                        props.insert(
                            "endpoint".to_string(),
                            serde_json::json!("http://localhost:9000/notifications"),
                        );
                        props.insert("method".to_string(), serde_json::json!("POST"));
                        props.insert(
                            "description".to_string(),
                            serde_json::json!("Send notifications for query results"),
                        );
                        props
                    },
                },
            ],
        },
    };

    // Validate the configuration
    config.validate()?;

    // Save it to a file
    config.save_to_file("config/example.yaml")?;

    println!("✅ Example configuration created successfully!");
    println!("📝 Configuration saved to: config/example.yaml");
    println!("🚀 You can now run the server with: cargo run -- --config config/example.yaml");
    println!();
    println!("This example includes:");
    println!("  • Two mock data sources (vehicle locations and order status)");
    println!("  • Two Cypher queries (available drivers and pending orders)");
    println!("  • Two reactions (logging and webhook notifications)");
    println!("  • Real-time data processing using Drasi continuous queries");

    Ok(())
}
