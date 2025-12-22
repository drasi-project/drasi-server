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

    /// Initialize a new configuration file
    Init {
        /// Output path for the configuration file
        #[arg(short, long, default_value = "config/server.yaml")]
        output: PathBuf,

        /// Template to use: minimal, postgres, http, mock
        #[arg(short, long, default_value = "minimal")]
        template: String,

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
        Some(Commands::Init {
            output,
            template,
            force,
        }) => init_config(output, template, force),
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

/// Initialize a new configuration file
fn init_config(output_path: PathBuf, template: String, force: bool) -> Result<()> {
    // Check if file already exists
    if output_path.exists() && !force {
        println!(
            "Configuration file already exists: {}",
            output_path.display()
        );
        println!("Use --force to overwrite.");
        std::process::exit(1);
    }

    // Create parent directories
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Get template content
    let content = match template.as_str() {
        "minimal" => get_minimal_template(),
        "postgres" => get_postgres_template(),
        "http" => get_http_template(),
        "mock" => get_mock_template(),
        _ => {
            println!("Unknown template: {template}");
            println!("Available templates: minimal, postgres, http, mock");
            std::process::exit(1);
        }
    };

    // Write file
    fs::write(&output_path, content)?;

    println!("Created configuration file: {}", output_path.display());
    println!("Template: {template}");
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {} to customize your configuration",
        output_path.display()
    );
    println!("  2. Run: drasi-server --config {}", output_path.display());

    Ok(())
}

fn get_minimal_template() -> &'static str {
    r#"# Drasi Server Configuration
# Generated with: drasi-server init --template minimal

host: 0.0.0.0
port: 8080
log_level: info

id: drasi-server

# Add your sources here
sources: []

# Add your queries here
# queries:
#   - id: my-query
#     query: "MATCH (n) RETURN n"
#     query_language: Cypher
#     sources: [my-source]

# Add your reactions here
reactions: []
"#
}

fn get_postgres_template() -> &'static str {
    r#"# Drasi Server Configuration - PostgreSQL CDC
# Generated with: drasi-server init --template postgres
#
# This template demonstrates PostgreSQL Change Data Capture (CDC).
# Update the connection details below to match your database.

host: "${SERVER_HOST:-0.0.0.0}"
port: "${SERVER_PORT:-8080}"
log_level: "${LOG_LEVEL:-info}"

id: "${SERVER_ID:-drasi-postgres}"

sources:
  - kind: postgres
    id: my-database
    auto_start: true

    # Database connection - use environment variables for security
    host: "${DB_HOST:-localhost}"
    port: "${DB_PORT:-5432}"
    database: "${DB_NAME:-mydb}"
    user: "${DB_USER:-postgres}"
    password: "${DB_PASSWORD}"  # Set via environment variable

    # Tables to monitor for changes
    tables:
      - my_table

    # Replication settings
    slot_name: drasi_slot
    publication_name: drasi_pub

    bootstrap_provider:
      kind: postgres
      host: "${DB_HOST:-localhost}"
      port: "${DB_PORT:-5432}"
      database: "${DB_NAME:-mydb}"
      user: "${DB_USER:-postgres}"
      password: "${DB_PASSWORD}"

queries:
  - id: sample-query
    query: |
      MATCH (n:MyTable)
      RETURN n.id, n.name, n.updated_at
    query_language: Cypher
    auto_start: true
    enable_bootstrap: true
    source_subscriptions:
      - source_id: my-database

reactions:
  - kind: log
    id: log-changes
    queries:
      - sample-query
    auto_start: true
    log_level: info

# To run:
#   export DB_HOST=localhost DB_NAME=mydb DB_USER=postgres DB_PASSWORD=secret
#   drasi-server --config config/server.yaml
"#
}

fn get_http_template() -> &'static str {
    r#"# Drasi Server Configuration - HTTP Source
# Generated with: drasi-server init --template http
#
# This template demonstrates receiving events via HTTP endpoint.

host: "${SERVER_HOST:-0.0.0.0}"
port: "${SERVER_PORT:-8080}"
log_level: "${LOG_LEVEL:-info}"

id: "${SERVER_ID:-drasi-http}"

sources:
  - kind: http
    id: http-ingestion
    auto_start: true
    host: 0.0.0.0
    port: 9000

    # Optional: bootstrap from file
    # bootstrap_provider:
    #   kind: scriptfile
    #   file_paths:
    #     - data/initial-data.jsonl

queries:
  - id: filter-query
    query: |
      MATCH (n:Event)
      WHERE n.severity = 'high'
      RETURN n.id, n.message, n.timestamp
    query_language: Cypher
    auto_start: true
    source_subscriptions:
      - source_id: http-ingestion

reactions:
  # Log high-severity events
  - kind: log
    id: log-events
    queries:
      - filter-query
    auto_start: true
    log_level: warn

  # Stream events via SSE
  - kind: sse
    id: sse-events
    queries:
      - filter-query
    auto_start: true
    host: 0.0.0.0
    port: 8081

# To run:
#   drasi-server --config config/server.yaml
#
# Send events:
#   curl -X POST http://localhost:9000/events \
#     -H "Content-Type: application/json" \
#     -d '{"op": "i", "labels": ["Event"], "data": {"id": "1", "severity": "high", "message": "Alert!"}}'
"#
}

fn get_mock_template() -> &'static str {
    r#"# Drasi Server Configuration - Mock Source (Testing)
# Generated with: drasi-server init --template mock
#
# This template uses a mock source that generates test data.
# Useful for testing queries and reactions without external dependencies.

host: 0.0.0.0
port: 8080
log_level: info

id: drasi-mock

sources:
  - kind: mock
    id: test-source
    auto_start: true
    interval_seconds: 5  # Generate data every 5 seconds

queries:
  - id: all-data
    query: |
      MATCH (n)
      RETURN n
    query_language: Cypher
    auto_start: true
    source_subscriptions:
      - source_id: test-source

reactions:
  - kind: log
    id: log-all
    queries:
      - all-data
    auto_start: true
    log_level: info

# To run:
#   drasi-server --config config/server.yaml
#
# The mock source will generate test data every 5 seconds.
# View the output in the server logs.
"#
}
