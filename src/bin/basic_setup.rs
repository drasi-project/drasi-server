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

use drasi_server_core::{Properties, Query, Reaction, Source};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create example names for linking components
    let source_name = "vehicle-location-source";
    let query_name = "available-drivers-query";
    let reaction_name = "driver-availability-logger";

    println!("🔧 Creating example Drasi configuration...");
    println!();

    // Build source configurations using the new API
    let vehicle_source = Source::mock(source_name)
        .auto_start(true)
        .with_properties(
            Properties::new()
                .with_string("data_type", "vehicle_location")
                .with_int("interval_seconds", 5)
                .with_string("description", "Mock vehicle location data"),
        )
        .build();

    let order_source = Source::mock("order-status-source")
        .auto_start(true)
        .with_properties(
            Properties::new()
                .with_string("data_type", "order_status")
                .with_int("interval_seconds", 3)
                .with_string("description", "Mock order status updates"),
        )
        .build();

    // Build query configurations
    let available_drivers_query = Query::cypher(query_name)
        .query(
            r#"
            MATCH (d:Driver {status: 'available'})
            WHERE d.latitude IS NOT NULL AND d.longitude IS NOT NULL
            RETURN elementId(d) AS driverId, d.driver_name AS driverName,
                   d.latitude AS lat, d.longitude AS lng, d.status AS status
        "#,
        )
        .from_source(source_name)
        .auto_start(true)
        .build();

    let pending_orders_query = Query::cypher("pending-orders-query")
        .query(
            r#"
            MATCH (o:Order)
            WHERE o.status IN ['pending', 'preparing', 'ready']
            RETURN elementId(o) AS orderId, o.status AS status,
                   o.restaurant AS restaurant, o.delivery_address AS address
        "#,
        )
        .from_source(source_name)
        .auto_start(true)
        .build();

    // Build reaction configurations
    let log_reaction = Reaction::log(reaction_name)
        .subscribe_to(query_name)
        .auto_start(true)
        .with_properties(
            Properties::new()
                .with_string("log_level", "info")
                .with_string("description", "Log driver availability changes"),
        )
        .build();

    let http_reaction = Reaction::http("order-notification-handler")
        .subscribe_to(query_name)
        .auto_start(true)
        .with_properties(
            Properties::new()
                .with_string("endpoint", "http://localhost:9000/notifications")
                .with_string("method", "POST")
                .with_string("description", "Send notifications for query results"),
        )
        .build();

    // Create the configuration structure
    let config = drasi_server_core::config::DrasiServerCoreConfig {
        server_core: drasi_server_core::config::DrasiServerCoreSettings::default(),
        sources: vec![vehicle_source, order_source],
        queries: vec![available_drivers_query, pending_orders_query],
        reactions: vec![log_reaction, http_reaction],
        storage_backends: vec![],
    };

    // Save configuration to file
    std::fs::create_dir_all("config")?;
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
