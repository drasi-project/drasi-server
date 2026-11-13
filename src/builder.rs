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

use drasi_lib::secret_store::SecretStoreProvider;
use drasi_lib::state_store::StateStoreProvider;
use drasi_lib::{DrasiError, DrasiLib, DrasiLibBuilder, Query};
use drasi_lib::{IndexBackendPlugin, Reaction as ReactionTrait, Source as SourceTrait};
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{ensure, Context};
use drasi_lib::observability::{init_default_subscriber, TelemetryProfile, TelemetrySettings};

use crate::{
    api::{mappings::DtoMapper, models::ConfigValue},
    builder_result::{DrasiServerWithHandles, DrasiServerWithOwnedHandles},
    config::telemetry::{MetricsBackend, TelemetryConfig},
    server::{DrasiServer, ServerCleanupHandle, ServerPreparation},
    telemetry::StandaloneTelemetry,
    TelemetryBindings,
};

/// Builder for creating a DrasiServer instance programmatically
pub struct DrasiServerBuilder {
    core_builders: Vec<DrasiLibBuilder>,
    enable_api: bool,
    enable_ui: bool,
    port: Option<u16>,
    host: Option<String>,
    config_file_path: Option<String>,
    telemetry_config: Option<TelemetryConfig>,
    telemetry_bindings: Option<TelemetryBindings>,
}

impl Default for DrasiServerBuilder {
    fn default() -> Self {
        Self {
            core_builders: vec![DrasiLib::builder()],
            enable_api: false,
            enable_ui: true,
            port: Some(8080),
            host: Some("127.0.0.1".to_string()),
            config_file_path: None,
            telemetry_config: None,
            telemetry_bindings: None,
        }
    }
}

impl DrasiServerBuilder {
    /// Explicitly register the query plugin for the primary instance.
    pub fn with_query_factory(
        mut self,
        factory: Arc<dyn drasi_lib::computation::v1::ComponentFactory>,
    ) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_query_factory(factory);
        self
    }

    fn primary_builder_mut(&mut self) -> &mut DrasiLibBuilder {
        self.core_builders
            .first_mut()
            .expect("DrasiServerBuilder must have at least one DrasiLibBuilder; call new() to create the default builder")
    }

    /// Create a new DrasiServerBuilder with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace configured telemetry without discarding a separately supplied
    /// borrowed policy. Supplying both inputs is a build error in either order.
    pub fn with_telemetry(mut self, config: TelemetryConfig) -> Self {
        self.telemetry_config = Some(config);
        self
    }

    /// Change only the configured profile; final validation happens at build.
    pub fn with_telemetry_profile(mut self, profile: TelemetryProfile) -> Self {
        let config = self.telemetry_config.get_or_insert_with(Default::default);
        config.profile = ConfigValue::Static(
            match profile {
                TelemetryProfile::Off => "off",
                TelemetryProfile::Basic => "basic",
                TelemetryProfile::Debug => "debug",
                TelemetryProfile::Persistence => "persistence",
            }
            .into(),
        );
        self
    }

    /// Change only the configured backend, retaining destination settings.
    pub fn with_metrics_backend(mut self, backend: MetricsBackend) -> Self {
        let config = self.telemetry_config.get_or_insert_with(Default::default);
        config.metrics.backend = ConfigValue::Static(
            match backend {
                MetricsBackend::None => "none",
                MetricsBackend::Prometheus => "prometheus",
                MetricsBackend::Otlp => "otlp",
            }
            .into(),
        );
        self
    }

    /// Replace borrowed telemetry; never install globals or close its providers.
    pub fn with_telemetry_bindings(mut self, bindings: TelemetryBindings) -> Self {
        self.telemetry_bindings = Some(bindings);
        self
    }

    /// Set the server ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_id(id);
        self
    }

    /// Add a pre-built source instance (ownership transferred)
    pub fn with_source(
        mut self,
        name: impl Into<String>,
        source: impl SourceTrait + 'static,
    ) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_source(name, source);
        self
    }

    /// Add a pre-built reaction instance (ownership transferred)
    pub fn with_reaction(
        mut self,
        name: impl Into<String>,
        reaction: impl ReactionTrait + 'static,
    ) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_reaction(name, reaction);
        self
    }

    pub fn with_allow_external_component_ids(mut self, allow: bool) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_allow_external_component_ids(allow);
        self
    }

    pub fn with_source_factory<S: SourceTrait + 'static>(
        mut self,
        name: impl Into<String>,
        auto_start: bool,
        factory: impl FnOnce(drasi_lib::config::ComponentId) -> anyhow::Result<S> + Send + 'static,
    ) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_source_factory(name, auto_start, factory);
        self
    }

    pub fn with_reaction_inputs_factory<R: ReactionTrait + 'static>(
        mut self,
        name: impl Into<String>,
        inputs: Vec<drasi_lib::config::ComponentReference>,
        auto_start: bool,
        factory: impl FnOnce(
                drasi_lib::config::ComponentId,
                Vec<drasi_lib::config::ComponentId>,
            ) -> anyhow::Result<R>
            + Send
            + 'static,
    ) -> Self {
        let builder = self.primary_builder_mut();
        *builder =
            std::mem::take(builder).with_reaction_inputs_factory(name, inputs, auto_start, factory);
        self
    }

    /// Register a named index backend provider and make it the instance default.
    ///
    /// By default, DrasiLib uses in-memory indexes. Use this method to inject a
    /// persistent provider (e.g. RocksDB) under `name` and mark it as the default
    /// backend: every query without an explicit `storageBackend` is persisted to
    /// it. When a per-query `storageBackend` references a named provider, it must
    /// specify this same `name`.
    ///
    /// This mirrors [`DrasiLibBuilder::with_default_index_provider`]. drasi-server
    /// registers its built-in RocksDB provider under
    /// [`crate::PERSISTENT_INDEX_PROVIDER_NAME`] (`"rocksdb"`).
    pub fn with_default_index_provider(
        mut self,
        name: impl Into<String>,
        provider: Arc<dyn IndexBackendPlugin>,
    ) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_default_index_provider(name, provider);
        self
    }

    /// Add a state store provider for plugin state persistence
    ///
    /// By default, DrasiLib uses an in-memory state store. Use this method to inject
    /// a persistent state store provider like REDB for plugins to persist runtime state.
    ///
    /// # Arguments
    ///
    /// * `provider` - An Arc-wrapped StateStoreProvider implementation
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use drasi_state_store_redb::RedbStateStoreProvider;
    ///
    /// let state_store = RedbStateStoreProvider::new("./data/state.redb")?;
    /// let server = DrasiServerBuilder::new()
    ///     .with_state_store_provider(Arc::new(state_store))
    ///     .build()
    ///     .await?;
    /// ```
    pub fn with_state_store_provider(mut self, provider: Arc<dyn StateStoreProvider>) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_state_store_provider(provider);
        self
    }

    /// Add a secret store provider for resolving ConfigValue::Secret references
    ///
    /// When set, component configs can use `{ kind: Secret, name: "..." }` values
    /// whose actual values are resolved at runtime by the configured provider.
    pub fn with_secret_store_provider(mut self, provider: Arc<dyn SecretStoreProvider>) -> Self {
        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_secret_store_provider(provider);
        self
    }

    /// Add a query configuration helper (creates a Cypher query)
    /// For GQL queries, use with_query() with Query::gql() builder instead
    pub fn with_query_config(
        mut self,
        name: impl Into<String>,
        query_str: impl Into<String>,
        sources: Vec<String>,
    ) -> Result<Self, DrasiError> {
        let mut query_builder = Query::cypher(name).query(query_str);

        for source in sources {
            query_builder = query_builder.from_input(source);
        }

        let builder = self.primary_builder_mut();
        *builder = std::mem::take(builder).with_query(query_builder.build()?);
        Ok(self)
    }

    /// Add a query with simple parameters
    pub fn with_simple_query(
        self,
        name: impl Into<String>,
        query_str: impl Into<String>,
        sources: Vec<String>,
    ) -> Result<Self, DrasiError> {
        self.with_query_config(name, query_str, sources)
    }

    /// Add an additional DrasiLibBuilder to run as a separate DrasiLib instance
    pub fn add_instance_builder(mut self, builder: DrasiLibBuilder) -> Self {
        self.core_builders.push(builder);
        self
    }

    /// Enable the REST API on the default port
    pub fn enable_api(mut self) -> Self {
        self.enable_api = true;
        self
    }

    /// Enable the web UI (enabled by default)
    pub fn enable_ui(mut self) -> Self {
        self.enable_ui = true;
        self
    }

    /// Disable the web UI
    pub fn disable_ui(mut self) -> Self {
        self.enable_ui = false;
        self
    }

    /// Enable the REST API on a specific port
    pub fn with_port(mut self, port: u16) -> Self {
        self.enable_api = true;
        self.port = Some(port);
        self
    }

    /// Enable the REST API on a specific host and port
    pub fn with_host_port(mut self, host: impl Into<String>, port: u16) -> Self {
        self.enable_api = true;
        self.host = Some(host.into());
        self.port = Some(port);
        self
    }

    /// Build only the primary Core. Configured Server-owned logging cannot be
    /// returned bare; use the owned result or supply borrowed bindings instead.
    pub async fn build_core(self) -> Result<DrasiLib, DrasiError> {
        let cleanup = ServerCleanupHandle::bootstrap();
        let submitted = (|| {
            let admission = cleanup.admit()?;
            ServerPreparation::start_blocking(admission, move |owner| {
                let built = self.construct(&owner, false, true, true)?;
                Ok(built.primary()?.as_ref().clone())
            })
        })();
        match submitted {
            Ok(preparation) => preparation,
            Err(error) => ServerPreparation::failed(cleanup, error),
        }
        .finish()
        .await
        .map_err(DrasiError::from)
    }

    /// Set the config file path for persistence
    pub fn with_config_file(mut self, path: impl Into<String>) -> Self {
        self.config_file_path = Some(path.into());
        self
    }

    /// Build a DrasiServer instance with optional API
    pub async fn build(self) -> Result<crate::server::DrasiServer, DrasiError> {
        self.prepare().finish().await.map_err(DrasiError::from)
    }

    /// Obtain the cleanup address before awaiting construction. Dropping the
    /// waiter rolls back accepted construction without aborting its workers.
    /// Call inside a live Tokio runtime and keep it alive through cleanup.
    pub fn prepare(self) -> ServerPreparation<DrasiServer> {
        let cleanup = ServerCleanupHandle::bootstrap();
        let submitted = (|| {
            let admission = cleanup.admit()?;
            ServerPreparation::start_blocking(admission, move |owner| {
                let built = self.construct(&owner, false, false, false)?;
                Ok(built.into_server(owner))
            })
        })();
        match submitted {
            Ok(preparation) => preparation,
            Err(error) => ServerPreparation::failed(cleanup, error),
        }
    }

    /// Construct and start Cores under an owner available before async work.
    /// Start acknowledges Core startup; node readiness is observed separately.
    ///
    /// ```no_run
    /// use drasi_server::{DrasiServerBuilder, TelemetryBindings};
    ///
    /// # async fn example(bindings: TelemetryBindings) -> anyhow::Result<()> {
    /// let preparation = DrasiServerBuilder::new()
    ///     .with_id("analytics")
    ///     .with_telemetry_bindings(bindings)
    ///     .prepare_with_owned_handles();
    /// let cleanup = preparation.cleanup_handle();
    /// let built = preparation.finish().await?;
    /// // Do application work with built.servers; providers remain borrowed.
    /// built.shutdown().await?;
    /// assert_eq!(cleanup.status().pending_workers, 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn prepare_with_owned_handles(self) -> ServerPreparation<DrasiServerWithOwnedHandles> {
        let cleanup = ServerCleanupHandle::bootstrap();
        let submitted = (|| {
            let admission = cleanup.admit()?;
            ServerPreparation::start_blocking(admission, move |owner| {
                let built = self.construct(&owner, true, false, false)?;
                Ok(DrasiServerWithOwnedHandles {
                    server: built.primary()?,
                    servers: built.servers,
                    cleanup: owner,
                })
            })
        })();
        match submitted {
            Ok(preparation) => preparation,
            Err(error) => ServerPreparation::failed(cleanup, error),
        }
    }

    pub async fn build_with_owned_handles(self) -> Result<DrasiServerWithOwnedHandles, DrasiError> {
        self.prepare_with_owned_handles()
            .finish()
            .await
            .map_err(DrasiError::from)
    }

    /// Build a DrasiLib instance, start it, and return a handle
    ///
    /// Note: Application source/reaction handles were removed during the plugin architecture refactor.
    /// Use the builder pattern in drasi-lib directly for programmatic integration.
    /// Newly configured Server-owned telemetry requires the owned-result API;
    /// borrowed bindings retain this result's existing two-field shape.
    pub async fn build_with_handles(
        self,
    ) -> Result<crate::builder_result::DrasiServerWithHandles, DrasiError> {
        let cleanup = ServerCleanupHandle::bootstrap();
        let submitted = (|| {
            let admission = cleanup.admit()?;
            ServerPreparation::start_blocking(admission, move |owner| {
                let built = self.construct(&owner, true, false, true)?;
                Ok(DrasiServerWithHandles {
                    server: built.primary()?,
                    servers: built.servers,
                })
            })
        })();
        match submitted {
            Ok(preparation) => preparation,
            Err(error) => ServerPreparation::failed(cleanup, error),
        }
        .finish()
        .await
        .map_err(DrasiError::from)
    }

    // Runs only in an admitted blocking worker. Initialization and the Core
    // future remain owned through cancellation without occupying Tokio's
    // current-thread executor. Every builder is bound before the first build.
    fn construct(
        mut self,
        owner: &ServerCleanupHandle,
        start: bool,
        only_primary: bool,
        legacy_result: bool,
    ) -> anyhow::Result<BuiltCores> {
        ensure!(
            !(self.telemetry_config.is_some() && self.telemetry_bindings.is_some()),
            "Configured telemetry and borrowed telemetry bindings are mutually exclusive"
        );
        ensure!(
            !legacy_result || self.telemetry_config.is_none(),
            "This result cannot retain Server-owned telemetry; use build, prepare or build_with_owned_handles"
        );
        let new_telemetry = self.telemetry_config.is_some() || self.telemetry_bindings.is_some();
        let port = self.port.unwrap_or(8080);
        let (bindings, legacy_logging) = if let Some(config) = self.telemetry_config {
            let effective = config.resolve(&DtoMapper::new(), port, self.enable_ui)?;
            effective.ensure_runtime_available()?;
            (
                Some(StandaloneTelemetry::initialize_configured(
                    owner, effective,
                )?),
                None,
            )
        } else if let Some(bindings) = self.telemetry_bindings {
            owner.retain_bindings(bindings.clone())?;
            (Some(bindings), None)
        } else {
            let logging = init_default_subscriber()
                .context("Cannot initialize or borrow Core's known default logging service")?;
            owner.retain_bindings(TelemetryBindings::new(
                TelemetrySettings::new(TelemetryProfile::Off),
                logging.clone(),
            ))?;
            (None, Some(logging))
        };
        if only_primary {
            self.core_builders.truncate(1);
        }
        let builders = self
            .core_builders
            .into_iter()
            .map(|builder| {
                if let Some(bindings) = &bindings {
                    bindings.apply_to(builder)
                } else if let Some(logging) = &legacy_logging {
                    Ok(builder.with_component_logging(logging.clone()))
                } else {
                    anyhow::bail!("Missing prepared component logging")
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        ensure!(
            !builders.is_empty(),
            "At least one Core builder is required"
        );
        let (cores, servers) = tokio::runtime::Handle::try_current()?.block_on(async {
            let mut cores = Vec::new();
            let mut servers = HashMap::new();
            for builder in builders {
                let core = Arc::new(builder.build().await.context("Core construction failed")?);
                owner.retain_core(core.clone())?;
                let id = core.get_config().id.clone();
                ensure!(
                    !servers.contains_key(&id),
                    "Duplicate Core instance ID '{id}'"
                );
                if start {
                    core.start().await.context("Core startup failed")?;
                }
                servers.insert(id, core.clone());
                cores.push(core);
            }
            Ok::<_, anyhow::Error>((cores, servers))
        })?;
        Ok(BuiltCores {
            cores,
            servers,
            enable_api: self.enable_api,
            enable_ui: self.enable_ui,
            host: self.host.unwrap_or_else(|| "127.0.0.1".into()),
            port,
            config_file_path: self.config_file_path,
            new_telemetry,
        })
    }
}

struct BuiltCores {
    cores: Vec<Arc<DrasiLib>>,
    servers: HashMap<String, Arc<DrasiLib>>,
    enable_api: bool,
    enable_ui: bool,
    host: String,
    port: u16,
    config_file_path: Option<String>,
    new_telemetry: bool,
}

impl BuiltCores {
    fn primary(&self) -> anyhow::Result<Arc<DrasiLib>> {
        self.cores.first().cloned().context("Missing primary Core")
    }

    fn into_server(self, cleanup: ServerCleanupHandle) -> DrasiServer {
        DrasiServer::from_prepared_cores(
            self.cores
                .into_iter()
                .map(|core| (core.as_ref().clone(), None))
                .collect(),
            self.enable_api,
            self.enable_ui,
            self.host,
            self.port,
            self.config_file_path,
            cleanup,
            self.new_telemetry,
        )
    }
}

#[cfg(test)]
#[path = "builder/telemetry_tests.rs"]
mod telemetry_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder =
            DrasiServerBuilder::new().with_query_factory(crate::query_test_support::factory());
        assert_eq!(builder.host, Some("127.0.0.1".to_string()));
        assert_eq!(builder.port, Some(8080));
        assert!(!builder.enable_api);
    }

    #[test]
    fn test_builder_fluent_api() {
        let builder = DrasiServerBuilder::new()
            .with_query_factory(crate::query_test_support::factory())
            .with_simple_query(
                "test_query",
                "MATCH (n) RETURN n",
                vec!["test_source".to_string()],
            )
            .unwrap()
            .with_port(9090);

        assert!(builder.enable_api);
        assert_eq!(builder.port, Some(9090));
    }
}
