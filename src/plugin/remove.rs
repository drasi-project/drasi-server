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

use crate::cli_styles;

/// Remove an installed plugin.
pub fn remove(reference: &str, plugins_dir: &std::path::Path) -> Result<()> {
    if !plugins_dir.exists() {
        println!(
            "{}",
            cli_styles::error(&format!(
                "Plugins directory does not exist: {}",
                plugins_dir.display()
            ))
        );
        std::process::exit(1);
    }

    let mut removed = false;

    // Try exact filename first
    let target = plugins_dir.join(reference);
    if target.exists() {
        fs::remove_file(&target)?;
        println!("{}", cli_styles::success(&format!("Removed {reference}")));
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
                    println!("{}", cli_styles::success(&format!("Removed {pattern}")));
                    removed = true;
                    break;
                }
            }
        }
    }

    if !removed {
        println!(
            "{}",
            cli_styles::error(&format!("Plugin not found: {reference}"))
        );
        std::process::exit(1);
    }

    // Update lockfile: remove the entry
    let lockfile_dir = plugins_dir;
    if let Ok(Some(mut lockfile)) = PluginLockfile::read(lockfile_dir) {
        if lockfile.remove(reference).is_some() {
            let _ = lockfile.write(lockfile_dir);
            println!("{}", cli_styles::detail("Updated plugins.lock"));
        }
    }

    Ok(())
}
