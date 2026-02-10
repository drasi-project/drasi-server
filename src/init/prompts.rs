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

use anyhow::Result;
use inquire::{Confirm, MultiSelect, Password, Select, Text};

use drasi_server::api::models::sources::mock::DataTypeDto;
use drasi_server::api::models::{
    ApplicationBootstrapConfigDto, BootstrapProviderConfig, ConfigValue, GrpcReactionConfigDto,
    GrpcSourceConfigDto, HttpReactionConfigDto, HttpSourceConfigDto, LogReactionConfigDto,
    MockSourceConfigDto, PlatformBootstrapConfigDto, PlatformReactionConfigDto,
    PlatformSourceConfigDto, PostgresBootstrapConfigDto, PostgresSourceConfigDto, ReactionConfig,
    ScriptFileBootstrapConfigDto, SourceConfig, SseReactionConfigDto, SslModeDto,
    TableKeyConfigDto,
};

use drasi_server::api::models::StateStoreConfig;

/// Server settings collected from user prompts.
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub persist_index: bool,
    pub state_store: Option<StateStoreConfig>,
}

/// Source type selection options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    Postgres,
    Http,
    Grpc,
    Mock,
    Platform,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Postgres => write!(f, "PostgreSQL - CDC from PostgreSQL database"),
            SourceType::Http => write!(f, "HTTP - Receive events via HTTP endpoint"),
            SourceType::Grpc => write!(f, "gRPC - Stream events via gRPC"),
            SourceType::Mock => write!(f, "Mock - Generate test data (for development)"),
            SourceType::Platform => write!(f, "Platform - Redis Streams integration"),
        }
    }
}

/// Bootstrap provider type selection options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapType {
    None,
    Postgres,
    ScriptFile,
    Platform,
}

impl std::fmt::Display for BootstrapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootstrapType::None => write!(f, "None - No initial data loading"),
            BootstrapType::Postgres => {
                write!(f, "PostgreSQL - Load initial data from PostgreSQL")
            }
            BootstrapType::ScriptFile => write!(f, "Script File - Load from JSONL file"),
            BootstrapType::Platform => write!(f, "Platform - Load from Redis/Platform"),
        }
    }
}

/// Reaction type selection options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionType {
    Log,
    Http,
    Sse,
    Grpc,
    Platform,
}

impl std::fmt::Display for ReactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReactionType::Log => write!(f, "Log - Write query results to console"),
            ReactionType::Http => write!(f, "HTTP Webhook - POST results to external URL"),
            ReactionType::Sse => write!(f, "SSE - Server-Sent Events endpoint"),
            ReactionType::Grpc => write!(f, "gRPC - Stream results via gRPC"),
            ReactionType::Platform => write!(f, "Platform - Drasi Platform integration"),
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

    let host = Text::new("Server host:")
        .with_default("0.0.0.0")
        .with_help_message("IP address to bind to (0.0.0.0 for all interfaces)")
        .prompt()?;

    let port_str = Text::new("Server port:")
        .with_default("8080")
        .with_help_message("Port for the REST API")
        .prompt()?;

    let port: u16 = port_str.parse().unwrap_or(8080);

    let log_levels = vec!["info", "debug", "warn", "error", "trace"];
    let log_level = Select::new("Log level:", log_levels)
        .with_help_message("Logging verbosity")
        .prompt()?
        .to_string();

    let persist_index = Confirm::new("Enable persistent indexing (RocksDB)?")
        .with_default(false)
        .with_help_message("Persists query index data to disk. Use for production workloads.")
        .prompt()?;

    // Prompt for state store configuration
    let state_store = prompt_state_store()?;

    println!();

    Ok(ServerSettings {
        host,
        port,
        log_level,
        persist_index,
        state_store,
    })
}

/// Prompt for state store configuration.
fn prompt_state_store() -> Result<Option<StateStoreConfig>> {
    let state_store_types = vec![StateStoreType::None, StateStoreType::Redb];

    let selected = Select::new(
        "State store (for plugin state persistence):",
        state_store_types,
    )
    .with_help_message("Allows plugins to persist runtime state that survives restarts")
    .prompt()?;

    match selected {
        StateStoreType::None => Ok(None),
        StateStoreType::Redb => {
            let path = Text::new("State store file path:")
                .with_default("./data/state.redb")
                .with_help_message("Path to REDB database file for state persistence")
                .prompt()?;

            Ok(Some(StateStoreConfig::redb(path)))
        }
    }
}

/// Prompt for source selection and configuration.
pub fn prompt_sources() -> Result<Vec<SourceConfig>> {
    println!("Data Sources");
    println!("------------");
    println!("Select one or more data sources for your configuration.");
    println!();

    let source_types = vec![
        SourceType::Postgres,
        SourceType::Http,
        SourceType::Grpc,
        SourceType::Mock,
        SourceType::Platform,
    ];

    let selected = MultiSelect::new(
        "Select sources (space to select, enter to confirm):",
        source_types,
    )
    .with_help_message("Use arrow keys to navigate, space to select/deselect")
    .prompt()?;

    if selected.is_empty() {
        println!("No sources selected. You can add sources later by editing the config file.");
        println!();
        return Ok(Vec::new());
    }

    let mut sources = Vec::new();

    for source_type in selected {
        println!();
        let source = prompt_source_details(source_type)?;
        sources.push(source);
    }

    println!();
    Ok(sources)
}

/// Prompt for details of a specific source type.
fn prompt_source_details(source_type: SourceType) -> Result<SourceConfig> {
    match source_type {
        SourceType::Postgres => prompt_postgres_source(),
        SourceType::Http => prompt_http_source(),
        SourceType::Grpc => prompt_grpc_source(),
        SourceType::Mock => prompt_mock_source(),
        SourceType::Platform => prompt_platform_source(),
    }
}

/// Prompt for PostgreSQL source configuration.
fn prompt_postgres_source() -> Result<SourceConfig> {
    println!("Configuring PostgreSQL Source");
    println!("------------------------------");

    let id = Text::new("Source ID:")
        .with_default("postgres-source")
        .prompt()?;

    let host = Text::new("Database host:")
        .with_default("localhost")
        .with_help_message("Use ${DB_HOST} for environment variable")
        .prompt()?;

    let port_str = Text::new("Database port:").with_default("5432").prompt()?;
    let port: u16 = port_str.parse().unwrap_or(5432);

    let database = Text::new("Database name:")
        .with_default("postgres")
        .with_help_message("Use ${DB_NAME} for environment variable")
        .prompt()?;

    let user = Text::new("Database user:")
        .with_default("postgres")
        .with_help_message("Use ${DB_USER} for environment variable")
        .prompt()?;

    let password = Password::new("Database password:")
        .with_help_message("Use ${DB_PASSWORD} for environment variable, or leave empty")
        .without_confirmation()
        .prompt()?;

    let tables_str = Text::new("Tables to monitor (comma-separated):")
        .with_default("my_table")
        .with_help_message("e.g., users,orders,products")
        .prompt()?;

    let tables: Vec<String> = tables_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Ask about table keys (primary keys for tables without them)
    let table_keys = prompt_table_keys(&tables)?;

    // Ask about bootstrap provider
    let bootstrap_provider = prompt_bootstrap_provider_for_postgres()?;

    Ok(SourceConfig::Postgres {
        id,
        auto_start: true,
        bootstrap_provider,
        config: PostgresSourceConfigDto {
            host: ConfigValue::Static(host),
            port: ConfigValue::Static(port),
            database: ConfigValue::Static(database),
            user: ConfigValue::Static(user),
            password: ConfigValue::Static(password),
            tables,
            slot_name: "drasi_slot".to_string(),
            publication_name: "drasi_pub".to_string(),
            ssl_mode: ConfigValue::Static(SslModeDto::Prefer),
            table_keys,
        },
    })
}

/// Prompt for bootstrap provider for PostgreSQL source.
fn prompt_bootstrap_provider_for_postgres() -> Result<Option<BootstrapProviderConfig>> {
    let bootstrap_types = vec![
        BootstrapType::Postgres,
        BootstrapType::ScriptFile,
        BootstrapType::None,
    ];

    let selected = Select::new(
        "Bootstrap provider (for initial data loading):",
        bootstrap_types,
    )
    .with_help_message("Load existing data when starting")
    .prompt()?;

    match selected {
        BootstrapType::None => Ok(None),
        BootstrapType::Postgres => {
            // PostgresBootstrapConfig is now an empty struct - connection details come from source
            Ok(Some(BootstrapProviderConfig::Postgres(
                PostgresBootstrapConfigDto::default(),
            )))
        }
        BootstrapType::ScriptFile => prompt_scriptfile_bootstrap(),
        BootstrapType::Platform => prompt_platform_bootstrap(),
    }
}

/// Prompt for table keys configuration.
/// Table keys are needed for tables that don't have a primary key defined.
fn prompt_table_keys(tables: &[String]) -> Result<Vec<TableKeyConfigDto>> {
    let configure_keys = Confirm::new("Configure table keys for tables without primary keys?")
        .with_default(false)
        .with_help_message("Required for tables lacking a primary key constraint")
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
        MultiSelect::new(
            "Select tables that need key columns:",
            tables.to_vec(),
        )
        .with_help_message("Space to select, Enter to confirm")
        .prompt()?
    };

    for table in tables_needing_keys {
        let key_columns_str = Text::new(&format!("Key columns for '{table}' (comma-separated):"))
            .with_help_message("e.g., id or user_id,timestamp")
            .prompt()?;

        let key_columns: Vec<String> = key_columns_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if !key_columns.is_empty() {
            table_keys.push(TableKeyConfigDto { table, key_columns });
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

    let port_str = Text::new("Listen port:")
        .with_default("9000")
        .with_help_message("Port to receive HTTP events on")
        .prompt()?;
    let port: u16 = port_str.parse().unwrap_or(9000);

    // Ask about bootstrap provider
    let bootstrap_provider = prompt_bootstrap_provider_generic()?;

    Ok(SourceConfig::Http {
        id,
        auto_start: true,
        bootstrap_provider,
        config: HttpSourceConfigDto {
            host: ConfigValue::Static(host),
            port: ConfigValue::Static(port),
            endpoint: None,
            timeout_ms: ConfigValue::Static(10000),
            adaptive_max_batch_size: None,
            adaptive_min_batch_size: None,
            adaptive_max_wait_ms: None,
            adaptive_min_wait_ms: None,
            adaptive_window_secs: None,
            adaptive_enabled: None,
        },
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

    let port_str = Text::new("Listen port:")
        .with_default("50051")
        .with_help_message("Port to receive gRPC streams on")
        .prompt()?;
    let port: u16 = port_str.parse().unwrap_or(50051);

    // Ask about bootstrap provider
    let bootstrap_provider = prompt_bootstrap_provider_generic()?;

    Ok(SourceConfig::Grpc {
        id,
        auto_start: true,
        bootstrap_provider,
        config: GrpcSourceConfigDto {
            host: ConfigValue::Static(host),
            port: ConfigValue::Static(port),
            endpoint: None,
            timeout_ms: ConfigValue::Static(5000),
        },
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
    let data_type_selection = Select::new("Data type to generate:", data_type_options)
        .with_help_message("Type of synthetic data to generate")
        .prompt()?;

    let data_type = match data_type_selection {
        "counter" => DataTypeDto::Counter,
        "sensorReading" => {
            let sensor_count_str = Text::new("Number of sensors to simulate:")
                .with_default("5")
                .with_help_message("How many unique sensors to simulate (1-100)")
                .prompt()?;
            let sensor_count: u32 = sensor_count_str.parse().unwrap_or(5).clamp(1, 100);
            DataTypeDto::SensorReading { sensor_count }
        }
        _ => DataTypeDto::Generic,
    };

    let interval_str = Text::new("Data generation interval (milliseconds):")
        .with_default("5000")
        .with_help_message("How often to generate test data (in milliseconds)")
        .prompt()?;
    let interval_ms: u64 = interval_str.parse().unwrap_or(5000);

    Ok(SourceConfig::Mock {
        id,
        auto_start: true,
        bootstrap_provider: None,
        config: MockSourceConfigDto {
            interval_ms: ConfigValue::Static(interval_ms),
            data_type,
        },
    })
}

/// Prompt for Platform source configuration.
fn prompt_platform_source() -> Result<SourceConfig> {
    println!("Configuring Platform Source");
    println!("---------------------------");

    let id = Text::new("Source ID:")
        .with_default("platform-source")
        .prompt()?;

    let redis_url = Text::new("Redis URL:")
        .with_default("redis://localhost:6379")
        .with_help_message("Redis connection URL for streams")
        .prompt()?;

    let stream_key = Text::new("Stream key in Redis:")
        .with_default("external-source:changes")
        .with_help_message("Redis stream key to consume from")
        .prompt()?;

    let consumer_group = Text::new("Consumer group name:")
        .with_default("drasi-core")
        .prompt()?;

    // Ask about bootstrap provider
    let bootstrap_provider = prompt_bootstrap_provider_generic()?;

    Ok(SourceConfig::Platform {
        id,
        auto_start: true,
        bootstrap_provider,
        config: PlatformSourceConfigDto {
            redis_url: ConfigValue::Static(redis_url),
            stream_key: ConfigValue::Static(stream_key),
            consumer_group: ConfigValue::Static(consumer_group),
            consumer_name: None,
            batch_size: ConfigValue::Static(100),
            block_ms: ConfigValue::Static(5000),
        },
    })
}

/// Prompt for generic bootstrap provider selection (for non-Postgres sources).
fn prompt_bootstrap_provider_generic() -> Result<Option<BootstrapProviderConfig>> {
    let bootstrap_types = vec![
        BootstrapType::None,
        BootstrapType::ScriptFile,
        BootstrapType::Platform,
    ];

    let selected = Select::new(
        "Bootstrap provider (for initial data loading):",
        bootstrap_types,
    )
    .with_help_message("Load existing data when starting")
    .prompt()?;

    match selected {
        BootstrapType::None => Ok(None),
        BootstrapType::ScriptFile => prompt_scriptfile_bootstrap(),
        BootstrapType::Platform => prompt_platform_bootstrap(),
        BootstrapType::Postgres => Ok(None), // Not offered for non-Postgres sources
    }
}

/// Prompt for ScriptFile bootstrap configuration.
fn prompt_scriptfile_bootstrap() -> Result<Option<BootstrapProviderConfig>> {
    let file_path = Text::new("Bootstrap file path:")
        .with_default("data/bootstrap.jsonl")
        .with_help_message("Path to JSONL file with initial data")
        .prompt()?;

    Ok(Some(BootstrapProviderConfig::ScriptFile(
        ScriptFileBootstrapConfigDto {
            file_paths: vec![file_path],
        },
    )))
}

/// Prompt for Platform bootstrap configuration.
fn prompt_platform_bootstrap() -> Result<Option<BootstrapProviderConfig>> {
    let query_api_url = Text::new("Query API URL:")
        .with_default("http://localhost:8080")
        .with_help_message("URL of the Query API service for bootstrap data")
        .prompt()?;

    Ok(Some(BootstrapProviderConfig::Platform(
        PlatformBootstrapConfigDto {
            query_api_url: Some(query_api_url),
            timeout_seconds: 300,
        },
    )))
}

/// Prompt for reaction selection and configuration.
pub fn prompt_reactions(sources: &[SourceConfig]) -> Result<Vec<ReactionConfig>> {
    println!("Reactions");
    println!("---------");
    println!("Select how you want to receive query results.");
    println!();

    let reaction_types = vec![
        ReactionType::Log,
        ReactionType::Sse,
        ReactionType::Http,
        ReactionType::Grpc,
        ReactionType::Platform,
    ];

    let selected = MultiSelect::new(
        "Select reactions (space to select, enter to confirm):",
        reaction_types,
    )
    .with_help_message("Use arrow keys to navigate, space to select/deselect")
    .prompt()?;

    if selected.is_empty() {
        println!("No reactions selected. You can add reactions later by editing the config file.");
        println!();
        return Ok(Vec::new());
    }

    // Collect source IDs for query placeholder
    let source_ids: Vec<String> = sources.iter().map(|s| s.id().to_string()).collect();

    let mut reactions = Vec::new();

    for reaction_type in selected {
        println!();
        let reaction = prompt_reaction_details(reaction_type, &source_ids)?;
        reactions.push(reaction);
    }

    println!();
    Ok(reactions)
}

/// Prompt for details of a specific reaction type.
fn prompt_reaction_details(
    reaction_type: ReactionType,
    _source_ids: &[String],
) -> Result<ReactionConfig> {
    match reaction_type {
        ReactionType::Log => prompt_log_reaction(),
        ReactionType::Http => prompt_http_reaction(),
        ReactionType::Sse => prompt_sse_reaction(),
        ReactionType::Grpc => prompt_grpc_reaction(),
        ReactionType::Platform => prompt_platform_reaction(),
    }
}

/// Prompt for Log reaction configuration.
fn prompt_log_reaction() -> Result<ReactionConfig> {
    println!("Configuring Log Reaction");
    println!("------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("log-reaction")
        .prompt()?;

    Ok(ReactionConfig::Log {
        id,
        queries: vec!["my-query".to_string()], // Placeholder - user needs to edit
        auto_start: true,
        config: LogReactionConfigDto::default(),
    })
}

/// Prompt for HTTP reaction configuration.
fn prompt_http_reaction() -> Result<ReactionConfig> {
    println!("Configuring HTTP Webhook Reaction");
    println!("----------------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("http-reaction")
        .prompt()?;

    let base_url = Text::new("Webhook base URL:")
        .with_default("http://localhost:9000")
        .with_help_message("URL to POST query results to")
        .prompt()?;

    Ok(ReactionConfig::Http {
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        config: HttpReactionConfigDto {
            base_url: ConfigValue::Static(base_url),
            token: None,
            timeout_ms: ConfigValue::Static(5000),
            routes: std::collections::HashMap::new(),
        },
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

    let port_str = Text::new("SSE server port:")
        .with_default("8081")
        .with_help_message("Port for SSE endpoint")
        .prompt()?;
    let port: u16 = port_str.parse().unwrap_or(8081);

    Ok(ReactionConfig::Sse {
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        config: SseReactionConfigDto {
            host: ConfigValue::Static(host),
            port: ConfigValue::Static(port),
            sse_path: ConfigValue::Static("/events".to_string()),
            heartbeat_interval_ms: ConfigValue::Static(30000),
            routes: std::collections::HashMap::new(),
            default_template: None,
        },
    })
}

/// Prompt for gRPC reaction configuration.
fn prompt_grpc_reaction() -> Result<ReactionConfig> {
    println!("Configuring gRPC Reaction");
    println!("-------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("grpc-reaction")
        .prompt()?;

    let endpoint = Text::new("gRPC endpoint URL:")
        .with_default("grpc://localhost:50052")
        .with_help_message("Endpoint for gRPC streaming")
        .prompt()?;

    Ok(ReactionConfig::Grpc {
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        config: GrpcReactionConfigDto {
            endpoint: ConfigValue::Static(endpoint),
            timeout_ms: ConfigValue::Static(5000),
            batch_size: ConfigValue::Static(100),
            batch_flush_timeout_ms: ConfigValue::Static(1000),
            max_retries: ConfigValue::Static(3),
            connection_retry_attempts: ConfigValue::Static(5),
            initial_connection_timeout_ms: ConfigValue::Static(10000),
            metadata: std::collections::HashMap::new(),
        },
    })
}

/// Prompt for Platform reaction configuration.
fn prompt_platform_reaction() -> Result<ReactionConfig> {
    println!("Configuring Platform Reaction");
    println!("-----------------------------");

    let id = Text::new("Reaction ID:")
        .with_default("platform-reaction")
        .prompt()?;

    let redis_url = Text::new("Redis URL:")
        .with_default("redis://localhost:6379")
        .with_help_message("Redis connection for publishing results")
        .prompt()?;

    Ok(ReactionConfig::Platform {
        id,
        queries: vec!["my-query".to_string()],
        auto_start: true,
        config: PlatformReactionConfigDto {
            redis_url: ConfigValue::Static(redis_url),
            pubsub_name: None,
            source_name: None,
            max_stream_length: None,
            emit_control_events: ConfigValue::Static(false),
            batch_enabled: ConfigValue::Static(false),
            batch_max_size: ConfigValue::Static(100),
            batch_max_wait_ms: ConfigValue::Static(100),
        },
    })
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
        };

        assert_eq!(settings.host, "127.0.0.1");
        assert_eq!(settings.port, 9090);
        assert_eq!(settings.log_level, "debug");
        assert!(settings.persist_index);
        assert!(settings.state_store.is_none());
    }

    #[test]
    fn test_server_settings_default_values() {
        let settings = ServerSettings {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            persist_index: false,
            state_store: None,
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
        };

        assert!(settings.state_store.is_some());
        assert_eq!(settings.state_store.as_ref().unwrap().kind(), "redb");
    }

    // ==================== SourceType enum tests ====================

    #[test]
    fn test_source_type_display_postgres() {
        let source_type = SourceType::Postgres;
        let display = source_type.to_string();
        assert!(display.contains("PostgreSQL"));
        assert!(display.contains("CDC"));
    }

    #[test]
    fn test_source_type_display_http() {
        let source_type = SourceType::Http;
        let display = source_type.to_string();
        assert!(display.contains("HTTP"));
        assert!(display.contains("endpoint"));
    }

    #[test]
    fn test_source_type_display_grpc() {
        let source_type = SourceType::Grpc;
        let display = source_type.to_string();
        assert!(display.contains("gRPC"));
    }

    #[test]
    fn test_source_type_display_mock() {
        let source_type = SourceType::Mock;
        let display = source_type.to_string();
        assert!(display.contains("Mock"));
        assert!(display.contains("test"));
    }

    #[test]
    fn test_source_type_display_platform() {
        let source_type = SourceType::Platform;
        let display = source_type.to_string();
        assert!(display.contains("Platform"));
        assert!(display.contains("Redis"));
    }

    #[test]
    fn test_source_type_equality() {
        assert_eq!(SourceType::Postgres, SourceType::Postgres);
        assert_ne!(SourceType::Postgres, SourceType::Http);
        assert_ne!(SourceType::Mock, SourceType::Grpc);
    }

    #[test]
    fn test_source_type_clone() {
        let original = SourceType::Http;
        let cloned = original;
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_source_type_debug() {
        let source_type = SourceType::Postgres;
        let debug = format!("{source_type:?}");
        assert_eq!(debug, "Postgres");
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
    fn test_bootstrap_type_display_platform() {
        let bootstrap_type = BootstrapType::Platform;
        let display = bootstrap_type.to_string();
        assert!(display.contains("Platform"));
        assert!(display.contains("Redis"));
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

    // ==================== ReactionType enum tests ====================

    #[test]
    fn test_reaction_type_display_log() {
        let reaction_type = ReactionType::Log;
        let display = reaction_type.to_string();
        assert!(display.contains("Log"));
        assert!(display.contains("console"));
    }

    #[test]
    fn test_reaction_type_display_http() {
        let reaction_type = ReactionType::Http;
        let display = reaction_type.to_string();
        assert!(display.contains("HTTP"));
        assert!(display.contains("Webhook"));
    }

    #[test]
    fn test_reaction_type_display_sse() {
        let reaction_type = ReactionType::Sse;
        let display = reaction_type.to_string();
        assert!(display.contains("SSE"));
        assert!(display.contains("Server-Sent Events"));
    }

    #[test]
    fn test_reaction_type_display_grpc() {
        let reaction_type = ReactionType::Grpc;
        let display = reaction_type.to_string();
        assert!(display.contains("gRPC"));
    }

    #[test]
    fn test_reaction_type_display_platform() {
        let reaction_type = ReactionType::Platform;
        let display = reaction_type.to_string();
        assert!(display.contains("Platform"));
        assert!(display.contains("Drasi"));
    }

    #[test]
    fn test_reaction_type_equality() {
        assert_eq!(ReactionType::Log, ReactionType::Log);
        assert_ne!(ReactionType::Http, ReactionType::Sse);
        assert_ne!(ReactionType::Grpc, ReactionType::Platform);
    }

    #[test]
    fn test_reaction_type_clone() {
        let original = ReactionType::Sse;
        let cloned = original;
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_reaction_type_debug() {
        let reaction_type = ReactionType::Platform;
        let debug = format!("{reaction_type:?}");
        assert_eq!(debug, "Platform");
    }

    // ==================== All enum variants coverage ====================

    #[test]
    fn test_all_source_types_have_display() {
        let source_types = vec![
            SourceType::Postgres,
            SourceType::Http,
            SourceType::Grpc,
            SourceType::Mock,
            SourceType::Platform,
        ];

        for source_type in source_types {
            let display = source_type.to_string();
            assert!(
                !display.is_empty(),
                "SourceType {source_type:?} has empty display"
            );
        }
    }

    #[test]
    fn test_all_bootstrap_types_have_display() {
        let bootstrap_types = vec![
            BootstrapType::None,
            BootstrapType::Postgres,
            BootstrapType::ScriptFile,
            BootstrapType::Platform,
        ];

        for bootstrap_type in bootstrap_types {
            let display = bootstrap_type.to_string();
            assert!(
                !display.is_empty(),
                "BootstrapType {bootstrap_type:?} has empty display"
            );
        }
    }

    #[test]
    fn test_all_reaction_types_have_display() {
        let reaction_types = vec![
            ReactionType::Log,
            ReactionType::Http,
            ReactionType::Sse,
            ReactionType::Grpc,
            ReactionType::Platform,
        ];

        for reaction_type in reaction_types {
            let display = reaction_type.to_string();
            assert!(
                !display.is_empty(),
                "ReactionType {reaction_type:?} has empty display"
            );
        }
    }

    // ==================== Display descriptions are helpful ====================

    #[test]
    fn test_source_type_displays_are_descriptive() {
        // Each display should contain a description, not just the type name
        assert!(SourceType::Postgres.to_string().len() > 15);
        assert!(SourceType::Http.to_string().len() > 15);
        assert!(SourceType::Grpc.to_string().len() > 15);
        assert!(SourceType::Mock.to_string().len() > 15);
        assert!(SourceType::Platform.to_string().len() > 15);
    }

    #[test]
    fn test_bootstrap_type_displays_are_descriptive() {
        assert!(BootstrapType::None.to_string().len() > 10);
        assert!(BootstrapType::Postgres.to_string().len() > 15);
        assert!(BootstrapType::ScriptFile.to_string().len() > 15);
        assert!(BootstrapType::Platform.to_string().len() > 15);
    }

    #[test]
    fn test_reaction_type_displays_are_descriptive() {
        assert!(ReactionType::Log.to_string().len() > 15);
        assert!(ReactionType::Http.to_string().len() > 15);
        assert!(ReactionType::Sse.to_string().len() > 15);
        assert!(ReactionType::Grpc.to_string().len() > 15);
        assert!(ReactionType::Platform.to_string().len() > 15);
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
}
