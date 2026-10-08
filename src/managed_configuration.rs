// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::{
    computation::v1::{ResourceHandle, ResourceSpecification},
    management::{DesiredInstance, ManagementResourceResolver},
    secret_store::SecretStoreProvider,
    DrasiLibBuilder,
};
use drasi_state_store_redb::RedbConfigurationStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    io::AsyncReadExt,
    sync::{Mutex, RwLock},
};

use crate::{
    computation::{ComputationConfig, ServerManagementResources},
    plugin_registry::PluginRegistry,
};

pub const INITIAL_REQUEST_ID: &str = "drasi-server/initial-configuration";

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ConfigurationStoreConfig {
    Redb {
        path: PathBuf,
        #[serde(rename = "keyFile")]
        key_file: PathBuf,
    },
}

impl ConfigurationStoreConfig {
    pub fn validate(&self) -> Result<()> {
        let Self::Redb { path, key_file } = self;
        anyhow::ensure!(
            !path.as_os_str().is_empty() && !key_file.as_os_str().is_empty(),
            "configurationStore requires nonempty path and keyFile"
        );
        Ok(())
    }
}

struct OpenStore {
    key_digest: [u8; 32],
    provider: Arc<RedbConfigurationStore>,
}

/// Physical configuration databases, not a second runtime or desired-state cache.
#[derive(Clone, Default)]
pub struct ConfigurationStores {
    stores: Arc<Mutex<BTreeMap<PathBuf, OpenStore>>>,
}

impl ConfigurationStores {
    pub async fn open(
        &self,
        config: &ConfigurationStoreConfig,
    ) -> Result<Arc<RedbConfigurationStore>> {
        config.validate()?;
        let ConfigurationStoreConfig::Redb { path, key_file } = config;
        let file = tokio::fs::File::open(key_file)
            .await
            .context("read configuration encryption key")?;
        let mut key = Vec::with_capacity(33);
        file.take(33)
            .read_to_end(&mut key)
            .await
            .context("read configuration encryption key")?;
        let key: [u8; 32] = key.try_into().map_err(|_| {
            anyhow::anyhow!("configuration key file must contain exactly 32 raw bytes")
        })?;
        let key_digest: [u8; 32] = Sha256::digest(key).into();
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let name = path
            .file_name()
            .context("configuration database path must name a file")?;
        tokio::fs::create_dir_all(parent)
            .await
            .context("create configuration database directory")?;
        let path = match tokio::fs::canonicalize(path).await {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::canonicalize(parent).await?.join(name)
            }
            Err(error) => return Err(error).context("resolve configuration database path"),
        };
        let mut stores = self.stores.clone().lock_owned().await;
        // The cache guard and provider acquisition survive cancellation together.
        tokio::task::spawn_blocking(move || {
            if let Some(open) = stores.get(&path) {
                anyhow::ensure!(
                    open.key_digest == key_digest,
                    "configuration database is already open with a different encryption key"
                );
                return Ok(open.provider.clone());
            }
            let provider = Arc::new(RedbConfigurationStore::new(&path, key)?);
            stores.insert(
                path,
                OpenStore {
                    key_digest,
                    provider: provider.clone(),
                },
            );
            Ok(provider)
        })
        .await
        .context("configuration database worker failed")?
    }

    pub async fn configure(
        &self,
        builder: DrasiLibBuilder,
        instance: &str,
        config: &ConfigurationStoreConfig,
        seed: Option<&ComputationConfig>,
        plugins: Arc<RwLock<PluginRegistry>>,
        secrets: Option<Arc<dyn SecretStoreProvider>>,
    ) -> Result<DrasiLibBuilder> {
        let provider = self.open(config).await?;
        provider
            .initialize_if_absent_with(instance, INITIAL_REQUEST_ID, || {
                if let Some(seed) = seed {
                    crate::computation::validate_definition(seed)?;
                }
                Ok(seed
                    .map(|config| DesiredInstance::from(config.definition.clone()))
                    .unwrap_or_default())
            })
            .await?;
        let factories = plugins.read().await.computation_factory_registry()?;
        Ok(builder
            .with_id(instance)
            .with_component_factories(factories)
            .with_management_resources(Arc::new(LiveResources { plugins, secrets }))
            .with_configuration_store(provider))
    }
}

pub(crate) async fn start_instance(core: &drasi_lib::DrasiLib) -> Result<()> {
    if let Err(error) = core.start().await {
        if !core.configuration_is_persistent() || !core.is_running().await {
            return Err(error.into());
        }
        let status = core.management_status().await?;
        if status.converged() || core.computation_info().await?.driver_error.is_some() {
            return Err(error.into());
        }
        log::warn!(
            "Managed deployment revision {} is not ready; retained for inspection and reconciliation",
            status.revision
        );
    }
    Ok(())
}

struct LiveResources {
    plugins: Arc<RwLock<PluginRegistry>>,
    secrets: Option<Arc<dyn SecretStoreProvider>>,
}

#[async_trait]
impl ManagementResourceResolver for LiveResources {
    fn validate_transition(
        &self,
        previous: &drasi_lib::computation::v1::DesiredTopology,
        desired: &drasi_lib::computation::v1::DesiredTopology,
    ) -> Result<()> {
        let plugins = self
            .plugins
            .try_read()
            .context("plugin registry is being updated; retry configuration acceptance")?;
        ServerManagementResources::new(&plugins, self.secrets.clone())?
            .validate_transition(previous, desired)
    }

    async fn resolve(
        &self,
        instance: &str,
        graph: &str,
        specification: &ResourceSpecification,
        configuration: &serde_json::Value,
    ) -> Result<ResourceHandle> {
        self.resolve_with_dependencies(
            instance,
            graph,
            specification,
            configuration,
            &std::collections::BTreeMap::new(),
        )
        .await
    }

    async fn resolve_with_dependencies(
        &self,
        instance: &str,
        graph: &str,
        specification: &ResourceSpecification,
        configuration: &serde_json::Value,
        dependencies: &std::collections::BTreeMap<
            drasi_lib::computation::v1::ResourceId,
            ResourceHandle,
        >,
    ) -> Result<ResourceHandle> {
        let resources =
            ServerManagementResources::new(&*self.plugins.read().await, self.secrets.clone())?;
        resources
            .resolve_with_dependencies(instance, graph, specification, configuration, dependencies)
            .await
    }
}
