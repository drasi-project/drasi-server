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

//! Interactive prompt functions for configuration initialization.

use std::path::Path;

use anyhow::Result;
use inquire::{Confirm, MultiSelect, Password, Select, Text};

use drasi_server::api::models::StateStoreConfig;
use drasi_server::api::models::{
    BootstrapProviderConfig, BootstrapProviderRef, ReactionConfig, SourceConfig,
};
use drasi_server::plugin_operations::PluginOperations;

/// Print a dim-colored description line before a prompt, with a blank line separator.
fn hint(description: &str) {
    use console::Style;
    let dim = Style::new().dim();
    println!();
    println!("  {}", dim.apply_to(description));
}

/// Server settings collected from user prompts.
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub persist_index: bool,
    pub state_store: Option<StateStoreConfig>,
    pub hot_reload_plugins: bool,
    pub plugin_registry: String,
    pub auto_install_plugins: bool,
    pub verify_plugins: bool,
}

/// A plugin discovered by scanning the plugins directory.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub kind: String,
    pub version: String,
    pub filename: String,
}

impl std::fmt::Display for DiscoveredPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (v{}, from {})",
            self.kind, self.version, self.filename
        )
    }
}

/// Plugins discovered by scanning the local plugins directory, grouped by category.
#[derive(Debug, Clone, Default)]
pub struct DiscoveredPlugins {
    pub sources: Vec<DiscoveredPlugin>,
    pub reactions: Vec<DiscoveredPlugin>,
    pub bootstrappers: Vec<DiscoveredPlugin>,
}

/// Discover available plugins by scanning a local plugins directory.
///
/// Uses `PluginOperations::scan_local_plugins()` which reads plugin metadata
/// (calling `drasi_plugin_metadata()` only, no `drasi_plugin_init()`)
/// and groups them by category based on their `plugin_id` prefix
/// (e.g. `"source/postgres"`, `"reaction/log"`).
pub fn discover_available_plugins(plugins_dir: &Path) -> DiscoveredPlugins {
    let ops = PluginOperations::new(
        plugins_dir.to_path_buf(),
        "ghcr.io/drasi-project".to_string(),
    );
    let summaries = match ops.scan_local_plugins() {
        Ok(s) => s,
        Err(_) => return DiscoveredPlugins::default(),
    };

    let mut result = DiscoveredPlugins::default();
    for summary in summaries {
        let filename = summary
            .file_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        // plugin_id is "type/kind", e.g. "source/postgres", "reaction/log"
        let parts: Vec<&str> = summary.plugin_id.splitn(2, '/').collect();
        if parts.len() != 2 {
            continue;
        }
        let (category, kind) = (parts[0], parts[1]);

        let plugin = DiscoveredPlugin {
            kind: kind.to_string(),
            version: summary.version.clone(),
            filename,
        };

        match category {
            "source" => result.sources.push(plugin),
            "reaction" => result.reactions.push(plugin),
            "bootstrap" => result.bootstrappers.push(plugin),
            _ => {}
        }
    }

    result
}

/// Bootstrap provider type selection options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapType {
    None,
    Postgres,
    ScriptFile,
}

impl std::fmt::Display for BootstrapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootstrapType::None => write!(f, "None - No initial data loading"),
            BootstrapType::Postgres => {
                write!(f, "PostgreSQL - Load initial data from PostgreSQL")
            }
            BootstrapType::ScriptFile => write!(f, "Script File - Load from JSONL file"),
        }
    }
}

/// State store type selection options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateStoreType {
    None,
    Redb,
}

impl std::fmt::Display for StateStoreType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateStoreType::None => write!(f, "None - In-memory state (lost on restart)"),
            StateStoreType::Redb => write!(f, "REDB - Persistent file-based state"),
        }
    }
}

/// Prompt for server settings (host, port, log level).
pub fn prompt_server_settings() -> Result<ServerSettings> {
    println!("Server Settings");
    println!("---------------");

    hint("IP address to bind to (0.0.0.0 for all interfaces)");
    let host = Text::new("Server host:").with_default("0.0.0.0").prompt()?;

    hint("Port for the REST API");
    let port_str = Text::new("Server port:").with_default("8080").prompt()?;

    let port: u16 = port_str.parse().unwrap_or(8080);

    hint("Logging verbosity");
    let log_levels = vec!["info", "debug", "warn", "error", "trace"];
    let log_level = Select::new("Log level:", log_levels).prompt()?.to_string();

    hint("Persists query index data to disk. Use for production workloads.");
    let persist_index = Confirm::new("Enable persistent indexing (RocksDB)?")
        .with_default(false)
        .prompt()?;

    // Prompt for state store configuration
    let state_store = prompt_state_store()?;

    // Prompt for plugin settings
    hint("Default registry for downloading plugins");
    let plugin_registry = Text::new("Plugin registry (OCI URL or local path):")
        .with_default("ghcr.io/drasi-project")
        .prompt()?;

    hint("Verify cosign signatures on downloaded plugins for supply-chain security");
    let verify_plugins = Confirm::new("Enable plugin signature verification (cosign)?")
        .with_default(false)
        .prompt()?;

    hint("Automatically download missing plugins when the server starts");
    let auto_install_plugins = Confirm::new("Auto-install plugins from registry on startup?")
        .with_default(false)
        .prompt()?;

    // Prompt for hot-reload settings
    hint("Automatically detect and reload plugins when files change on disk");
    let hot_reload_plugins = Confirm::new("Enable hot-reload for plugins?")
        .with_default(false)
        .prompt()?;

    println!();

    Ok(ServerSettings {
        host,
        port,
        log_level,
        persist_index,
        state_store,
        hot_reload_plugins,
        plugin_registry,
        auto_install_plugins,
        verify_plugins,
    })
}

/// Prompt for state store configuration.
fn prompt_state_store() -> Result<Option<StateStoreConfig>> {
    let state_store_types = vec![StateStoreType::None, StateStoreType::Redb];

    hint("Allows plugins to persist runtime state that survives restarts");
    let selected = Select::new(
        "State store (for plugin state persistence):",
        state_store_types,
    )
    .prompt()?;

    match selected {
        StateStoreType::None => Ok(None),
        StateStoreType::Redb => {
            hint("Path to REDB database file for state persistence");
            let path = Text::new("State store file path:")
                .with_default("./data/state.redb")
                .prompt()?;

            Ok(Some(StateStoreConfig::redb(path)))
        }
    }
}

/// Prompt for PostgreSQL source configuration.
fn prompt_postgres_source() -> Result<SourceConfig> {
    println!("Configuring PostgreSQL Source");
    println!("------------------------------");

    let id = Text::new("Source ID:")
        .with_default("postgres-source")
        .prompt()?;

    hint("Use ${DB_HOST} for environment variable");
    let host = Text::new("Database host:")
        .with_default("localhost")
        .prompt()?;

    let port_str = Text::new("Database port:").with_default("5432").prompt()?;
    let port: u16 = port_str.parse().unwrap_or(5432);

    hint("Use ${DB_NAME} for environment variable");
    let database = Text::new("Database name:")
        .with_default("postgres")
        .prompt()?;

    hint("Use ${DB_USER} for environment variable");
    let user = Text::new("Database user:")
        .with_default("postgres")
        .prompt()?;

    hint("Use ${DB_PASSWORD} for environment variable, or leave empty");
    let password = Password::new("Database password:")
        .without_confirmation()
        .prompt()?;

    hint("e.g., users,orders,products");
    let tables_str = Text::new("Tables to monitor (comma-separated):")
        .with_default("my_table")
        .prompt()?;

    let tables: Vec<String> = tables_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Ask about table keys (primary keys for tables without them)
    let table_keys = prompt_table_keys(&tables)?;

    // Ask about bootstrap provider
    let bootstrap_provider =
        prompt_bootstrap_provider_for_postgres(&host, port, &database, &user, &password, &tables)?;

    Ok(SourceConfig {
        kind: "postgres".to_string(),
        id,
        auto_start: true,
        bootstrap_provider: bootstrap_provider.map(BootstrapProviderRef::Inline),
        identity_provider: None,
        config: serde_json::json!({
            "host": host,
            "port": port,
            "database": database,
            "user": user,
            "password": password,
            "tables": tables,
            "slotName": "drasi_slot",
            "publicationName": "drasi_pub",
            "sslMode": "prefer",
            "tableKeys": table_keys
        }),
    })
}

/// Prompt for bootstrap provider for PostgreSQL source.
fn prompt_bootstrap_provider_for_postgres(
    host: &str,
    port: u16,
    database: &str,
    user: &str,
    password: &str,
    tables: &[String],
) -> Result<Option<BootstrapProviderConfig>> {
    let bootstrap_types = vec![
        BootstrapType::Postgres,
        BootstrapType::ScriptFile,
        BootstrapType::None,
    ];

    hint("Load existing data when starting");
    let selected = Select::new(
        "Bootstrap provider (for initial data loading):",
        bootstrap_types,
    )
    .prompt()?;

    match selected {
        BootstrapType::None => Ok(None),
        BootstrapType::Postgres => Ok(Some(BootstrapProviderConfig {
            kind: "postgres".to_string(),
            id: None,
            config: serde_json::json!({
                "host": host,
                "port": port,
                "database": database,
                "user": user,
                "password": password,
                "tables": tables,
                "slotName": "drasi_slot",
                "publicationName": "drasi_pub",
                "sslMode": "prefer"
            }),
        })),
        BootstrapType::ScriptFile => prompt_scriptfile_bootstrap(),
    }
}

/// Prompt for table keys configuration.
/// Table keys are needed for tables that don't have a primary key defined.
fn prompt_table_keys(tables: &[String]) -> Result<Vec<serde_json::Value>> {
    hint("Required for tables lacking a primary key constraint");
    let configure_keys = Confirm::new("Configure table keys for tables without primary keys?")
        .with_default(false)
        .prompt()?;

    if !configure_keys {
        return Ok(vec![]);
    }

    let mut table_keys = Vec::new();

    // Let user select which tables need key configuration
    let tables_needing_keys = if tables.len() == 1 {
        // If only one table, just ask if it needs keys
        let needs_keys = Confirm::new(&format!(
            "Does table '{}' need key columns specified?",
            tables[0]
        ))
        .with_default(true)
        .prompt()?;

        if needs_keys {
            vec![tables[0].clone()]
        } else {
            vec![]
        }
    } else {
        // Multiple tables - let user select which ones
        MultiSelect::new("Select tables that need key columns:", tables.to_vec())
            .with_help_message("Space to select, Enter to confirm")
            .prompt()?
    };

    for table in tables_needing_keys {
        hint("e.g., id or user_id,timestamp");
        let key_columns_str =
            Text::new(&format!("Key columns for '{table}' (comma-separated):")).prompt()?;

        let key_columns: Vec<String> = key_columns_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if !key_columns.is_empty() {
            table_keys.push(serde_json::json!({
                "table": table,
                "keyColumns": key_columns
            }));
        }
    }

    Ok(table_keys)
}

/// Prompt for HTTP source configuration.
fn prompt_http_source() -> Result<SourceConfig> {
    println!("Configuring HTTP Source");
    println!("-----------------------");

    let id = Text::new("Source ID:")
        .with_default("http-source")
        .prompt()?;

    let host = Text::new("Listen host:").with_default("0.0.0.0").prompt()?;

    hint("Port to receive HTTP events on");
    let port_str = Text::new("Listen port:").with_default("9000").prompt()?;
    let port: u16 = port_str.parse().unwrap_or(9000);

    // Ask about bootstrap provider
    let bootstrap_provider = prompt_bootstrap_provider_generic()?;

    Ok(SourceConfig {
        kind: "http".to_string(),
        id,
        auto_start: true,
        bootstrap_provider: bootstrap_provider.map(BootstrapProviderRef::Inline),
        identity_provider: None,
        config: serde_json::json!({
            "host": host,
            "port": port,
            "timeoutMs": 10000
        }),
    })
}

/// Prompt for gRPC source configuration.
fn prompt_grpc_source() -> Result<SourceConfig> {
    println!("Configuring gRPC Source");
    println!("-----------------------");

    let id = Text::new("Source ID:")
        .with_default("grpc-source")
        .prompt()?;

    let host = Text::new("Listen host:").with_default("0.0.0.0").prompt()?;

    hint("Port to receive gRPC streams on");
    let port_str = Text::new("Listen port:").with_default("50051").prompt()?;
    let port: u16 = port_str.parse().unwrap_or(50051);

    // Ask about bootstrap provider
    let bootstrap_provider = prompt_bootstrap_provider_generic()?;

    Ok(SourceConfig {
        kind: "grpc".to_string(),
        id,
        auto_start: true,
        bootstrap_provider: bootstrap_provider.map(BootstrapProviderRef::Inline),
        identity_provider: None,
        config: serde_json::json!({
            "host": host,
            "port": port,
            "timeoutMs": 5000
        }),
    })
}

/// Prompt for Mock source configuration.
fn prompt_mock_source() -> Result<SourceConfig> {
    println!("Configuring Mock Source");
    println!("-----------------------");

    let id = Text::new("Source ID:")
        .with_default("mock-source")
        .prompt()?;

    let data_type_options = vec!["generic", "sensorReading", "counter"];
    hint("Type of synthetic data to generate");
    let data_type_selection = Select::new("Data type to generate:", data_type_options).prompt()?;

    let data_type = match data_type_selection {
        "counter" => serde_json::json!({"type": "counter"}),
        "sensorReading" => {
            hint("How many unique sensors to simulate (1-100)");
            let sensor_count_str = Text::new("Number of sensors to simulate:")
                .with_default("5")
                .prompt()?;
            let sensor_count: u32 = sensor_count_str.parse().unwrap_or(5).clamp(1, 100);
            serde_json::json!({"type": "sensorReading", "sensorCount": sensor_count})
        }
        _ => serde_json::json!({"type": "generic"}),
    };

    hint("How often to generate test data (in milliseconds)");
    let interval_str = Text::new("Data generation interval (milliseconds):")
        .with_default("5000")
        .prompt()?;
    let interval_ms: u64 = interval_str.parse().unwrap_or(5000);

    Ok(SourceConfig {
        kind: "mock".to_string(),
        id,
        auto_start: true,
        bootstrap_provider: None,
        identity_provider: None,
        config: serde_json::json!({
            "intervalMs": interval_ms,
            "dataType": data_type
        }),
    })
}

/// Prompt for generic bootstrap provider selection (for non-Postgres sources).
fn prompt_bootstrap_provider_generic() -> Result<Option<BootstrapProviderConfig>> {
    let bootstrap_types = vec![BootstrapType::None, BootstrapType::ScriptFile];

    hint("Load existing data when starting");
    let selected = Select::new(
        "Bootstrap provider (for initial data loading):",
        bootstrap_types,
    )
    .prompt()?;

    match selected {
        BootstrapType::None => Ok(None),
        BootstrapType::ScriptFile => prompt_scriptfile_bootstrap(),
        BootstrapType::Postgres => Ok(None), // Not offered for non-Postgres sources
    }
}

/// Prompt for ScriptFile bootstrap configuration.
fn prompt_scriptfile_bootstrap() -> Result<Option<BootstrapProviderConfig>> {
    hint("Path to JSONL file with initial data");
    let file_path = Text::new("Bootstrap file path:")
        .with_default("data/bootstrap.jsonl")
        .prompt()?;

    Ok(Some(BootstrapProviderConfig {
        kind: "scriptfile".to_string(),
        id: None,
        config: serde_json::json!({
            "filePaths": [file_path]
        }),
    }))
}

/// Prompt for Log reaction configuration.
fn prompt_log_reaction() -> Result<ReactionConfig> {
    println!("Configuring Log Reaction");
    println!("------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("log-reaction")
        .prompt()?;

    Ok(ReactionConfig {
        kind: "log".to_string(),
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        identity_provider: None,
        config: serde_json::json!({
            "routes": {}
        }),
    })
}

/// Prompt for HTTP reaction configuration.
fn prompt_http_reaction() -> Result<ReactionConfig> {
    println!("Configuring HTTP Webhook Reaction");
    println!("----------------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("http-reaction")
        .prompt()?;

    hint("URL to POST query results to");
    let base_url = Text::new("Webhook base URL:")
        .with_default("http://localhost:9000")
        .prompt()?;

    Ok(ReactionConfig {
        kind: "http".to_string(),
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        identity_provider: None,
        config: serde_json::json!({
            "baseUrl": base_url,
            "timeoutMs": 5000,
            "routes": {}
        }),
    })
}

/// Prompt for SSE reaction configuration.
fn prompt_sse_reaction() -> Result<ReactionConfig> {
    println!("Configuring SSE Reaction");
    println!("------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("sse-reaction")
        .prompt()?;

    let host = Text::new("SSE server host:")
        .with_default("0.0.0.0")
        .prompt()?;

    hint("Port for SSE endpoint");
    let port_str = Text::new("SSE server port:")
        .with_default("8081")
        .prompt()?;
    let port: u16 = port_str.parse().unwrap_or(8081);

    Ok(ReactionConfig {
        kind: "sse".to_string(),
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        identity_provider: None,
        config: serde_json::json!({
            "host": host,
            "port": port,
            "ssePath": "/events",
            "heartbeatIntervalMs": 30000,
            "routes": {}
        }),
    })
}

/// Prompt for gRPC reaction configuration.
fn prompt_grpc_reaction() -> Result<ReactionConfig> {
    println!("Configuring gRPC Reaction");
    println!("-------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("grpc-reaction")
        .prompt()?;

    hint("Endpoint for gRPC streaming");
    let endpoint = Text::new("gRPC endpoint URL:")
        .with_default("grpc://localhost:50052")
        .prompt()?;

    Ok(ReactionConfig {
        kind: "grpc".to_string(),
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        identity_provider: None,
        config: serde_json::json!({
            "endpoint": endpoint,
            "timeoutMs": 5000,
            "batchSize": 100,
            "batchFlushTimeoutMs": 1000,
            "maxRetries": 3,
            "connectionRetryAttempts": 5,
            "initialConnectionTimeoutMs": 10000,
            "metadata": {}
        }),
    })
}

// ── Dynamic plugin discovery prompts ──

/// Known source kinds that have dedicated prompt functions.
#[cfg(test)]
const KNOWN_SOURCE_KINDS: &[&str] = &["postgres", "http", "grpc", "mock"];

/// Known reaction kinds that have dedicated prompt functions.
#[cfg(test)]
const KNOWN_REACTION_KINDS: &[&str] = &["log", "http", "sse", "grpc"];

/// A wrapper for displaying plugin choices in `MultiSelect` prompts.
#[derive(Debug, Clone)]
pub struct PluginChoice {
    pub kind: String,
    pub label: String,
}

impl std::fmt::Display for PluginChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

/// Unified select-or-install pattern for sources, bootstrappers, and reactions.
///
/// Shows locally installed plugins for the category with an "Install from registry"
/// option. If install is selected, searches the registry (filtered by category),
/// downloads selections, re-scans, and loops back.
pub async fn select_or_install_plugins(
    category: &str,
    plugins_dir: Option<&Path>,
    default_registry: &str,
) -> Result<Vec<String>> {
    let install_label = format!("\u{2B07} Install a {category} from a registry");

    loop {
        // Scan local plugins
        let discovered = plugins_dir
            .map(discover_available_plugins)
            .unwrap_or_default();

        let local_plugins = match category {
            "source" => &discovered.sources,
            "reaction" => &discovered.reactions,
            "bootstrap" => &discovered.bootstrappers,
            _ => return Ok(Vec::new()),
        };

        if local_plugins.is_empty() {
            println!("No {category} plugins installed locally.");
        }

        // Build selection list
        let mut items: Vec<PluginChoice> = local_plugins
            .iter()
            .map(|p| PluginChoice {
                kind: p.kind.clone(),
                label: p.to_string(),
            })
            .collect();

        items.push(PluginChoice {
            kind: "__install__".to_string(),
            label: install_label.clone(),
        });

        let prompt_msg = format!("Select {category}s (space to select, enter to confirm):");
        let selected = MultiSelect::new(&prompt_msg, items)
            .with_help_message("Use arrow keys to navigate, space to select/deselect")
            .prompt()?;

        let wants_install = selected.iter().any(|s| s.kind == "__install__");
        let actual_selections: Vec<String> = selected
            .iter()
            .filter(|s| s.kind != "__install__")
            .map(|s| s.kind.clone())
            .collect();

        if wants_install {
            if let Some(dir) = plugins_dir {
                let registry_url =
                    Text::new("Plugin source (registry URL or local directory path):")
                        .with_default(default_registry)
                        .prompt()?;

                println!("Searching {registry_url}...");
                let all_available = search_registry_plugins(&registry_url).await?;

                // Filter to this category only
                let category_prefix = format!("{category}/");
                let filtered: Vec<&RegistryPlugin> = all_available
                    .iter()
                    .filter(|p| p.reference.starts_with(&category_prefix))
                    .collect();

                if filtered.is_empty() {
                    println!("No {category} plugins found.");
                } else {
                    let options: Vec<String> = filtered.iter().map(|p| p.to_string()).collect();
                    let to_download =
                        MultiSelect::new(&format!("Select {category}s to install:"), options)
                            .prompt()?;

                    for display in &to_download {
                        if let Some(plugin) = filtered.iter().find(|p| p.to_string() == *display) {
                            println!("Installing {}...", plugin.reference);
                            match install_registry_plugin(&plugin.reference, dir, &registry_url)
                                .await
                            {
                                Ok(()) => println!("  \u{2713} Installed {}", plugin.reference),
                                Err(e) => println!("  \u{2717} Failed: {e}"),
                            }
                        }
                    }
                }
                println!();
                continue; // Loop back to show updated list
            }
        }

        // User made selection or selected nothing
        if actual_selections.is_empty() && !wants_install {
            println!("No {category}s selected. You can add them later by editing the config file.");
        }
        return Ok(actual_selections);
    }
}

/// Prompt for details of a source by its kind string.
///
/// If the kind matches a known built-in source, uses the dedicated prompt.
/// Otherwise, uses a generic JSON config prompt.
pub fn prompt_source_by_kind(kind: &str) -> Result<SourceConfig> {
    match kind {
        "postgres" => prompt_postgres_source(),
        "http" => prompt_http_source(),
        "grpc" => prompt_grpc_source(),
        "mock" => prompt_mock_source(),
        _ => prompt_generic_source(kind),
    }
}

/// Generic prompt for a source plugin with no dedicated prompt function.
///
/// NOTE: During `drasi init`, plugins are discovered via metadata-only scanning
/// (`drasi_plugin_metadata`), which does NOT call `drasi_plugin_init()`. This means
/// plugin descriptors (and their `config_schema_json()`) are not available for
/// unknown plugin kinds at init time.
fn prompt_generic_source(kind: &str) -> Result<SourceConfig> {
    println!("Configuring {kind} Source");
    println!("{}", "-".repeat(25 + kind.len()));

    let id = Text::new("Source ID:")
        .with_default(&format!("{kind}-source"))
        .prompt()?;

    hint("Enter plugin-specific config as a JSON object, or leave as {}");
    let config_json = Text::new("Configuration (JSON):")
        .with_default("{}")
        .prompt()?;

    let config: serde_json::Value =
        serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));

    let bootstrap_provider = prompt_bootstrap_provider_generic()?;

    Ok(SourceConfig {
        kind: kind.to_string(),
        id,
        auto_start: true,
        identity_provider: None,
        bootstrap_provider: bootstrap_provider.map(BootstrapProviderRef::Inline),
        config,
    })
}

/// Attach a bootstrap provider to a source config by kind.
pub fn attach_bootstrap_to_source(
    mut source: SourceConfig,
    bootstrap_kind: &str,
) -> Result<SourceConfig> {
    let bootstrap = match bootstrap_kind {
        "postgres" => {
            // Reuse postgres bootstrap with generic config
            let host = Text::new("Bootstrap DB host:")
                .with_default("localhost")
                .prompt()?;
            let port_str = Text::new("Bootstrap DB port:")
                .with_default("5432")
                .prompt()?;
            let port: u16 = port_str.parse().unwrap_or(5432);
            let database = Text::new("Bootstrap DB name:")
                .with_default("postgres")
                .prompt()?;
            let user = Text::new("Bootstrap DB user:")
                .with_default("postgres")
                .prompt()?;
            let password = Password::new("Bootstrap DB password:")
                .without_confirmation()
                .prompt()?;

            Some(BootstrapProviderConfig {
                kind: "postgres".to_string(),
                id: None,
                config: serde_json::json!({
                    "host": host,
                    "port": port,
                    "database": database,
                    "user": user,
                    "password": password,
                    "sslMode": "prefer"
                }),
            })
        }
        "scriptfile" => prompt_scriptfile_bootstrap()?,
        _ => {
            // Generic bootstrap config
            let config_json = Text::new(&format!("{bootstrap_kind} bootstrap config (JSON):"))
                .with_default("{}")
                .prompt()?;
            let config =
                serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));
            Some(BootstrapProviderConfig {
                kind: bootstrap_kind.to_string(),
                id: None,
                config,
            })
        }
    };
    source.bootstrap_provider = bootstrap.map(BootstrapProviderRef::Inline);
    Ok(source)
}

/// Prompt for details of a reaction by its kind string.
///
/// If the kind matches a known built-in reaction, uses the dedicated prompt.
/// Otherwise, uses a generic JSON config prompt.
pub fn prompt_reaction_by_kind(kind: &str, _source_ids: &[String]) -> Result<ReactionConfig> {
    match kind {
        "log" => prompt_log_reaction(),
        "http" => prompt_http_reaction(),
        "sse" => prompt_sse_reaction(),
        "grpc" => prompt_grpc_reaction(),
        _ => prompt_generic_reaction(kind),
    }
}

/// Generic prompt for a reaction plugin with no dedicated prompt function.
///
/// See [`prompt_generic_source`] for why schema-driven prompts are not used here:
/// metadata-only scanning at init time does not load plugin descriptors.
fn prompt_generic_reaction(kind: &str) -> Result<ReactionConfig> {
    println!("Configuring {kind} Reaction");
    println!("{}", "-".repeat(25 + kind.len()));

    let id = Text::new("Reaction ID:")
        .with_default(&format!("{kind}-reaction"))
        .prompt()?;

    hint("Enter plugin-specific config as a JSON object, or leave as {}");
    let config_json = Text::new("Configuration (JSON):")
        .with_default("{}")
        .prompt()?;

    let config: serde_json::Value =
        serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));

    Ok(ReactionConfig {
        kind: kind.to_string(),
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        identity_provider: None,
        config,
    })
}

// ---------------------------------------------------------------------------
// Registry download helpers for init wizard
// ---------------------------------------------------------------------------

/// A plugin available in a remote OCI registry.
#[derive(Debug, Clone)]
pub struct RegistryPlugin {
    /// Short plugin reference (e.g., "source/postgres").
    pub reference: String,
    /// Available versions with their platforms.
    pub versions: Vec<(String, Vec<String>)>,
}

impl std::fmt::Display for RegistryPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let latest = self
            .versions
            .first()
            .map(|(v, _)| v.as_str())
            .unwrap_or("unknown");
        write!(f, "{} (latest: {})", self.reference, latest)
    }
}

/// Search a remote OCI registry or local directory for available plugins.
pub async fn search_registry_plugins(registry_url: &str) -> Result<Vec<RegistryPlugin>> {
    use drasi_host_sdk::registry::{PluginSourceKind, RegistryConfig};

    match PluginSourceKind::parse(registry_url) {
        PluginSourceKind::LocalDir(dir) => {
            let local = drasi_host_sdk::registry::LocalDirRegistry::new(&dir);
            let results = local.search("*")?;
            Ok(results
                .into_iter()
                .map(|r| {
                    let version_str = if r.version.is_empty() {
                        "unknown".to_string()
                    } else {
                        r.version
                    };
                    RegistryPlugin {
                        reference: r.reference,
                        versions: vec![(version_str, vec![env!("TARGET_TRIPLE").to_string()])],
                    }
                })
                .collect())
        }
        PluginSourceKind::Oci(_) => {
            let config = RegistryConfig {
                default_registry: registry_url.to_string(),
                auth: PluginOperations::registry_auth(),
            };
            let client = PluginOperations::build_registry_client(config);

            let results = client.search_plugins("*").await?;

            Ok(results
                .into_iter()
                .map(|r| RegistryPlugin {
                    reference: r.reference,
                    versions: r
                        .versions
                        .into_iter()
                        .map(|v| (v.version, v.platforms))
                        .collect(),
                })
                .collect())
        }
    }
}

/// Download a single plugin from the registry into the plugins directory.
pub async fn install_registry_plugin(
    reference: &str,
    plugins_dir: &Path,
    registry_url: &str,
) -> Result<()> {
    let ops = PluginOperations::new(plugins_dir.to_path_buf(), registry_url.to_string());
    ops.install_from_registry(reference, Some(registry_url))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ServerSettings tests ====================

    #[test]
    fn test_server_settings_creation() {
        let settings = ServerSettings {
            host: "127.0.0.1".to_string(),
            port: 9090,
            log_level: "debug".to_string(),
            persist_index: true,
            state_store: None,
            hot_reload_plugins: false,
            plugin_registry: "ghcr.io/drasi-project".to_string(),
            auto_install_plugins: false,
            verify_plugins: false,
        };

        assert_eq!(settings.host, "127.0.0.1");
        assert_eq!(settings.port, 9090);
        assert_eq!(settings.log_level, "debug");
        assert!(settings.persist_index);
        assert!(settings.state_store.is_none());
        assert!(!settings.hot_reload_plugins);
        assert_eq!(settings.plugin_registry, "ghcr.io/drasi-project");
        assert!(!settings.auto_install_plugins);
        assert!(!settings.verify_plugins);
    }

    #[test]
    fn test_server_settings_default_values() {
        let settings = ServerSettings {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            persist_index: false,
            state_store: None,
            hot_reload_plugins: false,
            plugin_registry: "ghcr.io/drasi-project".to_string(),
            auto_install_plugins: false,
            verify_plugins: false,
        };

        assert_eq!(settings.host, "0.0.0.0");
        assert_eq!(settings.port, 8080);
        assert_eq!(settings.log_level, "info");
        assert!(!settings.persist_index);
        assert!(settings.state_store.is_none());
    }

    #[test]
    fn test_server_settings_with_state_store() {
        let settings = ServerSettings {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            persist_index: false,
            state_store: Some(StateStoreConfig::redb("./data/state.redb")),
            hot_reload_plugins: false,
            plugin_registry: "ghcr.io/drasi-project".to_string(),
            auto_install_plugins: false,
            verify_plugins: false,
        };

        assert!(settings.state_store.is_some());
        assert_eq!(settings.state_store.as_ref().unwrap().kind(), "redb");
    }

    #[test]
    fn test_server_settings_with_hot_reload() {
        let settings = ServerSettings {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            persist_index: false,
            state_store: None,
            hot_reload_plugins: true,
            plugin_registry: "ghcr.io/drasi-project".to_string(),
            auto_install_plugins: false,
            verify_plugins: false,
        };

        assert!(settings.hot_reload_plugins);
    }

    // ==================== BootstrapType enum tests ====================

    #[test]
    fn test_bootstrap_type_display_none() {
        let bootstrap_type = BootstrapType::None;
        let display = bootstrap_type.to_string();
        assert!(display.contains("None"));
        assert!(display.contains("No initial data"));
    }

    #[test]
    fn test_bootstrap_type_display_postgres() {
        let bootstrap_type = BootstrapType::Postgres;
        let display = bootstrap_type.to_string();
        assert!(display.contains("PostgreSQL"));
        assert!(display.contains("initial data"));
    }

    #[test]
    fn test_bootstrap_type_display_scriptfile() {
        let bootstrap_type = BootstrapType::ScriptFile;
        let display = bootstrap_type.to_string();
        assert!(display.contains("Script File"));
        assert!(display.contains("JSONL"));
    }

    #[test]
    fn test_bootstrap_type_equality() {
        assert_eq!(BootstrapType::None, BootstrapType::None);
        assert_ne!(BootstrapType::Postgres, BootstrapType::ScriptFile);
    }

    #[test]
    fn test_bootstrap_type_debug() {
        let bootstrap_type = BootstrapType::ScriptFile;
        let debug = format!("{bootstrap_type:?}");
        assert_eq!(debug, "ScriptFile");
    }

    // ==================== StateStoreType enum tests ====================

    #[test]
    fn test_state_store_type_display_none() {
        let state_store_type = StateStoreType::None;
        let display = state_store_type.to_string();
        assert!(display.contains("None"));
        assert!(display.contains("In-memory"));
    }

    #[test]
    fn test_state_store_type_display_redb() {
        let state_store_type = StateStoreType::Redb;
        let display = state_store_type.to_string();
        assert!(display.contains("REDB"));
        assert!(display.contains("Persistent"));
    }

    #[test]
    fn test_state_store_type_equality() {
        assert_eq!(StateStoreType::None, StateStoreType::None);
        assert_eq!(StateStoreType::Redb, StateStoreType::Redb);
        assert_ne!(StateStoreType::None, StateStoreType::Redb);
    }

    #[test]
    fn test_state_store_type_debug() {
        let state_store_type = StateStoreType::Redb;
        let debug = format!("{state_store_type:?}");
        assert_eq!(debug, "Redb");
    }

    #[test]
    fn test_all_state_store_types_have_display() {
        let state_store_types = vec![StateStoreType::None, StateStoreType::Redb];

        for state_store_type in state_store_types {
            let display = state_store_type.to_string();
            assert!(
                !display.is_empty(),
                "StateStoreType {state_store_type:?} has empty display"
            );
        }
    }

    #[test]
    fn test_state_store_type_displays_are_descriptive() {
        assert!(StateStoreType::None.to_string().len() > 10);
        assert!(StateStoreType::Redb.to_string().len() > 10);
    }

    // ==================== DiscoveredPlugin / DiscoveredPlugins tests ====================

    #[test]
    fn test_discovered_plugin_display() {
        let plugin = DiscoveredPlugin {
            kind: "postgres".to_string(),
            version: "0.4.2".to_string(),
            filename: "libdrasi_source_postgres.dylib".to_string(),
        };
        let display = plugin.to_string();
        assert!(display.contains("postgres"));
        assert!(display.contains("0.4.2"));
        assert!(display.contains("libdrasi_source_postgres.dylib"));
    }

    #[test]
    fn test_discovered_plugins_grouping() {
        let plugins = DiscoveredPlugins {
            sources: vec![
                DiscoveredPlugin {
                    kind: "postgres".to_string(),
                    version: "1.0".to_string(),
                    filename: "libdrasi_source_postgres.so".to_string(),
                },
                DiscoveredPlugin {
                    kind: "http".to_string(),
                    version: "1.0".to_string(),
                    filename: "libdrasi_source_http.so".to_string(),
                },
            ],
            reactions: vec![DiscoveredPlugin {
                kind: "log".to_string(),
                version: "1.0".to_string(),
                filename: "libdrasi_reaction_log.so".to_string(),
            }],
            bootstrappers: vec![],
        };
        assert_eq!(plugins.sources.len(), 2);
        assert_eq!(plugins.reactions.len(), 1);
        assert!(plugins.bootstrappers.is_empty());
    }

    #[test]
    fn test_discover_available_plugins_nonexistent_dir() {
        use std::path::PathBuf;
        let result = discover_available_plugins(&PathBuf::from("/nonexistent/path/to/plugins"));
        assert!(result.sources.is_empty());
        assert!(result.reactions.is_empty());
        assert!(result.bootstrappers.is_empty());
    }

    #[test]
    fn test_known_source_kinds_list() {
        assert!(KNOWN_SOURCE_KINDS.contains(&"postgres"));
        assert!(KNOWN_SOURCE_KINDS.contains(&"http"));
        assert!(KNOWN_SOURCE_KINDS.contains(&"grpc"));
        assert!(KNOWN_SOURCE_KINDS.contains(&"mock"));
    }

    #[test]
    fn test_known_reaction_kinds_list() {
        assert!(KNOWN_REACTION_KINDS.contains(&"log"));
        assert!(KNOWN_REACTION_KINDS.contains(&"http"));
        assert!(KNOWN_REACTION_KINDS.contains(&"sse"));
        assert!(KNOWN_REACTION_KINDS.contains(&"grpc"));
    }
}
