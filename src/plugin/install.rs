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

use drasi_server::load_config_file;
use drasi_server::plugin_lockfile::{LockedPlugin, PluginLockfile, PluginSignatureInfo};

use super::{
    cli_host_version_info, cli_registry_client, get_cli_registry_auth, get_plugin_registry,
    load_trusted_identities,
};
use crate::cli_styles;

/// Install a single plugin from the registry.
pub async fn install_single(
    reference: &str,
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    use drasi_host_sdk::fetcher::{parse_source_type, PluginSourceType};

    match parse_source_type(reference) {
        PluginSourceType::File | PluginSourceType::Http => {
            install_from_uri(reference, plugins_dir).await
        }
        PluginSourceType::Oci => {
            install_from_oci(reference, plugins_dir, config_path, registry_override).await
        }
    }
}

/// Install a plugin from a file:// or http(s):// URI.
pub async fn install_from_uri(reference: &str, plugins_dir: &std::path::Path) -> Result<()> {
    use drasi_host_sdk::fetcher::{
        fetch_from_file, fetch_from_http, parse_source_type, read_plugin_metadata, PluginSourceType,
    };

    let source_type = parse_source_type(reference);
    let sp = cli_styles::spinner(&format!("Fetching plugin from {reference}"));

    let fetched = match source_type {
        PluginSourceType::File => fetch_from_file(reference, plugins_dir).await?,
        PluginSourceType::Http => fetch_from_http(reference, plugins_dir).await?,
        PluginSourceType::Oci => unreachable!(),
    };

    sp.finish_and_clear();

    // Read metadata from the binary to populate the lockfile
    let metadata = read_plugin_metadata(&fetched.path).unwrap_or_default();

    let mut lockfile = PluginLockfile::read(plugins_dir)?.unwrap_or_default();
    lockfile.insert(
        reference.to_string(),
        LockedPlugin {
            reference: reference.to_string(),
            version: metadata.plugin_version.clone(),
            digest: String::new(),
            sdk_version: metadata.sdk_version.clone(),
            core_version: metadata.core_version.clone(),
            lib_version: metadata.lib_version.clone(),
            platform: metadata.target_triple.clone(),
            filename: fetched.filename,
            file_hash: drasi_server::plugin_lockfile::compute_file_hash(&fetched.path).ok(),
            git_commit: Some(metadata.git_commit.clone()).filter(|s| !s.is_empty()),
            build_timestamp: Some(metadata.build_timestamp.clone()).filter(|s| !s.is_empty()),
            signature: None,
        },
    );
    lockfile.write(plugins_dir)?;

    let ver_info = if !metadata.plugin_version.is_empty() {
        format!(" v{}", metadata.plugin_version)
    } else {
        String::new()
    };
    println!(
        "{}",
        cli_styles::success(&format!(
            "Installed{} → {}",
            ver_info,
            cli_styles::path(&fetched.path.display().to_string())
        ))
    );
    Ok(())
}

/// Install a plugin from an OCI registry.
async fn install_from_oci(
    reference: &str,
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    use drasi_host_sdk::registry::{PluginResolver, RegistryConfig};

    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };

    let client = cli_registry_client(config);
    let host_info = cli_host_version_info();
    let resolver = PluginResolver::new(&client, &host_info);

    let sp = cli_styles::spinner(&format!("Resolving {reference} from {registry_url}..."));

    let resolved = resolver.resolve(reference, &registry_url).await?;

    sp.finish_and_clear();
    println!(
        "{}",
        cli_styles::success(&format!(
            "Resolved {} {} ({})",
            reference,
            cli_styles::version(&resolved.version),
            resolved.platform
        ))
    );

    let sp = cli_styles::spinner(&format!("Downloading {}...", resolved.filename));

    std::fs::create_dir_all(plugins_dir)?;
    let download = client
        .download_plugin(&resolved.reference, plugins_dir, &resolved.filename)
        .await?;

    sp.finish_and_clear();
    println!(
        "{}",
        cli_styles::success(&format!(
            "Installed → {}",
            cli_styles::path(&plugins_dir.join(&resolved.filename).display().to_string())
        ))
    );

    // Update lockfile
    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();
    let sig_info = match &download.verification {
        drasi_host_sdk::registry::SignatureStatus::Verified(v) => Some(PluginSignatureInfo {
            verified: true,
            issuer: v.issuer.clone(),
            subject: v.subject.clone(),
        }),
        _ => None,
    };
    // Display signature status with trust check
    let trusted = load_trusted_identities(config_path);
    println!(
        "  {}",
        cli_styles::sig_status_from_result(&download.verification, &trusted)
    );
    lockfile.insert(
        reference.to_string(),
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
    lockfile.write(lockfile_dir)?;

    Ok(())
}

/// Install all plugins from the config file.
pub async fn install_from_config(
    config_path: &std::path::Path,
    plugins_dir: &std::path::Path,
    registry_override: Option<&str>,
    locked: bool,
) -> Result<()> {
    let config = load_config_file(config_path)?;

    if config.plugins.is_empty() {
        println!(
            "{}",
            cli_styles::skip("No plugins declared in config file.")
        );
        return Ok(());
    }

    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();

    if locked && lockfile.plugins.is_empty() {
        println!(
            "{}",
            cli_styles::error("--locked flag used but no plugins.lock file found")
        );
        std::process::exit(1);
    }

    let mut installed = 0;
    let mut existing = 0;
    let mut failed = 0;

    if locked {
        cli_styles::section(&format!(
            "Installing {} plugin(s) from lockfile",
            config.plugins.len()
        ));

        for dep in &config.plugins {
            let locked_entry = match lockfile.get(&dep.reference) {
                Some(entry) => entry.clone(),
                None => {
                    println!(
                        "{}",
                        cli_styles::error(&format!(
                            "{} — not found in plugins.lock (required by --locked)",
                            dep.reference
                        ))
                    );
                    failed += 1;
                    continue;
                }
            };

            let dest_path = plugins_dir.join(&locked_entry.filename);
            if dest_path.exists() {
                println!(
                    "{}",
                    cli_styles::success(&format!(
                        "{} {} — already installed",
                        dep.reference,
                        cli_styles::version(&locked_entry.version)
                    ))
                );
                existing += 1;
                continue;
            }

            let registry_url = registry_override
                .map(|s| s.to_string())
                .or_else(|| config.plugin_registry.clone())
                .unwrap_or_else(|| "ghcr.io/drasi-project".to_string());

            let auth = get_cli_registry_auth();
            let reg_config = drasi_host_sdk::registry::RegistryConfig {
                default_registry: registry_url,
                auth,
            };
            let client = cli_registry_client(reg_config);

            let sp = cli_styles::spinner(&format!(
                "Downloading {} {}...",
                dep.reference, locked_entry.version
            ));

            std::fs::create_dir_all(plugins_dir)?;
            match client
                .download_plugin(&locked_entry.reference, plugins_dir, &locked_entry.filename)
                .await
            {
                Ok(_download) => {
                    sp.finish_and_clear();
                    let trusted = load_trusted_identities(config_path);
                    let sig_label =
                        cli_styles::sig_status_from_result(&_download.verification, &trusted);
                    println!(
                        "{}  {}",
                        cli_styles::success(&format!(
                            "{} {} → {}",
                            dep.reference,
                            cli_styles::version(&locked_entry.version),
                            cli_styles::path(&locked_entry.filename)
                        )),
                        sig_label
                    );
                    installed += 1;
                }
                Err(e) => {
                    sp.finish_and_clear();
                    println!(
                        "{}",
                        cli_styles::error(&format!("{} — {}", dep.reference, e))
                    );
                    failed += 1;
                }
            }
        }
    } else {
        let registry_url = registry_override
            .map(|s| s.to_string())
            .or_else(|| config.plugin_registry.clone())
            .unwrap_or_else(|| "ghcr.io/drasi-project".to_string());

        cli_styles::section(&format!(
            "Installing {} plugin(s) from config",
            config.plugins.len()
        ));

        let auth = get_cli_registry_auth();
        let reg_config = drasi_host_sdk::registry::RegistryConfig {
            default_registry: registry_url.clone(),
            auth,
        };
        let client = cli_registry_client(reg_config);
        let host_info = cli_host_version_info();
        let resolver = drasi_host_sdk::registry::PluginResolver::new(&client, &host_info);

        for dep in &config.plugins {
            let source_type = drasi_host_sdk::fetcher::parse_source_type(&dep.reference);
            match source_type {
                drasi_host_sdk::fetcher::PluginSourceType::File
                | drasi_host_sdk::fetcher::PluginSourceType::Http => {
                    match install_from_uri(&dep.reference, plugins_dir).await {
                        Ok(()) => {
                            installed += 1;
                        }
                        Err(e) => {
                            println!(
                                "{}",
                                cli_styles::error(&format!("{} — {}", dep.reference, e))
                            );
                            failed += 1;
                        }
                    }
                }
                drasi_host_sdk::fetcher::PluginSourceType::Oci => {
                    let sp = cli_styles::spinner(&format!("Resolving {}...", dep.reference));
                    match resolver.resolve(&dep.reference, &registry_url).await {
                        Ok(resolved) => {
                            sp.finish_and_clear();
                            let dest_path = plugins_dir.join(&resolved.filename);
                            let mut sig_info = None;
                            if dest_path.exists() {
                                println!(
                                    "{}",
                                    cli_styles::success(&format!(
                                        "{} {} — already installed",
                                        dep.reference,
                                        cli_styles::version(&resolved.version)
                                    ))
                                );
                                existing += 1;
                            } else {
                                let sp = cli_styles::spinner(&format!(
                                    "Downloading {} {}...",
                                    dep.reference, resolved.version
                                ));
                                std::fs::create_dir_all(plugins_dir)?;
                                match client
                                    .download_plugin(
                                        &resolved.reference,
                                        plugins_dir,
                                        &resolved.filename,
                                    )
                                    .await
                                {
                                    Ok(_download) => {
                                        sp.finish_and_clear();
                                        let trusted = load_trusted_identities(config_path);
                                        let sig_label = cli_styles::sig_status_from_result(
                                            &_download.verification,
                                            &trusted,
                                        );
                                        println!(
                                            "{}  {}",
                                            cli_styles::success(&format!(
                                                "{} {} → {}",
                                                dep.reference,
                                                cli_styles::version(&resolved.version),
                                                cli_styles::path(&resolved.filename)
                                            )),
                                            sig_label
                                        );
                                        if let drasi_host_sdk::registry::SignatureStatus::Verified(
                                            v,
                                        ) = _download.verification
                                        {
                                            sig_info = Some(PluginSignatureInfo {
                                                verified: true,
                                                issuer: v.issuer,
                                                subject: v.subject,
                                            });
                                        }
                                        installed += 1;
                                    }
                                    Err(e) => {
                                        sp.finish_and_clear();
                                        println!(
                                            "{}",
                                            cli_styles::error(&format!(
                                                "{} — {}",
                                                dep.reference, e
                                            ))
                                        );
                                        failed += 1;
                                        continue;
                                    }
                                }
                            }

                            lockfile.insert(
                                dep.reference.clone(),
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
                        }
                        Err(e) => {
                            sp.finish_and_clear();
                            println!(
                                "{}",
                                cli_styles::error(&format!("{} — {}", dep.reference, e))
                            );
                            failed += 1;
                        }
                    }
                }
            }
        }

        lockfile.write(lockfile_dir)?;
    }

    cli_styles::install_summary(installed, existing, failed);
    Ok(())
}

/// Install all plugins from the registry's plugin directory.
pub async fn install_all(
    plugins_dir: &std::path::Path,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    use drasi_host_sdk::registry::{PluginResolver, RegistryConfig};
    use drasi_server::plugin_lockfile::LockedPlugin;

    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };

    let client = cli_registry_client(config);
    let host_info = cli_host_version_info();
    let resolver = PluginResolver::new(&client, &host_info);

    let sp = cli_styles::spinner(&format!("Discovering plugins from {registry_url}..."));

    let results = client.search_plugins("*").await?;
    sp.finish_and_clear();

    if results.is_empty() {
        println!("{}", cli_styles::skip("No plugins found in the directory."));
        return Ok(());
    }

    cli_styles::section(&format!(
        "Installing {} plugin(s) from registry",
        results.len()
    ));

    std::fs::create_dir_all(plugins_dir)?;
    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut fail_count = 0;

    for result in &results {
        let reference = &result.reference;
        let sp = cli_styles::spinner(&format!("Resolving {reference}..."));
        match resolver.resolve(reference, &registry_url).await {
            Ok(resolved) => {
                sp.finish_and_clear();
                let dest_path = plugins_dir.join(&resolved.filename);
                let mut sig_info = None;
                if dest_path.exists() {
                    println!(
                        "{}",
                        cli_styles::success(&format!(
                            "{} {} — already installed",
                            reference,
                            cli_styles::version(&resolved.version)
                        ))
                    );
                    skip_count += 1;
                } else {
                    let sp = cli_styles::spinner(&format!(
                        "Downloading {} {}...",
                        reference, resolved.version
                    ));
                    match client
                        .download_plugin(&resolved.reference, plugins_dir, &resolved.filename)
                        .await
                    {
                        Ok(_download) => {
                            sp.finish_and_clear();
                            let trusted = load_trusted_identities(config_path);
                            let sig_label = cli_styles::sig_status_from_result(
                                &_download.verification,
                                &trusted,
                            );
                            println!(
                                "{}  {}",
                                cli_styles::success(&format!(
                                    "{} {} → {}",
                                    reference,
                                    cli_styles::version(&resolved.version),
                                    cli_styles::path(&resolved.filename)
                                )),
                                sig_label
                            );
                            if let drasi_host_sdk::registry::SignatureStatus::Verified(v) =
                                _download.verification
                            {
                                sig_info = Some(PluginSignatureInfo {
                                    verified: true,
                                    issuer: v.issuer,
                                    subject: v.subject,
                                });
                            }
                            success_count += 1;
                        }
                        Err(e) => {
                            sp.finish_and_clear();
                            println!("{}", cli_styles::error(&format!("{reference} — {e}")));
                            fail_count += 1;
                            continue;
                        }
                    }
                }

                lockfile.insert(
                    reference.clone(),
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
            }
            Err(e) => {
                sp.finish_and_clear();
                println!("{}", cli_styles::error(&format!("{reference} — {e}")));
                fail_count += 1;
            }
        }
    }

    lockfile.write(lockfile_dir)?;
    cli_styles::install_summary(success_count, skip_count, fail_count);

    if fail_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}
