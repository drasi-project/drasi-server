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

use drasi_lib::config::QueryLanguage;
use drasi_server::models::{QueryConfigDto, SourceSubscriptionConfigDto};

#[tokio::main]
#[allow(clippy::print_stdout)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating example Drasi configuration...");
    println!();

    // Build query configurations using QueryConfigDto
    let available_drivers_query = QueryConfigDto {
        id: "available-drivers-query".to_string(),
        auto_start: true,
        query: r#"
            MATCH (d:Driver {status: 'available'})
            WHERE d.latitude IS NOT NULL AND d.longitude IS NOT NULL
            RETURN elementId(d) AS driverId, d.driver_name AS driverName,
                   d.latitude AS lat, d.longitude AS lng, d.status AS status
        "#
        .to_string(),
        query_language: QueryLanguage::Cypher,
        middleware: vec![],
        sources: vec![SourceSubscriptionConfigDto {
            source_id: "vehicle-location-source".to_string(),
            nodes: vec![],
            relations: vec![],
            pipeline: vec![],
        }],
        enable_bootstrap: true,
        bootstrap_buffer_size: 10000,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
    };

    let pending_orders_query = QueryConfigDto {
        id: "pending-orders-query".to_string(),
        auto_start: true,
        query: r#"
            MATCH (o:Order)
            WHERE o.status IN ['pending', 'preparing', 'ready']
            RETURN elementId(o) AS orderId, o.status AS status,
                   o.restaurant AS restaurant, o.delivery_address AS address
        "#
        .to_string(),
        query_language: QueryLanguage::Cypher,
        middleware: vec![],
        sources: vec![SourceSubscriptionConfigDto {
            source_id: "vehicle-location-source".to_string(),
            nodes: vec![],
            relations: vec![],
            pipeline: vec![],
        }],
        enable_bootstrap: true,
        bootstrap_buffer_size: 10000,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
    };

    // Create the configuration structure
    // Note: Sources and reactions can be defined in the config file using the tagged enum format
    let config = drasi_server::DrasiServerConfig {
        api_version: None,
        id: drasi_server::models::ConfigValue::Static(uuid::Uuid::new_v4().to_string()),
        host: drasi_server::models::ConfigValue::Static("0.0.0.0".to_string()),
        port: drasi_server::models::ConfigValue::Static(8080),
        log_level: drasi_server::models::ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,                  // Use in-memory indexes (default)
        state_store: None,                     // Use in-memory state store (default)
        default_priority_queue_capacity: None, // Use lib defaults
        default_dispatch_buffer_capacity: None, // Use lib defaults
        sources: vec![],                       // Add sources using SourceConfig enum
        reactions: vec![],                     // Add reactions using ReactionConfig enum
        queries: vec![available_drivers_query, pending_orders_query],
        instances: vec![], // Empty = use legacy single-instance mode
    };

    // Save configuration to file
    std::fs::create_dir_all("config")?;
    config.save_to_file("config/server-docker.yaml")?;

    println!("Example configuration created successfully!");
    println!("Configuration saved to: config/server-docker.yaml");
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
