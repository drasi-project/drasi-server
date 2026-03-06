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
use std::collections::HashSet;
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

    let is_wildcard = is_wildcard_pattern(reference);
    let mut removed_filenames: Vec<String> = Vec::new();

    if is_wildcard {
        // Wildcard remove: match against filename, lockfile key-like kind (source/postgres),
        // and derived kind from filename.
        for entry in fs::read_dir(plugins_dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_plugin_binary(&name) {
                continue;
            }
            let kind_match = plugin_kind_from_filename(&name)
                .as_deref()
                .is_some_and(|kind| wildcard_match(reference, kind));
            if wildcard_match(reference, &name) || kind_match {
                let path = plugins_dir.join(&name);
                fs::remove_file(&path)?;
                println!("{}", cli_styles::success(&format!("Removed {name}")));
                removed_filenames.push(name);
            }
        }
    } else {
        // Try exact filename first
        let target = plugins_dir.join(reference);
        if target.exists() {
            fs::remove_file(&target)?;
            let removed_name = target
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| reference.to_string());
            println!("{}", cli_styles::success(&format!("Removed {reference}")));
            removed_filenames.push(removed_name);
        }

        // Try matching by type/kind pattern (e.g., "source/postgres")
        if removed_filenames.is_empty() {
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
                        removed_filenames.push(pattern.clone());
                        break;
                    }
                }
            }
        }
    }

    if removed_filenames.is_empty() {
        println!(
            "{}",
            cli_styles::error(&format!("Plugin not found: {reference}"))
        );
        std::process::exit(1);
    }

    // Update lockfile: remove entries by key/pattern/filename.
    let lockfile_dir = plugins_dir;
    if let Ok(Some(mut lockfile)) = PluginLockfile::read(lockfile_dir) {
        let removed_set: HashSet<&str> = removed_filenames.iter().map(String::as_str).collect();
        let mut keys_to_remove: HashSet<String> = lockfile
            .iter()
            .filter_map(|(key, entry)| {
                let kind_match = plugin_kind_from_filename(&entry.filename)
                    .as_deref()
                    .is_some_and(|kind| wildcard_match(reference, kind));
                let wildcard_entry_match = is_wildcard
                    && (wildcard_match(reference, key)
                        || wildcard_match(reference, &entry.filename)
                        || kind_match);
                let direct_key_match = !is_wildcard && key == reference;
                if removed_set.contains(entry.filename.as_str())
                    || direct_key_match
                    || wildcard_entry_match
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        // Keep existing stale-entry cleanup behavior.
        keys_to_remove.extend(lockfile.iter().filter_map(|(key, entry)| {
            if !plugins_dir.join(&entry.filename).exists() {
                Some(key.clone())
            } else {
                None
            }
        }));

        for key in &keys_to_remove {
            lockfile.remove(key);
        }

        if !keys_to_remove.is_empty() {
            let _ = lockfile.write(lockfile_dir);
            println!("{}", cli_styles::detail("Updated plugins.lock"));
        }
    }

    Ok(())
}

fn is_wildcard_pattern(reference: &str) -> bool {
    reference.contains('*') || reference.contains('?') || reference.contains('[')
}

fn is_plugin_binary(name: &str) -> bool {
    name.ends_with(".so") || name.ends_with(".dll") || name.ends_with(".dylib")
}

fn plugin_kind_from_filename(filename: &str) -> Option<String> {
    let stem = if let Some(stem) = filename.strip_suffix(".so") {
        stem.strip_prefix("lib")?
    } else if let Some(stem) = filename.strip_suffix(".dll") {
        stem
    } else if let Some(stem) = filename.strip_suffix(".dylib") {
        stem.strip_prefix("lib")?
    } else {
        return None;
    };

    let stem = stem.strip_prefix("drasi_")?;
    let mut parts = stem.splitn(2, '_');
    let ptype = parts.next()?;
    let kind = parts.next()?.replace('_', "-");
    Some(format!("{ptype}/{kind}"))
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti) = (None::<usize>, 0usize);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::{plugin_kind_from_filename, wildcard_match};

    #[test]
    fn test_plugin_kind_from_filename() {
        assert_eq!(
            plugin_kind_from_filename("libdrasi_source_postgres.so").as_deref(),
            Some("source/postgres")
        );
        assert_eq!(
            plugin_kind_from_filename("drasi_reaction_log.dll").as_deref(),
            Some("reaction/log")
        );
        assert_eq!(
            plugin_kind_from_filename("libdrasi_bootstrap_postgres.dylib").as_deref(),
            Some("bootstrap/postgres")
        );
        assert!(plugin_kind_from_filename("not-a-plugin.txt").is_none());
    }

    #[test]
    fn test_wildcard_match() {
        assert!(wildcard_match("source/*", "source/postgres"));
        assert!(wildcard_match("*/postgres", "source/postgres"));
        assert!(wildcard_match("libdrasi_*", "libdrasi_source_postgres.so"));
        assert!(wildcard_match("source/postgre?", "source/postgres"));
        assert!(!wildcard_match("reaction/*", "source/postgres"));
    }
}
