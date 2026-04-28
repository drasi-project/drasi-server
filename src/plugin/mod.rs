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

mod install;
mod list;
mod remove;
mod search;
mod upgrade;

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use drasi_lib::get_or_init_global_registry;
use drasi_server::plugin_operations::PluginOperations;

use crate::cli_styles;

#[derive(Subcommand)]
pub enum PluginAction {
    /// Install a plugin from an OCI registry, local file, or HTTP URL
    Install {
        /// Plugin reference: OCI (e.g., "source/postgres:0.1.8"),
        /// OCI wildcard pattern (e.g., "source/*" or "*/postgres"),
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
        /// Plugin filename, kind, or wildcard pattern
        /// (e.g., "libdrasi_source_postgres.so", "source/postgres", "source/*", "*/postgres")
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

/// Handle plugin subcommands.
pub async fn run_plugin_command(
    action: PluginAction,
    config_path: PathBuf,
    plugins_dir_override: Option<PathBuf>,
) -> Result<()> {
    // Initialize logging for CLI commands — suppress noisy oci_client warnings
    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var("RUST_LOG", "warn,oci_client=error");
        }
    }
    get_or_init_global_registry();

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
                install::install_from_config(
                    &config_path,
                    &plugins_dir,
                    registry.as_deref(),
                    locked,
                )
                .await
            } else if let Some(ref_str) = reference {
                install::install_single(&ref_str, &plugins_dir, &config_path, registry.as_deref())
                    .await
            } else {
                println!(
                    "{}",
                    cli_styles::error("provide a plugin reference or --from-config")
                );
                std::process::exit(1);
            }
        }
        PluginAction::List => list::list(&plugins_dir, &config_path),
        PluginAction::Search {
            reference,
            registry,
        } => search::search(&reference, &config_path, registry.as_deref()).await,
        PluginAction::Remove { reference } => remove::remove(&reference, &plugins_dir),
        PluginAction::InstallAll { registry } => {
            install::install_all(&plugins_dir, &config_path, registry.as_deref()).await
        }
        PluginAction::Upgrade {
            reference,
            all,
            registry,
            dry_run,
        } => {
            upgrade::upgrade(
                &plugins_dir,
                &config_path,
                reference.as_deref(),
                all,
                registry.as_deref(),
                dry_run,
            )
            .await
        }
    }
}

// ── Shared helpers (delegates to PluginOperations) ──────────────────────────

/// Get plugin registry URL from config or override.
pub(crate) fn get_plugin_registry(
    config_path: &std::path::Path,
    override_registry: Option<&str>,
) -> String {
    PluginOperations::resolve_registry(config_path, override_registry)
}

/// Get registry auth from environment for CLI commands.
pub(crate) fn get_cli_registry_auth() -> drasi_host_sdk::registry::RegistryAuth {
    PluginOperations::registry_auth()
}

/// Build host version info for CLI commands.
pub(crate) fn cli_host_version_info() -> drasi_host_sdk::registry::HostVersionInfo {
    PluginOperations::host_version_info()
}

/// Create an OCI registry client with best-effort signature verification.
pub(crate) fn cli_registry_client(
    config: drasi_host_sdk::registry::RegistryConfig,
) -> drasi_host_sdk::registry::OciRegistryClient {
    PluginOperations::build_registry_client(config)
}

/// Load trusted identities from the server config file.
pub(crate) fn load_trusted_identities(
    config_path: &std::path::Path,
) -> Vec<drasi_host_sdk::registry::TrustedIdentity> {
    PluginOperations::load_trusted_identities(config_path)
}
