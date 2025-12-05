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

//! Example: Basic Drasi Configuration Setup
//!
//! This example demonstrates how to create a Drasi configuration file
//! with queries. Sources and reactions are created as instances and passed
//! directly to the builder.

use drasi_lib::Query;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating example Drasi configuration...");
    println!();

    // Build query configurations using the Query builder
    let available_drivers_query = Query::cypher("available-drivers-query")
        .query(
            r#"
            MATCH (d:Driver {status: 'available'})
            WHERE d.latitude IS NOT NULL AND d.longitude IS NOT NULL
            RETURN elementId(d) AS driverId, d.driver_name AS driverName,
                   d.latitude AS lat, d.longitude AS lng, d.status AS status
        "#,
        )
        .from_source("vehicle-location-source")
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
        .from_source("vehicle-location-source")
        .auto_start(true)
        .build();

    // Create the configuration structure
    // Note: Sources and reactions can be defined in the config file using the tagged enum format
    let config = drasi_server::DrasiServerConfig {
        server: drasi_server::ServerSettings::default(),
        sources: vec![], // Add sources using SourceConfig enum
        reactions: vec![], // Add reactions using ReactionConfig enum
        core_config: drasi_lib::config::DrasiLibConfig {
            server_core: drasi_lib::config::DrasiLibSettings::default(),
            queries: vec![available_drivers_query, pending_orders_query],
            storage_backends: vec![],
        },
    };

    // Save configuration to file
    std::fs::create_dir_all("config")?;
    config.save_to_file("config/example.yaml")?;

    println!("Example configuration created successfully!");
    println!("Configuration saved to: config/example.yaml");
    println!();
    println!("This example includes:");
    println!("  - Two Cypher queries (available drivers and pending orders)");
    println!();
    println!("Note: Sources and reactions are created as instances and passed to the builder.");
    println!("To use sources and reactions, you need to:");
    println!("  1. Create source/reaction instances implementing Source/Reaction traits");
    println!("  2. Pass them to DrasiLibBuilder using with_source() and with_reaction()");

    Ok(())
}
