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

use anyhow::Result;
use clap::Parser;
use log::{debug, info, warn};
use std::fs;
use std::path::PathBuf;

use drasi_server::api::mappings::{map_server_settings, DtoMapper};
use drasi_server::api::models::ConfigValue;
use drasi_server::{load_config_file, save_config_file, DrasiServer, DrasiServerConfig};

#[derive(Parser)]
#[command(name = "drasi-server")]
#[command(about = "Standalone Drasi server for data change processing")]
struct Cli {
    #[arg(short, long, default_value = "config/server.yaml")]
    config: PathBuf,

    #[arg(short, long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load .env file if it exists (for environment variable interpolation)
    // Look for .env in the same directory as the config file
    let env_file_loaded = if let Some(config_dir) = cli.config.parent() {
        let env_file = config_dir.join(".env");
        if env_file.exists() {
            match dotenvy::from_path(&env_file) {
                Ok(_) => true,
                Err(e) => {
                    eprintln!("Warning: Failed to load .env file: {}", e);
                    false
                }
            }
        } else {
            false
        }
    } else {
        false
    };

    // Check if config file exists, create default if it doesn't
    let config = if !cli.config.exists() {
        // Initialize basic logging first since we don't have a config yet
        if std::env::var("RUST_LOG").is_err() {
            // SAFETY: set_var is called early in main() before any other threads are spawned
            unsafe {
                std::env::set_var("RUST_LOG", "info");
            }
        }
        env_logger::init();

        warn!(
            "Config file '{}' not found. Creating default configuration.",
            cli.config.display()
        );

        // Create parent directories if they don't exist
        if let Some(parent) = cli.config.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create default config with command line port if specified
        let mut default_config = DrasiServerConfig::default();

        // Use CLI port if provided
        if let Some(port) = cli.port {
            default_config.server.port = ConfigValue::Static(port);
            info!("Using command line port {port} in default configuration");
        }

        save_config_file(&default_config, &cli.config)?;

        info!("Default configuration created at: {}", cli.config.display());
        info!("Please edit the configuration file to add sources, queries, and reactions.");

        default_config
    } else {
        // Load config first to get log level
        load_config_file(&cli.config)?
    };

    // Resolve server settings for use in main
    let mapper = DtoMapper::new();
    let resolved_settings = map_server_settings(&config.server, &mapper)?;

    // Set log level from config if RUST_LOG wasn't explicitly set by user
    if std::env::var("RUST_LOG").is_err() {
        // SAFETY: set_var is called early in main() before any other threads are spawned
        unsafe {
            std::env::set_var("RUST_LOG", &resolved_settings.log_level);
        }
        // Initialize logger with correct level
        env_logger::init();
    } else {
        // User explicitly set RUST_LOG, so just init with their setting
        env_logger::init();
    }

    info!("Starting Drasi Server");
    debug!("Debug logging is enabled");

    if env_file_loaded {
        info!("Loaded environment variables from .env file");
    }

    info!("Config file: {}", cli.config.display());

    let final_port = cli.port.unwrap_or(resolved_settings.port);
    info!("Port: {final_port}");
    debug!("Server configuration: {:?}", resolved_settings);

    let server = DrasiServer::new(cli.config, final_port).await?;
    server.run().await?;

    Ok(())
}
