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

use drasi_lib::get_or_init_global_registry;
use drasi_server::api::mappings::{map_server_settings, DtoMapper};
use drasi_server::api::models::ConfigValue;
use drasi_server::{load_config_file, save_config_file, DrasiServer, DrasiServerConfig};

mod cli_styles;
mod init;
mod plugin;

#[derive(Parser)]
#[command(name = "drasi-server")]
#[command(about = "Standalone Drasi server for data change processing")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(long_version = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nrustc: ",
    env!("DRASI_RUSTC_VERSION"),
    "\nplugin-sdk: ",
    env!("DRASI_PLUGIN_SDK_VERSION"),
))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the configuration file
    #[arg(short, long, default_value = "config/server.yaml", global = true)]
    config: PathBuf,

    /// Override the server port
    #[arg(short, long, global = true)]
    port: Option<u16>,

    /// Directory to scan for plugin shared libraries (defaults to binary directory)
    #[arg(long, global = true)]
    plugins_dir: Option<PathBuf>,

    /// Disable cosign signature verification for plugins (verification is on by default)
    #[arg(long, global = true)]
    skip_verification: bool,

    /// Enable the web UI (overrides config file)
    #[arg(long, global = true, conflicts_with = "disable_ui")]
    enable_ui: bool,

    /// Disable the web UI (overrides config file)
    #[arg(long, global = true, conflicts_with = "enable_ui")]
    disable_ui: bool,
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

        /// Directory to scan for plugin shared libraries (defaults to binary directory)
        #[arg(long)]
        plugins_dir: Option<PathBuf>,

        /// Disable cosign signature verification for plugins (verification is on by default)
        #[arg(long)]
        skip_verification: bool,
    },

    /// Validate a configuration file without starting the server
    Validate {
        /// Path to the configuration file to validate
        #[arg(short, long, default_value = "config/server.yaml")]
        config: PathBuf,

        /// Show resolved configuration with environment variables expanded
        #[arg(long)]
        show_resolved: bool,

        /// Directory to scan for plugin shared libraries
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
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

    /// Manage plugins from OCI registries
    Plugin {
        #[command(subcommand)]
        action: plugin::PluginAction,
    },

    /// Run as a stdio-based MCP (Model Context Protocol) server
    ///
    /// Speaks JSON-RPC over stdin/stdout. The Drasi runtime and web UI are
    /// booted on demand when the `open_admin_ui` tool is called with a config
    /// path; that tool renders the admin UI as an MCP app.
    Mcp {
        /// Default config file used when a tool does not specify one
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Port for the local HTTP API/UI (0 = OS-assigned ephemeral port)
        #[arg(short, long, default_value_t = 0)]
        port: u16,

        /// Directory to scan for plugin shared libraries (defaults to binary directory)
        #[arg(long)]
        plugins_dir: Option<PathBuf>,

        /// Disable cosign signature verification for plugins (verification is on by default)
        #[arg(long)]
        skip_verification: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run {
            config,
            port,
            plugins_dir,
            skip_verification,
        }) => {
            let ui_override = if cli.enable_ui {
                Some(true)
            } else if cli.disable_ui {
                Some(false)
            } else {
                None
            };
            run_server(config, port, plugins_dir, skip_verification, ui_override).await
        }
        Some(Commands::Validate {
            config,
            show_resolved,
            plugins_dir,
        }) => {
            let effective_plugins_dir = plugins_dir.or(cli.plugins_dir);
            validate_config(config, show_resolved, effective_plugins_dir)
        }
        Some(Commands::Doctor { all }) => run_doctor(all),
        Some(Commands::Init { output, force }) => {
            init::run_init(output, force, cli.plugins_dir).await
        }
        Some(Commands::Plugin { action }) => {
            plugin::run_plugin_command(action, cli.config, cli.plugins_dir).await
        }
        Some(Commands::Mcp {
            config,
            port,
            plugins_dir,
            skip_verification,
        }) => {
            run_mcp(
                config,
                port,
                plugins_dir.or(cli.plugins_dir),
                skip_verification,
            )
            .await
        }
        None => {
            // Default behavior: run the server (backward compatible)
            let ui_override = if cli.enable_ui {
                Some(true)
            } else if cli.disable_ui {
                Some(false)
            } else {
                None
            };
            run_server(
                cli.config,
                cli.port,
                cli.plugins_dir,
                cli.skip_verification,
                ui_override,
            )
            .await
        }
    }
}

/// Run the Drasi Server
async fn run_server(
    config_path: PathBuf,
    port_override: Option<u16>,
    plugins_dir: Option<PathBuf>,
    skip_verification: bool,
    ui_override: Option<bool>,
) -> Result<()> {
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
    let (config, tracing_initialized) = if !config_path.exists() {
        // Initialize tracing first since we don't have a config yet
        if std::env::var("RUST_LOG").is_err() {
            // SAFETY: set_var is called early in main() before any other threads are spawned
            unsafe {
                std::env::set_var("RUST_LOG", "info,oci_client=error");
            }
        }
        get_or_init_global_registry();

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

    // Initialize tracing if not already done
    if !tracing_initialized {
        // Set log level from config if RUST_LOG wasn't explicitly set by user
        if std::env::var("RUST_LOG").is_err() {
            // SAFETY: set_var is called early in main() before any other threads are spawned
            unsafe {
                std::env::set_var(
                    "RUST_LOG",
                    format!("{},oci_client=error", &resolved_settings.log_level),
                );
            }
        }
        get_or_init_global_registry();
    }

    info!("Starting Drasi Server");
    debug!("Debug logging is enabled");

    if env_file_loaded {
        info!("Loaded environment variables from .env file");
    }

    info!("Config file: {}", config_path.display());

    let final_port = port_override.unwrap_or(resolved_settings.port);
    let final_enable_ui = ui_override.unwrap_or(resolved_settings.enable_ui);
    info!("Port: {final_port}");
    info!(
        "Web UI: {}",
        if final_enable_ui {
            "enabled"
        } else {
            "disabled"
        }
    );
    debug!("Server configuration: {resolved_settings:?}");

    // Resolve the plugins directory: use CLI arg if provided, otherwise default to ./plugins under binary directory
    let plugins_dir = match plugins_dir {
        Some(dir) => dir,
        None => std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("plugins")))
            .unwrap_or_else(|| {
                warn!("Could not determine binary directory for plugin loading");
                PathBuf::from("plugins")
            }),
    };
    info!("Plugins directory: {}", plugins_dir.display());

    let server = DrasiServer::new(
        config_path,
        final_port,
        plugins_dir,
        skip_verification,
        final_enable_ui,
    )
    .await?;
    server.run().await?;

    Ok(())
}

/// Run the server in stdio MCP mode.
async fn run_mcp(
    config: Option<PathBuf>,
    port: u16,
    plugins_dir: Option<PathBuf>,
    skip_verification: bool,
) -> Result<()> {
    // Resolve the plugins directory the same way run_server does: use the CLI
    // arg if provided, otherwise default to ./plugins under the binary directory.
    let plugins_dir = match plugins_dir {
        Some(dir) => dir,
        None => std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("plugins")))
            .unwrap_or_else(|| PathBuf::from("plugins")),
    };

    let options = drasi_server::mcp::McpServerOptions {
        config,
        port,
        plugins_dir,
        skip_verification,
    };

    drasi_server::mcp::run_mcp_server(options).await
}

/// Validate a configuration file
fn validate_config(
    config_path: PathBuf,
    show_resolved: bool,
    plugins_dir: Option<PathBuf>,
) -> Result<()> {
    use drasi_server::config::{validate_with_plugins, FullValidationResult};

    println!("Validating: {}", config_path.display());
    println!();

    // Check if file exists
    if !config_path.exists() {
        println!(
            "[ERROR] Configuration file not found: {}",
            config_path.display()
        );
        std::process::exit(1);
    }

    // Load .env file if it exists (same as run_server)
    if let Some(config_dir) = config_path.parent() {
        let env_file = config_dir.join(".env");
        if env_file.exists() {
            let _ = dotenvy::from_path(&env_file);
        }
    }

    // Try to load and parse the config
    let config = match load_config_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            println!("Structure:");
            println!("  [ERR] {e}");
            std::process::exit(1);
        }
    };

    // Phase 1: structure & server settings
    println!("Structure:");
    println!("  [OK] YAML syntax valid");

    let mapper = DtoMapper::new();
    match map_server_settings(&config, &mapper) {
        Ok(resolved) => {
            println!(
                "  [OK] Server settings valid (host: {}, port: {}, logLevel: {})",
                resolved.host, resolved.port, resolved.log_level
            );
        }
        Err(e) => {
            println!("  [WARN] Could not resolve server settings: {e}");
        }
    }
    println!();

    // Resolve plugins_dir: provided > default (binary dir + /plugins)
    let effective_plugins_dir = plugins_dir.or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("plugins")))
    });

    // Phase 2-5: plugin-aware validation
    let result: FullValidationResult =
        validate_with_plugins(&config, effective_plugins_dir.as_deref());

    // Environment references
    println!("Environment references:");
    if result.env_warnings.is_empty() {
        println!("  [OK] No missing env var references");
    } else {
        for w in &result.env_warnings {
            println!("  [ERR] {}: {}", w.path, w.message);
        }
    }
    println!();

    // Plugins
    println!(
        "Plugins ({} loaded{}):",
        result.plugins_loaded,
        if let Some(dir) = &effective_plugins_dir {
            format!(" from {}", dir.display())
        } else {
            String::new()
        }
    );
    if result.plugins_not_loaded {
        println!("  [WARN] No plugins found — skipping plugin config validation.");
        println!("         Install plugins or use --plugins-dir to specify plugin location.");
    }
    for mp in &result.missing_plugins {
        println!(
            "  [WARN] {}/{} — not installed (referenced by {})",
            mp.requirement.category, mp.requirement.kind, mp.requirement.referenced_by
        );
    }
    println!();

    // Config validation
    println!("Config validation:");
    let instances = config.resolved_instances(&mapper).unwrap_or_default();
    let total_sources: usize = instances.iter().map(|i| i.sources.len()).sum();
    let total_queries: usize = instances.iter().map(|i| i.queries.len()).sum();
    let total_reactions: usize = instances.iter().map(|i| i.reactions.len()).sum();

    if result.config_errors.is_empty() {
        // Report OK for each validated component
        let all_sources = collect_all_sources_for_display(&config);
        for (id, kind) in &all_sources {
            println!("  [OK] source '{id}' ({kind})");
        }
        let all_reactions = collect_all_reactions_for_display(&config);
        for (id, kind) in &all_reactions {
            println!("  [OK] reaction '{id}' ({kind})");
        }
        if all_sources.is_empty() && all_reactions.is_empty() {
            println!("  (no components to validate)");
        }
    } else {
        for report in &result.config_errors {
            println!(
                "  [ERR] {} '{}' ({}):",
                report.component_type, report.component_id, report.plugin_kind
            );
            for err in &report.errors {
                println!("        - {}: {}", err.field, err.message);
            }
        }
    }
    println!();

    // Summary
    let error_count = result.env_warnings.len() + result.config_errors.len();
    let warning_count = result.missing_plugins.len();
    let instance_count = instances.len();

    print!(
        "Summary: {instance_count} instance(s), {total_sources} source(s), {total_queries} query/queries, {total_reactions} reaction(s)",
    );
    if error_count > 0 || warning_count > 0 {
        print!(" — ");
        let mut parts = Vec::new();
        if error_count > 0 {
            parts.push(format!("{error_count} error(s)"));
        }
        if warning_count > 0 {
            parts.push(format!("{warning_count} warning(s)"));
        }
        print!("{}", parts.join(", "));
    } else {
        print!(" — all valid");
    }
    println!();

    if show_resolved {
        println!();
        println!("Resolved server settings:");
        match map_server_settings(&config, &mapper) {
            Ok(resolved) => {
                println!("  Host: {}", resolved.host);
                println!("  Port: {}", resolved.port);
                println!("  Log Level: {}", resolved.log_level);
            }
            Err(e) => {
                println!("  [WARN] Could not resolve: {e}");
            }
        }
    }

    if error_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Collect (id, kind) pairs for all sources in the config.
fn collect_all_sources_for_display(config: &DrasiServerConfig) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for src in &config.sources {
        out.push((src.id.clone(), src.kind.clone()));
    }
    for inst in &config.instances {
        for src in &inst.sources {
            out.push((src.id.clone(), src.kind.clone()));
        }
    }
    out
}

/// Collect (id, kind) pairs for all reactions in the config.
fn collect_all_reactions_for_display(config: &DrasiServerConfig) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for rxn in &config.reactions {
        out.push((rxn.id.clone(), rxn.kind.clone()));
    }
    for inst in &config.instances {
        for rxn in &inst.reactions {
            out.push((rxn.id.clone(), rxn.kind.clone()));
        }
    }
    out
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
