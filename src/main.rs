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

// Allow println! in main.rs for CLI user-facing output (validate, doctor, init commands)
#![allow(clippy::print_stdout)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use log::{debug, info, warn};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use drasi_server::api::mappings::{map_server_settings, DtoMapper};
use drasi_server::api::models::ConfigValue;
use drasi_server::{load_config_file, save_config_file, DrasiServer, DrasiServerConfig};

mod init;

#[derive(Parser)]
#[command(name = "drasi-server")]
#[command(about = "Standalone Drasi server for data change processing")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the configuration file
    #[arg(short, long, default_value = "config/server.yaml", global = true)]
    config: PathBuf,

    /// Override the server port
    #[arg(short, long, global = true)]
    port: Option<u16>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the server (default if no subcommand specified)
    Run {
        /// Path to the configuration file
        #[arg(short, long, default_value = "config/server.yaml")]
        config: PathBuf,

        /// Override the server port
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Validate a configuration file without starting the server
    Validate {
        /// Path to the configuration file to validate
        #[arg(short, long, default_value = "config/server.yaml")]
        config: PathBuf,

        /// Show resolved configuration with environment variables expanded
        #[arg(long)]
        show_resolved: bool,
    },

    /// Check system dependencies and requirements
    Doctor {
        /// Check for optional dependencies (Docker, etc.)
        #[arg(long)]
        all: bool,
    },

    /// Initialize a new configuration file interactively
    Init {
        /// Output path for the configuration file
        #[arg(short, long, default_value = "config/server.yaml")]
        output: PathBuf,

        /// Overwrite existing configuration file
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run { config, port }) => run_server(config, port).await,
        Some(Commands::Validate {
            config,
            show_resolved,
        }) => validate_config(config, show_resolved),
        Some(Commands::Doctor { all }) => run_doctor(all),
        Some(Commands::Init { output, force }) => init::run_init(output, force),
        None => {
            // Default behavior: run the server (backward compatible)
            run_server(cli.config, cli.port).await
        }
    }
}

/// Run the Drasi Server
async fn run_server(config_path: PathBuf, port_override: Option<u16>) -> Result<()> {
    // Load .env file if it exists (for environment variable interpolation)
    // Look for .env in the same directory as the config file
    let env_file_loaded = if let Some(config_dir) = config_path.parent() {
        let env_file = config_dir.join(".env");
        if env_file.exists() {
            match dotenvy::from_path(&env_file) {
                Ok(_) => true,
                Err(e) => {
                    eprintln!("Warning: Failed to load .env file: {e}");
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
    let (config, logger_initialized) = if !config_path.exists() {
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
            config_path.display()
        );

        // Create parent directories if they don't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create default config with command line port if specified
        let mut default_config = DrasiServerConfig::default();

        // Use CLI port if provided
        if let Some(port) = port_override {
            default_config.port = ConfigValue::Static(port);
            info!("Using command line port {port} in default configuration");
        }

        save_config_file(&default_config, &config_path)?;

        info!(
            "Default configuration created at: {}",
            config_path.display()
        );
        info!("Please edit the configuration file to add sources, queries, and reactions.");

        (default_config, true)
    } else {
        // Load config first to get log level
        (load_config_file(&config_path)?, false)
    };

    // Resolve server settings for use in main
    let mapper = DtoMapper::new();
    let resolved_settings = map_server_settings(&config, &mapper)?;

    // Initialize logger if not already done
    if !logger_initialized {
        // Set log level from config if RUST_LOG wasn't explicitly set by user
        if std::env::var("RUST_LOG").is_err() {
            // SAFETY: set_var is called early in main() before any other threads are spawned
            unsafe {
                std::env::set_var("RUST_LOG", &resolved_settings.log_level);
            }
        }
        env_logger::init();
    }

    info!("Starting Drasi Server");
    debug!("Debug logging is enabled");

    if env_file_loaded {
        info!("Loaded environment variables from .env file");
    }

    info!("Config file: {}", config_path.display());

    let final_port = port_override.unwrap_or(resolved_settings.port);
    info!("Port: {final_port}");
    debug!("Server configuration: {resolved_settings:?}");

    let server = DrasiServer::new(config_path, final_port).await?;
    server.run().await?;

    Ok(())
}

/// Validate a configuration file
fn validate_config(config_path: PathBuf, show_resolved: bool) -> Result<()> {
    println!("Validating configuration: {}", config_path.display());
    println!();

    // Check if file exists
    if !config_path.exists() {
        println!(
            "[ERROR] Configuration file not found: {}",
            config_path.display()
        );
        std::process::exit(1);
    }

    // Try to load and parse the config
    match load_config_file(&config_path) {
        Ok(config) => {
            println!("[OK] Configuration file is valid");
            println!();

            // Show summary
            println!("Summary:");
            println!("  Sources: {}", config.sources.len());
            println!("  Queries: {}", config.core_config.queries.len());
            println!("  Reactions: {}", config.reactions.len());

            if show_resolved {
                println!();
                println!("Resolved server settings:");
                let mapper = DtoMapper::new();
                match map_server_settings(&config, &mapper) {
                    Ok(resolved) => {
                        println!("  Host: {}", resolved.host);
                        println!("  Port: {}", resolved.port);
                        println!("  Log Level: {}", resolved.log_level);
                    }
                    Err(e) => {
                        println!("[WARN] Could not resolve server settings: {e}");
                        println!("       Some environment variables may not be set.");
                    }
                }
            }

            Ok(())
        }
        Err(e) => {
            println!("[ERROR] Configuration is invalid:");
            println!("  {e}");
            std::process::exit(1);
        }
    }
}

/// Check system dependencies
fn run_doctor(check_all: bool) -> Result<()> {
    println!("Drasi Server Dependency Check");
    println!("==============================");
    println!();

    let mut all_ok = true;

    // Required dependencies
    println!("Required:");

    // Rust
    if let Ok(output) = Command::new("rustc").arg("--version").output() {
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("  [OK] {}", version.trim());
        } else {
            println!("  [MISSING] Rust - https://rustup.rs");
            all_ok = false;
        }
    } else {
        println!("  [MISSING] Rust - https://rustup.rs");
        all_ok = false;
    }

    // Git
    if let Ok(output) = Command::new("git").arg("--version").output() {
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("  [OK] {}", version.trim());
        } else {
            println!("  [MISSING] Git");
            all_ok = false;
        }
    } else {
        println!("  [MISSING] Git");
        all_ok = false;
    }

    // Submodules
    if std::path::Path::new("drasi-core/lib").exists() {
        println!("  [OK] Git submodules initialized");
    } else {
        println!("  [MISSING] Submodules - run: git submodule update --init --recursive");
        all_ok = false;
    }

    if check_all {
        println!();
        println!("Optional (for examples and Docker deployment):");

        // Docker
        if let Ok(output) = Command::new("docker").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                println!("  [OK] {}", version.trim());
            } else {
                println!("  [SKIP] Docker - https://docs.docker.com/get-docker/");
            }
        } else {
            println!("  [SKIP] Docker - https://docs.docker.com/get-docker/");
        }

        // Docker Compose
        let compose_ok = Command::new("docker")
            .args(["compose", "version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            || Command::new("docker-compose")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

        if compose_ok {
            println!("  [OK] Docker Compose");
        } else {
            println!("  [SKIP] Docker Compose");
        }

        // curl
        if Command::new("curl")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            println!("  [OK] curl");
        } else {
            println!("  [SKIP] curl");
        }

        // psql
        if Command::new("psql")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            println!("  [OK] psql (PostgreSQL client)");
        } else {
            println!("  [SKIP] psql (PostgreSQL client)");
        }
    }

    println!();

    if all_ok {
        println!("All required dependencies are available.");
        Ok(())
    } else {
        println!("Some required dependencies are missing.");
        std::process::exit(1);
    }
}
