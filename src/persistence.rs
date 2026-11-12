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

use crate::api::mappings::DtoMapper;
use crate::api::models::ConfigValue;
use crate::config::{DrasiLibInstanceConfig, DrasiServerConfig};
use crate::instance_registry::InstanceRegistry;
use anyhow::{Context, Result};
use indexmap::IndexMap;
use log::{debug, info};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub mod configuration_store;

/// Persists Server settings and configuration-store locations. Component
/// definitions, identities and provider references belong to each Core store,
/// including declarations whose constructors have not completed.
pub struct ConfigPersistence {
    config_file_path: PathBuf,
    default_store: crate::managed_configuration::ConfigurationStoreConfig,
    registry: InstanceRegistry,
    server_configuration: DrasiServerConfig,
    save_gate: Arc<Mutex<()>>,
    instance_configs: RwLock<IndexMap<String, DrasiLibInstanceConfig>>,
}

fn capture_instance_settings(
    mut config: DrasiLibInstanceConfig,
) -> Result<(String, DrasiLibInstanceConfig)> {
    let id: String = DtoMapper::new().resolve_typed(config.id.as_ref())?;
    anyhow::ensure!(!id.is_empty(), "Cannot preserve an empty instance identity");
    config.computation = None;
    config.sources.clear();
    config.queries.clear();
    config.reactions.clear();
    Ok((id, config))
}

impl ConfigPersistence {
    /// Preserve host-only settings without taking a lossy runtime snapshot of
    /// components or interpreting startup definitions as restoration authority.
    pub async fn new(
        config_file_path: PathBuf,
        registry: InstanceRegistry,
        host: String,
        port: u16,
        log_level: String,
        persist_config: bool,
        original_config: &DrasiServerConfig,
    ) -> Result<Self> {
        let mut server_configuration = original_config.clone();
        server_configuration.instances.clear();
        server_configuration.host = ConfigValue::Static(host);
        server_configuration.port = ConfigValue::Static(port);
        server_configuration.log_level = ConfigValue::Static(log_level);
        server_configuration.persist_config = persist_config;
        let mut instance_configs = IndexMap::new();
        for instance in &original_config.instances {
            let (id, settings) = capture_instance_settings(instance.clone())?;
            let core = registry
                .get(&id)
                .await
                .with_context(|| format!("Cannot preserve settings for missing instance '{id}'"))?;
            anyhow::ensure!(
                !persist_config || core.configuration_is_persistent(),
                "Instance '{id}' needs a durable configurationStore before enabling persistConfig; \
                 opaque pre-built components cannot be restored from a runtime snapshot"
            );
            anyhow::ensure!(
                instance_configs.insert(id.clone(), settings).is_none(),
                "Duplicate instance configuration '{id}'"
            );
        }
        Ok(Self {
            default_store: crate::managed_configuration::file_configuration_store(
                &config_file_path,
            )?,
            config_file_path,
            registry,
            server_configuration,
            save_gate: Arc::new(Mutex::new(())),
            instance_configs: RwLock::new(instance_configs),
        })
    }

    pub(crate) fn default_store(&self) -> crate::managed_configuration::ConfigurationStoreConfig {
        self.default_store.clone()
    }

    /// Replace the explicitly supplied settings, without inheriting another
    /// instance's budget, providers or storage configuration.
    pub async fn register_instance(&self, config: DrasiLibInstanceConfig) -> Result<()> {
        if !self.server_configuration.persist_config {
            return Ok(());
        }
        let (id, settings) = capture_instance_settings(config)?;
        let _save = self.save_gate.lock().await;
        self.instance_configs.write().await.insert(id, settings);
        Ok(())
    }

    /// Save host settings atomically. An empty registry writes an explicit empty
    /// instance list; existing component records are never regenerated or reset.
    pub async fn save(&self) -> Result<()> {
        if !self.server_configuration.persist_config {
            debug!("Persistence disabled (persist_config: false), skipping save");
            return Ok(());
        }
        let save = self.save_gate.clone().lock_owned().await;
        info!(
            "Saving configuration to {}",
            self.config_file_path.display()
        );

        let instance_settings = self.instance_configs.read().await;
        let mut instances = Vec::new();
        for (id, core) in self.registry.list().await {
            let mut settings = instance_settings.get(&id).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot persist instance '{id}': its config-only settings were not registered"
                )
            })?;
            anyhow::ensure!(
                core.configuration_is_persistent(),
                "Cannot persist instance '{id}' without a durable configurationStore"
            );
            anyhow::ensure!(
                settings.configuration_store.is_some(),
                "Cannot persist managed instance '{id}' without its configurationStore"
            );
            settings.allow_external_component_ids = core.allow_external_component_ids();
            if let Some(crate::managed_configuration::ConfigurationStoreConfig::Yaml {
                initialize_if_missing,
                ..
            }) = &mut settings.configuration_store
            {
                *initialize_if_missing = false;
            }
            instances.push(settings);
        }

        let mut configuration = self.server_configuration.clone();
        configuration.instances = instances;
        configuration.validate_for_persistence()?;
        let path = self.config_file_path.clone();
        tokio::task::spawn_blocking(move || {
            let _save = save;
            configuration_store::atomic_write_yaml(&path, &configuration)
        })
        .await
        .context("configuration settings writer failed")??;

        info!(
            "Configuration saved successfully to {}",
            self.config_file_path.display()
        );
        Ok(())
    }

    pub fn is_writable(&self) -> bool {
        Self::check_write_access(&self.config_file_path)
    }

    fn check_write_access(path: &Path) -> bool {
        std::fs::OpenOptions::new().append(true).open(path).is_ok()
    }
}

#[cfg(test)]
mod tests;
