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
/// 2. Data sources via select-or-install pattern
/// 3. Configuration + bootstrap provider for each source
/// 4. Reactions via select-or-install pattern
/// 5. Configuration for each reaction
///
/// When a `plugins_dir` is provided (or defaulted), the wizard scans for
/// locally installed plugins and presents them as selection options. Users
/// can also install additional plugins from a registry at each step.
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

    let effective_plugins_dir = plugins_dir.or_else(default_plugins_dir);

    // Step 1: Server settings
    let server_settings = prompts::prompt_server_settings()?;

    // Step 2: Source selection
    println!();
    println!("Data Sources");
    println!("------------");
    let source_kinds = prompts::select_or_install_plugins(
        "source",
        effective_plugins_dir.as_deref(),
        server_settings.plugin_registry.as_str(),
    )
    .await?;

    // Step 3: Configure each source + bootstrap
    let mut sources = Vec::new();
    for kind in &source_kinds {
        println!();
        let mut source = prompts::prompt_source_by_kind(kind)?;

        // Bootstrap for this source
        println!();
        println!("Bootstrap provider for source '{}':", source.id());
        let bootstrap_kinds = prompts::select_or_install_plugins(
            "bootstrap",
            effective_plugins_dir.as_deref(),
            server_settings.plugin_registry.as_str(),
        )
        .await?;
        if let Some(boot_kind) = bootstrap_kinds.first() {
            source = prompts::attach_bootstrap_to_source(source, boot_kind)?;
        }
        sources.push(source);
    }

    // Step 4: Reaction selection
    println!();
    println!("Reactions");
    println!("---------");
    let reaction_kinds = prompts::select_or_install_plugins(
        "reaction",
        effective_plugins_dir.as_deref(),
        server_settings.plugin_registry.as_str(),
    )
    .await?;

    // Step 5: Configure each reaction
    let source_ids: Vec<String> = sources.iter().map(|s| s.id().to_string()).collect();
    let mut reactions = Vec::new();
    for kind in &reaction_kinds {
        println!();
        let reaction = prompts::prompt_reaction_by_kind(kind, &source_ids)?;
        reactions.push(reaction);
    }

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
