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
use drasi_server::models::{QueryConfigDto, QueryInputConfigDto};

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
        inputs: vec![QueryInputConfigDto::Component(
            "vehicle-location-source".to_string(),
        )],
        enable_bootstrap: true,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
        outbox_capacity: 1000,
        bootstrap_timeout_secs: 300,
        result: Default::default(),
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
        inputs: vec![QueryInputConfigDto::Component(
            "vehicle-location-source".to_string(),
        )],
        enable_bootstrap: true,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
        outbox_capacity: 1000,
        bootstrap_timeout_secs: 300,
        result: Default::default(),
    };

    // Create the configuration structure
    // Note: Sources and reactions can be defined in the config file using the tagged enum format
    let config = drasi_server::DrasiServerConfig {
        api_version: None,

        host: drasi_server::models::ConfigValue::Static("0.0.0.0".to_string()),
        port: drasi_server::models::ConfigValue::Static(8080),
        log_level: drasi_server::models::ConfigValue::Static("info".to_string()),
        metrics: Default::default(),
        persist_config: true,
        // Use in-memory indexes (default)
        // Disable temporal history (default)
        // Use the RocksDB default when enabled
        enable_ui: true, // Enable web UI (default)
        // Use in-memory state store (default)
        // No secret store configured
        // Use lib defaults
        // Use lib defaults
        // Add sources using SourceConfig enum
        // Add reactions using ReactionConfig enum

        // No external plugin registry is configured.
        plugin_registry: None,
        auto_install_plugins: false,
        plugins: vec![],
        verify_plugins: false,
        trusted_identities: vec![],
        hot_reload_plugins: false,
        hot_reload_debounce_ms: 2000,
        solutions_dir: None, // Use default solutions directory
        cors_allowed_origins: vec![],

        instances: vec![drasi_server::config::DrasiLibInstanceConfig {
            id: drasi_server::models::ConfigValue::Static(uuid::Uuid::new_v4().to_string()).into(),
            persist_index: false,
            enable_archive: false,
            memory_budget_mib: None,
            state_store: None,
            secret_store: None,
            default_priority_queue_capacity: None,
            default_dispatch_buffer_capacity: None,
            activation_timeout_secs: None,
            sources: vec![],
            reactions: vec![],
            queries: vec![available_drivers_query, pending_orders_query],
            identity_providers: vec![],
            bootstrap_providers: vec![],
            computation: None,
            configuration_store: None,
            ..Default::default()
        }],
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
