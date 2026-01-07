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

/// Run the interactive configuration initialization.
///
/// This function guides the user through selecting:
/// 1. Server settings (host, port, log level)
/// 2. Data sources (PostgreSQL, HTTP, gRPC, Mock, Platform)
/// 3. Bootstrap providers for each source
/// 4. Reactions (Log, HTTP, SSE, gRPC, Platform)
///
/// The resulting configuration is written to the specified output file.
pub fn run_init(output_path: PathBuf, force: bool) -> Result<()> {
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

    // Step 1: Server settings
    let server_settings = prompts::prompt_server_settings()?;

    // Step 2: Select and configure sources
    let sources = prompts::prompt_sources()?;

    // Step 3: Select and configure reactions
    let reactions = prompts::prompt_reactions(&sources)?;

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
