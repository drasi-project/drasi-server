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

use drasi_host_sdk::registry::{PluginResolver, RegistryConfig};
use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile, PluginSignatureInfo};

use super::{
    cli_host_version_info, cli_registry_client, get_cli_registry_auth, get_plugin_registry,
    load_trusted_identities,
};
use crate::cli_styles;

/// Upgrade installed plugins to newer compatible versions.
pub async fn upgrade(
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    reference: Option<&str>,
    all: bool,
    registry_override: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    if reference.is_none() && !all {
        println!(
            "{}",
            cli_styles::error("provide a plugin reference or --all")
        );
        std::process::exit(1);
    }

    let lockfile = PluginLockfile::read(plugins_dir)?;
    let lockfile = match lockfile {
        Some(lf) if !lf.is_empty() => lf,
        _ => {
            println!(
                "{}",
                cli_styles::error("No plugins.lock found or no plugins installed.")
            );
            println!(
                "{}",
                cli_styles::detail("Use 'plugin install' to install plugins first.")
            );
            std::process::exit(1);
        }
    };

    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };
    let client = cli_registry_client(config);
    let host_info = cli_host_version_info();
    let resolver = PluginResolver::new(&client, &host_info);

    // Determine which plugins to upgrade
    let to_check: Vec<(String, LockedPlugin)> = if all {
        lockfile
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    } else {
        let ref_str = reference.expect("reference must be provided when not listing all");
        match lockfile.get(ref_str) {
            Some(entry) => vec![(ref_str.to_string(), entry.clone())],
            None => {
                let matches: Vec<_> = lockfile
                    .iter()
                    .filter(|(k, _)| k.contains(ref_str))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if matches.is_empty() {
                    println!(
                        "{}",
                        cli_styles::error(&format!("Plugin '{ref_str}' not found in lockfile."))
                    );
                    println!("{}", cli_styles::detail("Installed plugins:"));
                    for key in lockfile.keys() {
                        println!("  {}", cli_styles::heading(key));
                    }
                    std::process::exit(1);
                }
                matches
            }
        }
    };

    let title = if dry_run {
        "Upgrade Check (dry run)"
    } else {
        "Upgrade Summary"
    };
    cli_styles::section(title);

    let mut upgraded = 0;
    let mut up_to_date = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let mut new_lockfile = lockfile.clone();

    for (ref_key, current) in &to_check {
        // Skip non-OCI plugins
        if ref_key.starts_with("file://")
            || ref_key.starts_with("http://")
            || ref_key.starts_with("https://")
        {
            println!(
                "{}",
                cli_styles::skip(&format!("{ref_key} — non-OCI source"))
            );
            skipped += 1;
            continue;
        }

        // Strip the tag/digest suffix to get the base image reference.
        // OCI refs can be: registry:port/repo:tag or registry:port/repo@sha256:...
        // We strip only after the last `/` to avoid truncating port numbers.
        let base_ref = if let Some(at_pos) = ref_key.rfind('@') {
            &ref_key[..at_pos]
        } else if let Some(slash_pos) = ref_key.rfind('/') {
            if let Some(colon_pos) = ref_key[slash_pos..].find(':') {
                &ref_key[..slash_pos + colon_pos]
            } else {
                ref_key
            }
        } else if let Some(colon_pos) = ref_key.find(':') {
            &ref_key[..colon_pos]
        } else {
            ref_key
        };
        let sp = cli_styles::spinner(&format!("Checking {ref_key}..."));

        match resolver.resolve(base_ref, &registry_url).await {
            Ok(resolved) => {
                sp.finish_and_clear();
                if resolved.digest == current.digest {
                    println!(
                        "{}",
                        cli_styles::success(&format!(
                            "{} — up to date ({})",
                            ref_key,
                            cli_styles::version(&current.version)
                        ))
                    );
                    up_to_date += 1;
                } else if !dry_run {
                    let sp = cli_styles::spinner(&format!(
                        "Downloading {} {}...",
                        ref_key, resolved.version
                    ));
                    match client
                        .download_plugin(&resolved.reference, plugins_dir, &resolved.filename)
                        .await
                    {
                        Ok(download) => {
                            sp.finish_and_clear();
                            let trusted = load_trusted_identities(config_path);
                            let sig_label = cli_styles::sig_status_from_result(
                                &download.verification,
                                &trusted,
                            );
                            println!(
                                "{}  {}",
                                cli_styles::success(&format!(
                                    "{} {}",
                                    ref_key,
                                    cli_styles::version_upgrade(
                                        &current.version,
                                        &resolved.version
                                    )
                                )),
                                sig_label
                            );
                            let sig_info = match &download.verification {
                                drasi_host_sdk::registry::SignatureStatus::Verified(v) => {
                                    Some(PluginSignatureInfo {
                                        verified: true,
                                        issuer: v.issuer.clone(),
                                        subject: v.subject.clone(),
                                    })
                                }
                                _ => None,
                            };
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
                                    filename: resolved.filename.clone(),
                                    file_hash: drasi_server::plugin_lockfile::compute_file_hash(
                                        &plugins_dir.join(&resolved.filename),
                                    )
                                    .ok(),
                                    git_commit: None,
                                    build_timestamp: None,
                                    signature: sig_info,
                                },
                            );
                            upgraded += 1;
                        }
                        Err(e) => {
                            sp.finish_and_clear();
                            println!(
                                "{}",
                                cli_styles::error(&format!("{ref_key} — download failed: {e}"))
                            );
                            failed += 1;
                        }
                    }
                } else {
                    println!(
                        "{}",
                        cli_styles::download(&format!(
                            "{} {} (available)",
                            ref_key,
                            cli_styles::version_upgrade(&current.version, &resolved.version)
                        ))
                    );
                    upgraded += 1;
                }
            }
            Err(e) => {
                sp.finish_and_clear();
                println!(
                    "{}",
                    cli_styles::error(&format!("{ref_key} — resolve failed: {e}"))
                );
                failed += 1;
            }
        }
    }

    if !dry_run && upgraded > 0 {
        new_lockfile.write(plugins_dir)?;
    }

    cli_styles::summary(upgraded, up_to_date, skipped, failed);

    Ok(())
}
