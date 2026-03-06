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

use drasi_host_sdk::registry::RegistryConfig;

use super::{cli_registry_client, get_cli_registry_auth, get_plugin_registry};
use crate::cli_styles;

/// Search for available versions of a plugin.
pub async fn search(
    reference: &str,
    config_path: &std::path::Path,
    registry_override: Option<&str>,
) -> Result<()> {
    let registry_url = get_plugin_registry(config_path, registry_override);
    let auth = get_cli_registry_auth();
    let config = RegistryConfig {
        default_registry: registry_url.clone(),
        auth,
    };

    let client = cli_registry_client(config);

    let sp = cli_styles::spinner(&format!("Searching for {reference} in {registry_url}..."));

    let results = client.search_plugins(reference).await?;
    sp.finish_and_clear();

    if results.is_empty() {
        println!(
            "{}",
            cli_styles::skip(&format!("No plugins found matching '{reference}'."))
        );
        return Ok(());
    }

    for result in &results {
        println!(
            "\n  {} {}",
            cli_styles::heading(&result.reference),
            cli_styles::detail(&result.full_reference)
        );
        if result.versions.is_empty() {
            println!("{}", cli_styles::detail("No versions found."));
        } else {
            println!("{}", cli_styles::detail("Available versions:"));
            for v in &result.versions {
                println!(
                    "    {}  {}",
                    cli_styles::version(&v.version),
                    cli_styles::detail(&format!("({})", v.platforms.join(", ")))
                );
            }
        }
    }

    Ok(())
}
