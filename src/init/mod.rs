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

//! Interactive configuration initialization module.
//!
//! This module provides an interactive questionnaire for creating Drasi Server
//! configuration files. Users can select sources, bootstrap providers, and
//! reactions through a series of prompts.

// Allow println! in init module for CLI user-facing output
#![allow(clippy::print_stdout)]

mod builder;
mod prompts;

use anyhow::Result;
use inquire::{Confirm, Text};
use std::fs;
use std::path::PathBuf;

/// Resolve the default plugins directory (next to the current executable).
fn default_plugins_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("plugins")))
}

/// Run the interactive configuration initialization.
///
/// This function guides the user through selecting:
/// 1. Server settings (host, port, log level, hot-reload)
/// 2. Data sources (discovered plugins or built-in list)
/// 3. Bootstrap providers for each source
/// 4. Reactions (discovered plugins or built-in list)
///
/// When a `plugins_dir` is provided (or defaulted), the wizard scans for
/// locally installed plugins and presents them as selection options. If no
/// plugins are found, the wizard falls back to the hardcoded built-in list.
///
/// The resulting configuration is written to the specified output file.
pub async fn run_init(
    output_path: PathBuf,
    force: bool,
    plugins_dir: Option<PathBuf>,
) -> Result<()> {
    // Check if file already exists
    if output_path.exists() && !force {
        println!(
            "Configuration file already exists: {}",
            output_path.display()
        );
        println!("Use --force to overwrite.");
        std::process::exit(1);
    }

    println!();
    println!("Welcome to Drasi Server Configuration!");
    println!("======================================");
    println!();
    println!("This wizard will help you create a configuration file.");
    println!();

    // Discover available plugins
    let effective_plugins_dir = plugins_dir.or_else(default_plugins_dir);
    let mut discovered = if let Some(dir) = &effective_plugins_dir {
        let d = prompts::discover_available_plugins(dir);
        if !d.is_empty() {
            println!(
                "Discovered {} source(s), {} reaction(s), {} bootstrapper(s) in {}",
                d.sources.len(),
                d.reactions.len(),
                d.bootstrappers.len(),
                dir.display()
            );
            println!();
        }
        d
    } else {
        prompts::DiscoveredPlugins::default()
    };

    // Offer to download additional plugins from registry
    if let Some(dir) = &effective_plugins_dir {
        loop {
            let download_more = Confirm::new("Download additional plugins from a registry?")
                .with_default(false)
                .prompt()?;

            if !download_more {
                break;
            }

            let registry_url = Text::new("Registry URL:")
                .with_default("ghcr.io/drasi-project")
                .prompt()?;

            println!("Searching {registry_url}...");
            let available = prompts::search_registry_plugins(&registry_url).await?;

            if available.is_empty() {
                println!("No plugins found in registry.");
                continue;
            }

            let selected = prompts::prompt_registry_plugin_selection(&available)?;

            if selected.is_empty() {
                continue;
            }

            for plugin_ref in &selected {
                println!("Installing {plugin_ref}...");
                match prompts::install_registry_plugin(plugin_ref, dir, &registry_url).await {
                    Ok(()) => println!("  \u{2713} Installed {plugin_ref}"),
                    Err(e) => println!("  \u{2717} Failed to install {plugin_ref}: {e}"),
                }
            }

            // Re-discover local plugins after downloads
            discovered = prompts::discover_available_plugins(dir);
            println!();
            println!(
                "Now have {} source(s), {} reaction(s), {} bootstrapper(s)",
                discovered.sources.len(),
                discovered.reactions.len(),
                discovered.bootstrappers.len()
            );
        }
    }

    // Step 1: Server settings
    let server_settings = prompts::prompt_server_settings()?;

    // Step 2: Select and configure sources (dynamic or fallback)
    let sources = prompts::prompt_sources_dynamic(&discovered)?;

    // Step 3: Select and configure reactions (dynamic or fallback)
    let reactions = prompts::prompt_reactions_dynamic(&discovered, &sources)?;

    // Build the configuration
    let config = builder::build_config(server_settings, sources, reactions);

    // Create parent directories
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Serialize and write
    let yaml_content = builder::generate_yaml(&config)?;
    fs::write(&output_path, yaml_content)?;

    println!();
    println!("Configuration saved to: {}", output_path.display());
    println!();
    println!("Next steps:");
    println!("  1. Review and edit {} as needed", output_path.display());
    println!("  2. Run: drasi-server --config {}", output_path.display());

    Ok(())
}
