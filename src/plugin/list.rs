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

use anyhow::Result;
use std::fs;

use drasi_server::plugin_lockfile::PluginLockfile;

use super::load_trusted_identities;
use crate::cli_styles;

/// List installed plugins in the plugins directory.
pub fn list(plugins_dir: &std::path::Path, config_path: &std::path::Path) -> Result<()> {
    if !plugins_dir.exists() {
        println!(
            "{}",
            cli_styles::skip(&format!(
                "No plugins directory found: {}",
                plugins_dir.display()
            ))
        );
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
        println!(
            "{}",
            cli_styles::skip(&format!(
                "No plugins installed in {}",
                plugins_dir.display()
            ))
        );
        return Ok(());
    }

    // Load lockfile for metadata
    let lockfile_dir = plugins_dir;
    let lockfile = PluginLockfile::read(lockfile_dir)
        .ok()
        .flatten()
        .unwrap_or_default();

    // Load trusted identities from config
    let trusted = load_trusted_identities(config_path);

    // Compute file integrity for all lockfile entries
    let integrity = lockfile.verify_file_integrity(plugins_dir);

    // Build filename → (key, entry) lookup
    let mut by_filename: std::collections::HashMap<
        &str,
        (&str, &drasi_server::plugin_lockfile::LockedPlugin),
    > = std::collections::HashMap::new();
    for (key, entry) in &lockfile.plugins {
        by_filename.insert(&entry.filename, (key, entry));
    }

    plugins.sort_by(|a, b| a.0.cmp(&b.0));

    cli_styles::section(&format!("Installed plugins ({})", plugins.len()));
    println!(
        "{}",
        cli_styles::detail(&format!("Directory: {}", plugins_dir.display()))
    );
    println!();

    for (name, size) in &plugins {
        let size_mb = *size as f64 / 1_048_576.0;

        if let Some((key, entry)) = by_filename.get(name.as_str()) {
            println!(
                "  {} {}",
                cli_styles::heading(key),
                cli_styles::version(&format!("v{}", entry.version))
            );
            let mut detail = format!(
                "{} ({:.1} MB)  SDK: {}  Platform: {}",
                name, size_mb, entry.sdk_version, entry.platform
            );
            if let Some(ref commit) = entry.git_commit {
                detail.push_str(&format!("  Commit: {commit}"));
            }
            if let Some(ref built) = entry.build_timestamp {
                detail.push_str(&format!("  Built: {built}"));
            }
            let sig_display = cli_styles::sig_status(entry.signature.as_ref(), &trusted);
            println!("{}", cli_styles::detail(&detail));
            println!("    {sig_display}");

            // Show file integrity status
            if let Some(file_status) = integrity.get(name.as_str()) {
                println!("    {}", cli_styles::integrity_status(file_status));
            }
        } else {
            println!(
                "  {} {}",
                cli_styles::heading(name),
                cli_styles::detail(&format!("({size_mb:.1} MB)"))
            );
        }
    }

    Ok(())
}
