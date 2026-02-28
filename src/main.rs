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

mod init;

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

    /// Manage plugins from OCI registries
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
}

#[derive(Subcommand)]
enum PluginAction {
    /// Install a plugin from an OCI registry, local file, or HTTP URL
    Install {
        /// Plugin reference: OCI (e.g., "source/postgres:0.1.8"),
        /// file (e.g., "file:///path/to/plugin.so"),
        /// or HTTP (e.g., "https://example.com/plugin.so")
        #[arg(required_unless_present = "from_config")]
        reference: Option<String>,

        /// Install all plugins declared in the config file
        #[arg(long)]
        from_config: bool,

        /// Override OCI registry (default: from config or ghcr.io/drasi-project)
        #[arg(long)]
        registry: Option<String>,

        /// Override target platform (e.g., "linux/amd64")
        #[arg(long)]
        platform: Option<String>,

        /// Use exact versions from plugins.lock (fail if lockfile is missing or outdated)
        #[arg(long)]
        locked: bool,
    },

    /// List installed plugins
    List,

    /// Search for available versions of a plugin in the registry
    Search {
        /// Plugin name or reference (e.g., "postgres", "source/postgres",
        /// "ghcr.io/acme-corp/custom-source")
        reference: String,

        /// Override OCI registry
        #[arg(long)]
        registry: Option<String>,
    },

    /// Remove an installed plugin
    Remove {
        /// Plugin filename or kind (e.g., "libdrasi_source_postgres.so" or "source/postgres")
        reference: String,
    },

    /// Install all available plugins from the registry's plugin directory
    InstallAll {
        /// Override OCI registry (default: from config or ghcr.io/drasi-project)
        #[arg(long)]
        registry: Option<String>,
    },

    /// Upgrade installed plugins to newer compatible versions
    Upgrade {
        /// Plugin reference to upgrade (e.g., "source/postgres", "source/postgres:0.2.0").
        /// If omitted, use --all to upgrade everything.
        reference: Option<String>,

        /// Upgrade all installed plugins to their latest compatible versions
        #[arg(long)]
        all: bool,

        /// Override OCI registry
        #[arg(long)]
        registry: Option<String>,

        /// Show what would change without actually upgrading
        #[arg(long)]
        dry_run: bool,
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
        }) => run_server(config, port, plugins_dir).await,
        Some(Commands::Validate {
            config,
            show_resolved,
        }) => validate_config(config, show_resolved),
        Some(Commands::Doctor { all }) => run_doctor(all),
        Some(Commands::Init { output, force }) => init::run_init(output, force),
        Some(Commands::Plugin { action }) => run_plugin_command(action, cli.config, cli.plugins_dir).await,
        None => {
            // Default behavior: run the server (backward compatible)
            run_server(cli.config, cli.port, cli.plugins_dir).await
        }
    }
}

/// Run the Drasi Server
async fn run_server(
    config_path: PathBuf,
    port_override: Option<u16>,
    plugins_dir: Option<PathBuf>,
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
                std::env::set_var("RUST_LOG", "info");
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
                std::env::set_var("RUST_LOG", &resolved_settings.log_level);
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
    info!("Port: {final_port}");
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

    let server = DrasiServer::new(config_path, final_port, plugins_dir).await?;
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
            let mapper = DtoMapper::new();
            let instances = config.resolved_instances(&mapper).unwrap_or_default();
            let total_sources: usize = instances.iter().map(|i| i.sources.len()).sum();
            let total_queries: usize = instances.iter().map(|i| i.queries.len()).sum();
            let total_reactions: usize = instances.iter().map(|i| i.reactions.len()).sum();

            let instance_count = instances.len();
            println!("  Instances: {instance_count}");
            println!("  Sources: {total_sources}");
            println!("  Queries: {total_queries}");
            println!("  Reactions: {total_reactions}");

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

/// Handle plugin subcommands
async fn run_plugin_command(
    action: PluginAction,
    config_path: PathBuf,
    plugins_dir_override: Option<PathBuf>,
) -> Result<()> {
    let plugins_dir = match plugins_dir_override {
        Some(dir) => dir,
        None => std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("plugins")))
            .unwrap_or_else(|| PathBuf::from("plugins")),
    };

    match action {
        PluginAction::Install {
            reference,
            from_config,
            registry,
            platform: _platform,
            locked,
        } => {
            if from_config {
                plugin_install_from_config(&config_path, &plugins_dir, registry.as_deref(), locked)
                    .await
            } else if let Some(ref_str) = reference {
                plugin_install_single(&ref_str, &plugins_dir, &config_path, registry.as_deref())
                    .await
            } else {
                eprintln!("Error: provide a plugin reference or --from-config");
                std::process::exit(1);
            }
        }
        PluginAction::List => plugin_list(&plugins_dir),
        PluginAction::Search {
            reference,
            registry,
        } => plugin_search(&reference, &config_path, registry.as_deref()).await,
        PluginAction::Remove { reference } => plugin_remove(&reference, &plugins_dir),
        PluginAction::InstallAll { registry } => {
            plugin_install_all(&plugins_dir, &config_path, registry.as_deref()).await
        }
        PluginAction::Upgrade {
            reference,
            all,
            registry,
            dry_run,
        } => {
            plugin_upgrade(&plugins_dir, &config_path, reference.as_deref(), all, registry.as_deref(), dry_run).await
        }
    }
}

/// Install a single plugin from the registry.
#[cfg(feature = "dynamic-plugins")]
async fn plugin_install_single(
    reference: &str,
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    use drasi_host_sdk::fetcher::{parse_source_type, PluginSourceType};
    use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile};

    match parse_source_type(reference) {
        PluginSourceType::File | PluginSourceType::Http => {
            plugin_install_from_uri(reference, plugins_dir).await
        }
        PluginSourceType::Oci => {
            plugin_install_from_oci(reference, plugins_dir, config_path, registry_override).await
        }
    }
}

/// Install a plugin from a file:// or http(s):// URI.
#[cfg(feature = "dynamic-plugins")]
async fn plugin_install_from_uri(
    reference: &str,
    plugins_dir: &std::path::Path,
) -> Result<()> {
    use drasi_host_sdk::fetcher::{fetch_from_file, fetch_from_http, parse_source_type, read_plugin_metadata, PluginSourceType};
    use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile};

    let source_type = parse_source_type(reference);
    println!("Fetching plugin from {}...", reference);

    let fetched = match source_type {
        PluginSourceType::File => fetch_from_file(reference, plugins_dir).await?,
        PluginSourceType::Http => fetch_from_http(reference, plugins_dir).await?,
        PluginSourceType::Oci => unreachable!(),
    };

    println!("  → {}", fetched.path.display());

    // Read metadata from the binary to populate the lockfile
    let metadata = read_plugin_metadata(&fetched.path).unwrap_or_default();

    let mut lockfile = PluginLockfile::read(plugins_dir)?.unwrap_or_default();
    lockfile.insert(
        reference.to_string(),
        LockedPlugin {
            reference: reference.to_string(),
            version: metadata.plugin_version.clone(),
            digest: String::new(),
            sdk_version: metadata.sdk_version.clone(),
            core_version: metadata.core_version.clone(),
            lib_version: metadata.lib_version.clone(),
            platform: metadata.target_triple.clone(),
            filename: fetched.filename,
        },
    );
    lockfile.write(plugins_dir)?;

    if !metadata.plugin_version.is_empty() {
        println!("  Plugin version: {}", metadata.plugin_version);
    }
    println!("Done.");
    Ok(())
}

/// Install a plugin from an OCI registry.
#[cfg(feature = "dynamic-plugins")]
async fn plugin_install_from_oci(
    reference: &str,
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    use drasi_host_sdk::registry::{
        HostVersionInfo, OciRegistryClient, PluginResolver, RegistryConfig,
    };
    use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile};

    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };

    let client = OciRegistryClient::new(config);
    let host_info = cli_host_version_info();
    let resolver = PluginResolver::new(&client, &host_info);

    println!("Resolving {} from {}...", reference, registry_url);
    println!(
        "  Server versions: SDK {}, core {}, lib {}",
        host_info.sdk_version, host_info.core_version, host_info.lib_version
    );

    let resolved = resolver.resolve(reference, &registry_url).await?;

    println!(
        "Installing {}:{} ({}, {})",
        reference, resolved.version, resolved.platform, resolved.filename
    );

    std::fs::create_dir_all(plugins_dir)?;
    client
        .download_plugin(&resolved.reference, plugins_dir, &resolved.filename)
        .await?;

    println!("  → {}", plugins_dir.join(&resolved.filename).display());

    // Update lockfile
    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();
    lockfile.insert(
        reference.to_string(),
        LockedPlugin {
            reference: resolved.reference,
            version: resolved.version,
            digest: resolved.digest,
            sdk_version: resolved.sdk_version,
            core_version: resolved.core_version,
            lib_version: resolved.lib_version,
            platform: resolved.platform,
            filename: resolved.filename,
        },
    );
    lockfile.write(lockfile_dir)?;

    println!("Done.");
    Ok(())
}

#[cfg(not(feature = "dynamic-plugins"))]
async fn plugin_install_single(
    _reference: &str,
    _plugins_dir: &std::path::Path,
    _config_path: &std::path::Path,
    _registry_override: Option<&str>,
) -> Result<()> {
    eprintln!("Plugin management requires the 'dynamic-plugins' feature.");
    eprintln!("Rebuild with: cargo build --no-default-features --features dynamic-plugins");
    std::process::exit(1);
}

/// Install all plugins from the config file.
#[cfg(feature = "dynamic-plugins")]
async fn plugin_install_from_config(
    config_path: &std::path::Path,
    plugins_dir: &std::path::Path,
    registry_override: Option<&str>,
    locked: bool,
) -> Result<()> {
    use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile};

    let config = load_config_file(config_path)?;

    if config.plugins.is_empty() {
        println!("No plugins declared in config file.");
        return Ok(());
    }

    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();

    if locked && lockfile.plugins.is_empty() {
        eprintln!("Error: --locked flag used but no plugins.lock file found");
        std::process::exit(1);
    }

    if locked {
        println!(
            "Installing {} plugin(s) from lockfile...",
            config.plugins.len()
        );

        // In locked mode, use lockfile entries to download
        for dep in &config.plugins {
            let locked_entry = match lockfile.get(&dep.reference) {
                Some(entry) => entry.clone(),
                None => {
                    eprintln!(
                        "  ✗ {} — not found in plugins.lock (required by --locked)",
                        dep.reference
                    );
                    continue;
                }
            };

            let dest_path = plugins_dir.join(&locked_entry.filename);
            if dest_path.exists() {
                println!(
                    "  ✓ {} v{} — already installed",
                    dep.reference, locked_entry.version
                );
                continue;
            }

            // Download using locked reference
            let registry_url = registry_override
                .map(|s| s.to_string())
                .or_else(|| config.plugin_registry.clone())
                .unwrap_or_else(|| "ghcr.io/drasi-project".to_string());

            let auth = get_cli_registry_auth();
            let reg_config = drasi_host_sdk::registry::RegistryConfig {
                default_registry: registry_url,
                auth,
            };
            let client = drasi_host_sdk::registry::OciRegistryClient::new(reg_config);

            println!(
                "  ↓ {} v{} — downloading (locked)...",
                dep.reference, locked_entry.version
            );

            std::fs::create_dir_all(plugins_dir)?;
            match client
                .download_plugin(
                    &locked_entry.reference,
                    plugins_dir,
                    &locked_entry.filename,
                )
                .await
            {
                Ok(_path) => {
                    println!(
                        "  ✓ {} v{} — installed → {}",
                        dep.reference, locked_entry.version, locked_entry.filename
                    );
                }
                Err(e) => {
                    eprintln!("  ✗ {} — {}", dep.reference, e);
                }
            }
        }
    } else {
        let registry_url = registry_override
            .map(|s| s.to_string())
            .or_else(|| config.plugin_registry.clone())
            .unwrap_or_else(|| "ghcr.io/drasi-project".to_string());

        println!(
            "Installing {} plugin(s) from config...",
            config.plugins.len()
        );

        let auth = get_cli_registry_auth();
        let reg_config = drasi_host_sdk::registry::RegistryConfig {
            default_registry: registry_url.clone(),
            auth,
        };
        let client = drasi_host_sdk::registry::OciRegistryClient::new(reg_config);
        let host_info = cli_host_version_info();
        let resolver = drasi_host_sdk::registry::PluginResolver::new(&client, &host_info);

        for dep in &config.plugins {
            // Check if this is a file/HTTP URI or an OCI reference
            let source_type = drasi_host_sdk::fetcher::parse_source_type(&dep.reference);
            match source_type {
                drasi_host_sdk::fetcher::PluginSourceType::File
                | drasi_host_sdk::fetcher::PluginSourceType::Http => {
                    // Use the fetcher for file/HTTP URIs
                    println!("  ↓ {} — fetching...", dep.reference);
                    match plugin_install_from_uri(&dep.reference, plugins_dir).await {
                        Ok(()) => {}
                        Err(e) => {
                            eprintln!("  ✗ {} — {}", dep.reference, e);
                        }
                    }
                }
                drasi_host_sdk::fetcher::PluginSourceType::Oci => {
            match resolver.resolve(&dep.reference, &registry_url).await {
                Ok(resolved) => {
                    let dest_path = plugins_dir.join(&resolved.filename);
                    if dest_path.exists() {
                        println!(
                            "  ✓ {} v{} — already installed",
                            dep.reference, resolved.version
                        );
                    } else {
                        println!(
                            "  ↓ {} v{} — downloading...",
                            dep.reference, resolved.version
                        );
                        std::fs::create_dir_all(plugins_dir)?;
                        match client
                            .download_plugin(
                                &resolved.reference,
                                plugins_dir,
                                &resolved.filename,
                            )
                            .await
                        {
                            Ok(_path) => {
                                println!(
                                    "  ✓ {} v{} — installed → {}",
                                    dep.reference, resolved.version, resolved.filename
                                );
                            }
                            Err(e) => {
                                eprintln!("  ✗ {} — {}", dep.reference, e);
                                continue;
                            }
                        }
                    }

                    // Update lockfile entry
                    lockfile.insert(
                        dep.reference.clone(),
                        LockedPlugin {
                            reference: resolved.reference,
                            version: resolved.version,
                            digest: resolved.digest,
                            sdk_version: resolved.sdk_version,
                            core_version: resolved.core_version,
                            lib_version: resolved.lib_version,
                            platform: resolved.platform,
                            filename: resolved.filename,
                        },
                    );
                }
                Err(e) => {
                    eprintln!("  ✗ {} — {}", dep.reference, e);
                }
            }
                }
            }
        }

        // Write updated lockfile
        lockfile.write(lockfile_dir)?;
    }

    Ok(())
}

#[cfg(not(feature = "dynamic-plugins"))]
async fn plugin_install_from_config(
    _config_path: &std::path::Path,
    _plugins_dir: &std::path::Path,
    _registry_override: Option<&str>,
    _locked: bool,
) -> Result<()> {
    eprintln!("Plugin management requires the 'dynamic-plugins' feature.");
    std::process::exit(1);
}

/// Install all plugins from the registry's plugin directory.
#[cfg(feature = "dynamic-plugins")]
async fn plugin_install_all(
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    use drasi_host_sdk::registry::{
        OciRegistryClient, PluginResolver, RegistryConfig,
    };
    use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile};

    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };

    let client = OciRegistryClient::new(config);
    let host_info = cli_host_version_info();
    let resolver = PluginResolver::new(&client, &host_info);

    println!("Discovering plugins from {}...", registry_url);

    let results = client.search_plugins("*").await?;

    if results.is_empty() {
        println!("No plugins found in the directory.");
        return Ok(());
    }

    println!(
        "Found {} plugins, resolving compatible versions...",
        results.len()
    );

    std::fs::create_dir_all(plugins_dir)?;
    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut fail_count = 0;

    for result in &results {
        let reference = &result.reference;
        match resolver.resolve(reference, &registry_url).await {
            Ok(resolved) => {
                let dest_path = plugins_dir.join(&resolved.filename);
                if dest_path.exists() {
                    println!(
                        "  ✓ {} v{} — already installed",
                        reference, resolved.version
                    );
                    skip_count += 1;
                } else {
                    println!(
                        "  ↓ {} v{} — downloading...",
                        reference, resolved.version
                    );
                    match client
                        .download_plugin(
                            &resolved.reference,
                            plugins_dir,
                            &resolved.filename,
                        )
                        .await
                    {
                        Ok(_) => {
                            println!(
                                "  ✓ {} v{} — installed → {}",
                                reference, resolved.version, resolved.filename
                            );
                            success_count += 1;
                        }
                        Err(e) => {
                            eprintln!("  ✗ {} — {}", reference, e);
                            fail_count += 1;
                            continue;
                        }
                    }
                }

                lockfile.insert(
                    reference.clone(),
                    LockedPlugin {
                        reference: resolved.reference,
                        version: resolved.version,
                        digest: resolved.digest,
                        sdk_version: resolved.sdk_version,
                        core_version: resolved.core_version,
                        lib_version: resolved.lib_version,
                        platform: resolved.platform,
                        filename: resolved.filename,
                    },
                );
            }
            Err(e) => {
                eprintln!("  ✗ {} — {}", reference, e);
                fail_count += 1;
            }
        }
    }

    lockfile.write(lockfile_dir)?;

    println!(
        "\nDone: {} installed, {} already present, {} failed",
        success_count, skip_count, fail_count
    );

    if fail_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(not(feature = "dynamic-plugins"))]
async fn plugin_install_all(
    _plugins_dir: &std::path::Path,
    _config_path: &std::path::Path,
    _registry_override: Option<&str>,
) -> Result<()> {
    eprintln!("Plugin management requires the 'dynamic-plugins' feature.");
    std::process::exit(1);
}

/// List installed plugins in the plugins directory.
fn plugin_list(plugins_dir: &std::path::Path) -> Result<()> {
    use drasi_server::plugin_lockfile::PluginLockfile;

    if !plugins_dir.exists() {
        println!("No plugins directory found: {}", plugins_dir.display());
        return Ok(());
    }

    let entries = fs::read_dir(plugins_dir)?;
    let mut plugins = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".so") || name.ends_with(".dll") || name.ends_with(".dylib") {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            plugins.push((name, size));
        }
    }

    if plugins.is_empty() {
        println!("No plugins installed in {}", plugins_dir.display());
        return Ok(());
    }

    // Load lockfile for metadata
    let lockfile_dir = plugins_dir;
    let lockfile = PluginLockfile::read(lockfile_dir)
        .ok()
        .flatten()
        .unwrap_or_default();

    // Build filename → (key, entry) lookup
    let mut by_filename: std::collections::HashMap<&str, (&str, &drasi_server::plugin_lockfile::LockedPlugin)> =
        std::collections::HashMap::new();
    for (key, entry) in &lockfile.plugins {
        by_filename.insert(&entry.filename, (key, entry));
    }

    plugins.sort_by(|a, b| a.0.cmp(&b.0));
    println!("Installed plugins ({}):", plugins.len());
    println!("  Directory: {}", plugins_dir.display());
    println!();
    for (name, size) in &plugins {
        let size_mb = *size as f64 / 1_048_576.0;

        if let Some((key, entry)) = by_filename.get(name.as_str()) {
            println!("  {} v{}", key, entry.version);
            println!("    File: {} ({:.1} MB)", name, size_mb);
            println!("    SDK: {}  Platform: {}", entry.sdk_version, entry.platform);
        } else {
            println!("  {} ({:.1} MB)", name, size_mb);
        }
    }

    Ok(())
}

/// Search for available versions of a plugin.
#[cfg(feature = "dynamic-plugins")]
async fn plugin_search(
    reference: &str,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    use drasi_host_sdk::registry::OciRegistryClient;
    use drasi_host_sdk::registry::RegistryConfig;

    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };

    let client = OciRegistryClient::new(config);

    println!("Searching for {} in {}...", reference, registry_url);

    let results = client.search_plugins(reference).await?;

    if results.is_empty() {
        println!("No plugins found matching '{}'.", reference);
        return Ok(());
    }

    for result in &results {
        println!("\n  {} ({})", result.reference, result.full_reference);
        if result.versions.is_empty() {
            println!("    No versions found.");
        } else {
            println!("    Available versions:");
            for v in &result.versions {
                println!("      {}  ({})", v.version, v.platforms.join(", "));
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "dynamic-plugins"))]
async fn plugin_search(
    _reference: &str,
    _config_path: &std::path::Path,
    _registry_override: Option<&str>,
) -> Result<()> {
    eprintln!("Plugin management requires the 'dynamic-plugins' feature.");
    std::process::exit(1);
}

/// Remove an installed plugin.
fn plugin_remove(reference: &str, plugins_dir: &std::path::Path) -> Result<()> {
    use drasi_server::plugin_lockfile::PluginLockfile;

    if !plugins_dir.exists() {
        eprintln!("Plugins directory does not exist: {}", plugins_dir.display());
        std::process::exit(1);
    }

    let mut removed = false;

    // Try exact filename first
    let target = plugins_dir.join(reference);
    if target.exists() {
        fs::remove_file(&target)?;
        println!("Removed {}", reference);
        removed = true;
    }

    // Try matching by type/kind pattern (e.g., "source/postgres")
    if !removed {
        if let Some((ptype, kind)) = reference.split_once('/') {
            let base = format!("drasi_{}_{}", ptype, kind.replace('-', "_"));
            let patterns = [
                format!("lib{base}.so"),
                format!("{base}.dll"),
                format!("lib{base}.dylib"),
            ];

            for pattern in &patterns {
                let path = plugins_dir.join(pattern);
                if path.exists() {
                    fs::remove_file(&path)?;
                    println!("Removed {}", pattern);
                    removed = true;
                    break;
                }
            }
        }
    }

    if !removed {
        eprintln!("Plugin not found: {}", reference);
        std::process::exit(1);
    }

    // Update lockfile: remove the entry
    let lockfile_dir = plugins_dir;
    if let Ok(Some(mut lockfile)) = PluginLockfile::read(lockfile_dir) {
        if lockfile.remove(reference).is_some() {
            let _ = lockfile.write(lockfile_dir);
            println!("Updated plugins.lock");
        }
    }

    Ok(())
}

/// Upgrade installed plugins to newer compatible versions.
#[cfg(feature = "dynamic-plugins")]
async fn plugin_upgrade(
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    reference: Option<&str>,
    all: bool,
    registry_override: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    use drasi_host_sdk::registry::{
        OciRegistryClient, PluginResolver, RegistryConfig,
    };
    use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile};

    if reference.is_none() && !all {
        eprintln!("Error: provide a plugin reference or --all");
        std::process::exit(1);
    }

    let lockfile = PluginLockfile::read(plugins_dir)?;
    let lockfile = match lockfile {
        Some(lf) if !lf.is_empty() => lf,
        _ => {
            eprintln!("No plugins.lock found or no plugins installed. Nothing to upgrade.");
            eprintln!("Use 'plugin install' to install plugins first.");
            std::process::exit(1);
        }
    };

    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };
    let client = OciRegistryClient::new(config);
    let host_info = cli_host_version_info();
    let resolver = PluginResolver::new(&client, &host_info);

    // Determine which plugins to upgrade
    let to_check: Vec<(String, LockedPlugin)> = if all {
        lockfile.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    } else {
        let ref_str = reference.unwrap();
        match lockfile.get(ref_str) {
            Some(entry) => vec![(ref_str.to_string(), entry.clone())],
            None => {
                // Try to find by partial match (e.g., "source/postgres" matching a full OCI ref)
                let matches: Vec<_> = lockfile
                    .iter()
                    .filter(|(k, _)| k.contains(ref_str))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if matches.is_empty() {
                    eprintln!("Plugin '{}' not found in lockfile.", ref_str);
                    eprintln!("Installed plugins:");
                    for key in lockfile.keys() {
                        eprintln!("  {}", key);
                    }
                    std::process::exit(1);
                }
                matches
            }
        }
    };

    if dry_run {
        println!("Checking for upgrades (dry run)...");
    } else {
        println!("Checking for upgrades...");
    }

    let mut upgraded = 0;
    let mut up_to_date = 0;
    let mut failed = 0;
    let mut new_lockfile = lockfile.clone();

    for (ref_key, current) in &to_check {
        // Skip non-OCI plugins (file:// and http:// can't be auto-upgraded)
        if ref_key.starts_with("file://") || ref_key.starts_with("http://") || ref_key.starts_with("https://") {
            println!("  {} — skipped (non-OCI source)", ref_key);
            continue;
        }

        // Strip any existing tag for latest-compatible resolution
        let base_ref = ref_key.split(':').next().unwrap_or(ref_key);

        match resolver.resolve(base_ref, &registry_url).await {
            Ok(resolved) => {
                if resolved.digest == current.digest {
                    println!(
                        "  {} — up to date ({})",
                        ref_key, current.version
                    );
                    up_to_date += 1;
                } else if !dry_run {
                    println!(
                        "  {} — upgrading {} → {}",
                        ref_key, current.version, resolved.version
                    );
                    match client
                        .download_plugin(&resolved.reference, plugins_dir, &resolved.filename)
                        .await
                    {
                        Ok(_) => {
                            new_lockfile.insert(
                                ref_key.clone(),
                                LockedPlugin {
                                    reference: resolved.reference,
                                    version: resolved.version,
                                    digest: resolved.digest,
                                    sdk_version: resolved.sdk_version,
                                    core_version: resolved.core_version,
                                    lib_version: resolved.lib_version,
                                    platform: resolved.platform,
                                    filename: resolved.filename,
                                },
                            );
                            upgraded += 1;
                        }
                        Err(e) => {
                            eprintln!("  {} — download failed: {}", ref_key, e);
                            failed += 1;
                        }
                    }
                } else {
                    println!(
                        "  {} — {} → {} (available)",
                        ref_key, current.version, resolved.version
                    );
                    upgraded += 1;
                }
            }
            Err(e) => {
                eprintln!("  {} — resolve failed: {}", ref_key, e);
                failed += 1;
            }
        }
    }

    if !dry_run && upgraded > 0 {
        new_lockfile.write(plugins_dir)?;
    }

    println!();
    if dry_run {
        println!(
            "Dry run complete: {} upgradable, {} up to date, {} failed",
            upgraded, up_to_date, failed
        );
    } else {
        println!(
            "Upgrade complete: {} upgraded, {} up to date, {} failed",
            upgraded, up_to_date, failed
        );
    }

    Ok(())
}

#[cfg(not(feature = "dynamic-plugins"))]
async fn plugin_upgrade(
    _plugins_dir: &std::path::Path,
    _config_path: &std::path::Path,
    _reference: Option<&str>,
    _all: bool,
    _registry_override: Option<&str>,
    _dry_run: bool,
) -> Result<()> {
    eprintln!("Plugin management requires the 'dynamic-plugins' feature.");
    eprintln!("Rebuild with: cargo build --no-default-features --features dynamic-plugins");
    std::process::exit(1);
}

/// Get plugin registry URL from config or override.
fn get_plugin_registry(config_path: &std::path::Path, override_registry: Option<&str>) -> String {
    if let Some(r) = override_registry {
        return r.to_string();
    }
    if let Ok(config) = load_config_file(config_path) {
        config
            .plugin_registry
            .unwrap_or_else(|| "ghcr.io/drasi-project".to_string())
    } else {
        "ghcr.io/drasi-project".to_string()
    }
}

/// Get registry auth from environment for CLI commands.
#[cfg(feature = "dynamic-plugins")]
fn get_cli_registry_auth() -> drasi_host_sdk::registry::RegistryAuth {
    let password = std::env::var("OCI_REGISTRY_PASSWORD")
        .or_else(|_| std::env::var("GHCR_TOKEN"))
        .ok();
    match password {
        Some(pwd) => {
            let username = std::env::var("OCI_REGISTRY_USERNAME").unwrap_or_default();
            drasi_host_sdk::registry::RegistryAuth::Basic {
                username,
                password: pwd,
            }
        }
        None => drasi_host_sdk::registry::RegistryAuth::Anonymous,
    }
}

/// Build host version info for CLI commands.
#[cfg(feature = "dynamic-plugins")]
fn cli_host_version_info() -> drasi_host_sdk::registry::HostVersionInfo {
    drasi_host_sdk::registry::HostVersionInfo {
        sdk_version: env!("DRASI_PLUGIN_SDK_VERSION").to_string(),
        core_version: env!("DRASI_CORE_VERSION").to_string(),
        lib_version: env!("DRASI_LIB_VERSION").to_string(),
        target_triple: env!("TARGET_TRIPLE").to_string(),
    }
}
