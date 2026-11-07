// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::{
    computation::v1::{ResourceHandle, ResourceSpecification},
    management::{
        CommittedConfiguration, ConfigurationStore, DesiredInstance, ManagementResourceResolver,
        OrdinaryComponent, OrdinaryComponentDefinition,
    },
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
    computation::{ComputationConfigInput, ServerManagementResources},
    plugin_registry::PluginRegistry,
};

pub const INITIAL_REQUEST_ID: &str = "drasi-server/initial-configuration";

pub(crate) mod ordinary;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ConfigurationStoreConfig {
    Yaml {
        path: PathBuf,
        #[serde(rename = "initializeIfMissing", default = "initialize_if_missing")]
        initialize_if_missing: bool,
    },
    Redb {
        path: PathBuf,
        #[serde(rename = "keyFile")]
        key_file: PathBuf,
    },
}

fn initialize_if_missing() -> bool {
    true
}

impl ConfigurationStoreConfig {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Yaml { path, .. } => {
                anyhow::ensure!(
                    !path.as_os_str().is_empty(),
                    "configurationStore requires a nonempty path"
                );
            }
            Self::Redb { path, key_file } => {
                anyhow::ensure!(
                    !path.as_os_str().is_empty() && !key_file.as_os_str().is_empty(),
                    "configurationStore requires nonempty path and keyFile"
                );
            }
        }
        Ok(())
    }
}

pub(crate) fn file_configuration_store(
    config_path: &std::path::Path,
) -> Result<ConfigurationStoreConfig> {
    let path = std::fs::canonicalize(config_path).context("resolve configuration file owner")?;
    let mut path = path.into_os_string();
    path.push(".drasi-state.yaml");
    Ok(ConfigurationStoreConfig::Yaml {
        path: PathBuf::from(path),
        initialize_if_missing: true,
    })
}

struct OpenStore {
    key_digest: Option<[u8; 32]>,
    provider: StoreProvider,
}

#[derive(Clone)]
pub(crate) enum StoreProvider {
    Redb(Arc<RedbConfigurationStore>),
    Yaml(Arc<crate::persistence::configuration_store::YamlConfigurationStore>),
}

impl StoreProvider {
    fn provider(&self) -> Arc<dyn ConfigurationStore> {
        match self {
            Self::Redb(store) => store.clone(),
            Self::Yaml(store) => store.clone(),
        }
    }

    pub(crate) async fn configured_instance_ids(&self) -> Result<Vec<String>> {
        match self {
            Self::Redb(store) => store.configured_instance_ids().await,
            Self::Yaml(store) => store.configured_instance_ids().await,
        }
    }

    pub(crate) async fn initialize_if_absent_with(
        &self,
        instance: &str,
        request: &str,
        seed: impl FnOnce() -> Result<DesiredInstance> + Send,
    ) -> Result<bool> {
        match self {
            Self::Redb(store) => {
                store
                    .initialize_if_absent_with(instance, request, seed)
                    .await
            }
            Self::Yaml(store) => {
                store
                    .initialize_if_absent_with(instance, request, seed)
                    .await
            }
        }
    }

    async fn current(&self, instance: &str) -> Result<CommittedConfiguration> {
        let session = self.provider().open(instance).await?;
        let current = session.load().await;
        let closed = session.close().await;
        match (current, closed) {
            (Ok(current), Ok(())) => Ok(current),
            (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(close)) => {
                Err(error.context(format!("configuration close also failed: {close:#}")))
            }
        }
    }
}

/// Physical configuration databases, not a second runtime or desired-state cache.
#[derive(Clone, Default)]
pub struct ConfigurationStores {
    stores: Arc<Mutex<BTreeMap<PathBuf, OpenStore>>>,
}

impl ConfigurationStores {
    pub(crate) async fn configure_host(
        builder: DrasiLibBuilder,
        plugins: Arc<RwLock<PluginRegistry>>,
        secrets: Option<Arc<dyn SecretStoreProvider>>,
    ) -> Result<DrasiLibBuilder> {
        let factories = crate::computation::native_factory_registry(&*plugins.read().await)?;
        Ok(builder
            .with_component_factories(factories)
            .with_management_resources(Arc::new(LiveResources { plugins, secrets })))
    }

    pub(crate) fn configure_file_persistence(
        config: &mut crate::config::DrasiServerConfig,
        path: &std::path::Path,
    ) -> Result<()> {
        if config.persist_config {
            let store = file_configuration_store(path)?;
            for instance in &mut config.instances {
                instance
                    .configuration_store
                    .get_or_insert_with(|| store.clone());
            }
        }
        Ok(())
    }

    pub async fn resolve_instance_ids(
        &self,
        config: &mut crate::config::DrasiServerConfig,
    ) -> Result<()> {
        let mut bindings: Vec<(usize, Arc<dyn ConfigurationStore>)> = Vec::new();
        for (index, instance) in config.instances.iter().enumerate() {
            let Some(settings) = &instance.configuration_store else {
                continue;
            };
            let provider = self.open(settings).await?;
            for (previous, existing) in &bindings {
                anyhow::ensure!(
                    !Arc::ptr_eq(existing, &provider.provider())
                        || !instance.id.is_generated()
                            && !config.instances[*previous].id.is_generated(),
                    "Instances sharing a configurationStore must declare explicit IDs"
                );
            }
            bindings.push((index, provider.provider()));
        }
        for (index, _provider) in bindings {
            let instance = &mut config.instances[index];
            if !instance.id.is_generated() {
                continue;
            }
            let settings = instance
                .configuration_store
                .as_ref()
                .context("configuration store disappeared during instance resolution")?;
            match self.open(settings).await?.configured_instance_ids().await?.as_slice() {
                [] => {}
                [id] => instance.id = crate::api::models::ConfigValue::Static(id.clone()).into(),
                _ => anyhow::bail!(
                    "configurationStore contains multiple instances; specify an explicit ID instead of selecting the first"
                ),
            }
        }
        Ok(())
    }

    pub(crate) async fn open(&self, config: &ConfigurationStoreConfig) -> Result<StoreProvider> {
        config.validate()?;
        let (path, key) = match config {
            ConfigurationStoreConfig::Yaml { path, .. } => (path, None),
            ConfigurationStoreConfig::Redb { path, key_file } => {
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
                (path, Some(key))
            }
        };
        let key_digest = key.map(|key| <[u8; 32]>::from(Sha256::digest(key)));
        let initialize = !matches!(
            config,
            ConfigurationStoreConfig::Yaml {
                initialize_if_missing: false,
                ..
            }
        );
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
                    "configuration store is already open with a different backend or encryption key"
                );
                return Ok(open.provider.clone());
            }
            let provider = match key {
                Some(key) => {
                    StoreProvider::Redb(Arc::new(RedbConfigurationStore::new(&path, key)?))
                }
                None => StoreProvider::Yaml(Arc::new(
                    crate::persistence::configuration_store::YamlConfigurationStore::new(
                        path.clone(),
                        initialize,
                    )?,
                )),
            };
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
        allow_external_component_ids: bool,
        config: &ConfigurationStoreConfig,
        seed: Option<&ComputationConfigInput>,
        ordinary: Vec<OrdinaryComponentDefinition>,
        plugins: Arc<RwLock<PluginRegistry>>,
        secrets: Option<Arc<dyn SecretStoreProvider>>,
    ) -> Result<DrasiLibBuilder> {
        let provider = self.open(config).await?;
        let factories = crate::computation::native_factory_registry(&*plugins.read().await)?;
        provider
            .initialize_if_absent_with(instance, INITIAL_REQUEST_ID, || {
                anyhow::ensure!(
                    !matches!(config, ConfigurationStoreConfig::Yaml { initialize_if_missing: false, .. }),
                    "Stored configuration for instance '{instance}' is missing; refusing to initialize empty state"
                );
                let mut desired = seed.map(|config| {
                    if !allow_external_component_ids {
                        if let ComputationConfigInput::Definition(config) = config {
                            if let Some(component) = config.definition.components.first() {
                                return Err(
                                    drasi_lib::computation::v1::GraphError::ExternalComponentId {
                                        component: component.descriptor.id().clone(),
                                    }
                                    .into(),
                                );
                            }
                        }
                    }
                    config
                        .resolve_with_ids(&factories, |component| {
                            if let Some(id) = &component.id {
                                anyhow::ensure!(
                                    allow_external_component_ids,
                                    "externally assigned component UUID {id} is not permitted"
                                );
                                Ok(id.clone())
                            } else {
                                Ok(drasi_lib::config::ComponentId::generate())
                            }
                        })
                        .map(|config| DesiredInstance::from(config.definition.clone()))
                })
                .transpose()
                .map(Option::unwrap_or_default)?;
                for definition in ordinary {
                    let id = match &definition.id {
                        Some(id) if !allow_external_component_ids => {
                            return Err(drasi_lib::computation::v1::GraphError::ExternalComponentId {
                                component: id.clone(),
                            }.into());
                        }
                        Some(id) => id.clone(),
                        None => drasi_lib::config::ComponentId::generate(),
                    };
                    desired.ordinary.push(definition.with_assigned_id(id));
                }
                desired.normalized()
            })
            .await?;
        let current = provider.current(instance).await?;
        let builder = if current
            .desired
            .ordinary
            .iter()
            .any(|record| matches!(record.component, OrdinaryComponent::Query { .. }))
        {
            builder.with_query_factory(crate::query_plugin::factory(&*plugins.read().await)?)
        } else {
            builder
        };
        Ok(builder
            .with_id(instance)
            .with_component_factories(factories)
            .with_management_resources(Arc::new(LiveResources { plugins, secrets }))
            .with_configuration_store(provider.provider()))
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
