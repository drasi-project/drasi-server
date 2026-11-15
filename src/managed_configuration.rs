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
    computation::{ComputationConfigInput, ServerManagementResources},
    ordinary_resolver::{InstanceComponentSettings, OrdinarySeed, ServerOrdinaryResolver},
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
    pub async fn resolve_instance_ids(
        &self,
        config: &mut crate::config::DrasiServerConfig,
    ) -> Result<()> {
        let mut bindings: Vec<(usize, Arc<RedbConfigurationStore>)> = Vec::new();
        for (index, instance) in config.instances.iter().enumerate() {
            let Some(settings) = &instance.configuration_store else {
                continue;
            };
            let provider = self.open(settings).await?;
            for (previous, existing) in &bindings {
                anyhow::ensure!(
                    !Arc::ptr_eq(existing, &provider)
                        || !instance.id.is_generated()
                            && !config.instances[*previous].id.is_generated(),
                    "Instances sharing a configurationStore must declare explicit IDs"
                );
            }
            bindings.push((index, provider));
        }
        for (index, provider) in bindings {
            let instance = &mut config.instances[index];
            if !instance.id.is_generated() {
                continue;
            }
            match provider.configured_instance_ids().await?.as_slice() {
                [] => {}
                [id] => instance.id = crate::api::models::ConfigValue::Static(id.clone()).into(),
                _ => anyhow::bail!(
                    "configurationStore contains multiple instances; specify an explicit ID instead of selecting the first"
                ),
            }
        }
        Ok(())
    }

    pub async fn open(
        &self,
        config: &ConfigurationStoreConfig,
    ) -> Result<Arc<RedbConfigurationStore>> {
        config.validate()?;
        let ConfigurationStoreConfig::Redb { path, key_file } = config;
        let file = match tokio::fs::File::open(key_file).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                generate_key(path, key_file).await?;
                tokio::fs::File::open(key_file)
                    .await
                    .context("read configuration encryption key")?
            }
            Err(error) => return Err(error).context("read configuration encryption key"),
        };
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

    /// Prepare a persisted instance without declared ordinary components.
    pub async fn configure(
        &self,
        builder: DrasiLibBuilder,
        instance: &str,
        config: &ConfigurationStoreConfig,
        seed: Option<&ComputationConfigInput>,
        plugins: Arc<RwLock<PluginRegistry>>,
        secrets: Option<Arc<dyn SecretStoreProvider>>,
    ) -> Result<DrasiLibBuilder> {
        self.configure_with_components(
            builder,
            instance,
            config,
            seed,
            OrdinarySeed::default(),
            InstanceComponentSettings::default(),
            plugins,
            secrets,
        )
        .await
    }

    /// Prepare a persisted instance. On first start the store is seeded with
    /// the declared computation topology and ordinary components, keyed by
    /// [`INITIAL_REQUEST_ID`]; afterwards the store is authoritative.
    #[allow(clippy::too_many_arguments)]
    pub async fn configure_with_components(
        &self,
        builder: DrasiLibBuilder,
        instance: &str,
        config: &ConfigurationStoreConfig,
        seed: Option<&ComputationConfigInput>,
        ordinary: OrdinarySeed<'_>,
        settings: InstanceComponentSettings,
        plugins: Arc<RwLock<PluginRegistry>>,
        secrets: Option<Arc<dyn SecretStoreProvider>>,
    ) -> Result<DrasiLibBuilder> {
        let provider = self.open(config).await?;
        let (factories, query_factory, records) = {
            let registry = plugins.read().await;
            let factories = crate::computation::native_factory_registry(&registry)?;
            let query_factory = match crate::query_plugin::factory(&registry) {
                Ok(factory) => Some(factory),
                Err(error) if ordinary.queries.is_empty() => {
                    log::debug!("Persisted instance '{instance}' has no query factory: {error}");
                    None
                }
                Err(error) => return Err(error).context("query plugin for declared queries"),
            };
            // Seed errors only matter when the store is still empty.
            let records = ordinary.records(&registry, &settings);
            (factories, query_factory, records)
        };
        provider
            .initialize_if_absent_with(instance, INITIAL_REQUEST_ID, || {
                let mut desired = seed
                    .map(|config| {
                        config
                            .resolve(&factories)
                            .map(|config| DesiredInstance::from(config.definition.clone()))
                    })
                    .transpose()?
                    .unwrap_or_default();
                desired.ordinary = records.context("invalid declared components")?;
                let desired = desired.normalized()?;
                crate::ordinary_resolver::ensure_references_resolved(&desired)?;
                Ok(desired)
            })
            .await
            .with_context(|| format!("initialize configuration store for instance '{instance}'"))?;
        let mut builder = builder
            .with_id(instance)
            .with_component_factories(factories)
            .with_management_resources(Arc::new(LiveResources {
                plugins: plugins.clone(),
                secrets,
            }))
            .with_ordinary_component_resolver(Arc::new(ServerOrdinaryResolver::new(
                plugins, settings,
            )))
            .with_configuration_store(provider);
        if let Some(factory) = query_factory {
            builder = builder.with_query_factory(factory);
        }
        Ok(builder)
    }
}

/// Create a configuration key only for a new database. A database without its
/// key fails closed rather than being re-keyed.
async fn generate_key(database: &std::path::Path, key_file: &std::path::Path) -> Result<()> {
    anyhow::ensure!(
        !tokio::fs::try_exists(database).await.unwrap_or(true),
        "configuration encryption key file is missing for an existing configuration database"
    );
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key)
        .map_err(|error| anyhow::anyhow!("generate configuration encryption key: {error}"))?;
    if let Some(parent) = key_file.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent)
            .await
            .context("create configuration key directory")?;
    }
    let key_file = key_file.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&key_file) {
            Ok(file) => file,
            // A concurrent preparation created it; use that key.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
            Err(error) => return Err(error).context("create configuration encryption key"),
        };
        file.write_all(&key)
            .and_then(|_| file.sync_all())
            .context("write configuration encryption key")?;
        log::info!(
            "Generated configuration encryption key at {}",
            key_file.display()
        );
        Ok(())
    })
    .await
    .context("configuration key worker failed")?
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
    fn plan_update(
        &self,
        resource: &drasi_lib::computation::v1::ResourceId,
        previous: &drasi_lib::computation::v1::DesiredTopology,
        desired: &drasi_lib::computation::v1::DesiredTopology,
    ) -> Result<drasi_lib::computation::v1::ComponentUpdatePlan> {
        let plugins = self
            .plugins
            .try_read()
            .context("plugin registry is being updated; retry configuration acceptance")?;
        ServerManagementResources::new(&plugins, self.secrets.clone())?
            .plan_update(resource, previous, desired)
    }

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
