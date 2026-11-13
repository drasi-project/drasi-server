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

//! Instance registry for managing DrasiLib instances dynamically.
//!
//! This module provides thread-safe access to DrasiLib instances,
//! allowing dynamic creation and deletion of instances at runtime.

use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use drasi_lib::DrasiLib;

use crate::api::models::BootstrapProviderConfig;
use crate::{
    server::{lifecycle::ManagementRequest, ServerCleanupHandle},
    TelemetryBindings,
};

#[derive(Clone)]
struct RuntimeContext {
    bindings: Option<TelemetryBindings>,
    cleanup: ServerCleanupHandle,
}

/// Thread-safe registry for managing DrasiLib instances.
///
/// Supports dynamic instance creation and lookup at runtime.
#[derive(Clone)]
pub struct InstanceRegistry {
    instances: Arc<RwLock<IndexMap<String, Arc<DrasiLib>>>>,
    /// Per-instance top-level bootstrap provider configs, keyed by
    /// `instance_id` then by bootstrap provider `id`. Populated at startup
    /// from each instance's `bootstrapProviders`. Consulted by the source
    /// create/upsert handlers so a source created at runtime that references
    /// a top-level bootstrap provider (`bootstrapProvider: <id>`) can be
    /// resolved and wired live.
    bootstrap_providers: Arc<RwLock<IndexMap<String, HashMap<String, BootstrapProviderConfig>>>>,
    configuration_stores: crate::managed_configuration::ConfigurationStores,
    runtime: Option<Arc<RuntimeContext>>,
}

impl InstanceRegistry {
    pub(crate) fn metrics_reader(
        &self,
    ) -> impl Fn() -> anyhow::Result<Vec<(String, Arc<DrasiLib>)>> + Send + Sync + 'static {
        let instances = Arc::downgrade(&self.instances);
        move || {
            let Some(instances) = instances.upgrade() else {
                return Ok(Vec::new());
            };
            let instances = instances.try_read().map_err(|_| {
                anyhow::anyhow!("instance registry is busy; metrics sample omitted")
            })?;
            Ok(instances
                .iter()
                .map(|(id, core)| (id.clone(), core.clone()))
                .collect())
        }
    }

    pub(crate) fn same_instances(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.instances, &other.instances)
    }

    /// Create a new empty instance registry.
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(IndexMap::new())),
            bootstrap_providers: Arc::new(RwLock::new(IndexMap::new())),
            configuration_stores: Default::default(),
            runtime: None,
        }
    }

    /// Create a registry from an existing instance map.
    pub fn from_map(instances: IndexMap<String, Arc<DrasiLib>>) -> Self {
        Self {
            instances: Arc::new(RwLock::new(instances)),
            bootstrap_providers: Arc::new(RwLock::new(IndexMap::new())),
            configuration_stores: Default::default(),
            runtime: None,
        }
    }

    pub(crate) fn with_configuration_stores(
        mut self,
        stores: crate::managed_configuration::ConfigurationStores,
    ) -> Self {
        self.configuration_stores = stores;
        self
    }

    pub(crate) fn with_runtime_context(
        mut self,
        bindings: TelemetryBindings,
        cleanup: ServerCleanupHandle,
    ) -> Self {
        self.runtime = Some(Arc::new(RuntimeContext { bindings: Some(bindings), cleanup }));
        self
    }

    pub(crate) fn with_cleanup_context(mut self, cleanup: ServerCleanupHandle) -> Self {
        self.runtime = Some(Arc::new(RuntimeContext { bindings: None, cleanup }));
        self
    }

    pub(crate) fn bind_builder(
        &self,
        builder: drasi_lib::DrasiLibBuilder,
    ) -> anyhow::Result<drasi_lib::DrasiLibBuilder> {
        match self.runtime.as_ref().and_then(|context| context.bindings.as_ref()) {
            Some(bindings) => bindings.apply_to(builder),
            None => Ok(builder),
        }
    }

    pub(crate) fn cleanup_handle(&self) -> Option<ServerCleanupHandle> {
        self.runtime.as_ref().map(|context| context.cleanup.clone())
    }

    pub(crate) fn plugin_log_registry(
        &self,
    ) -> Option<Arc<drasi_lib::observability::ComponentLogRegistry>> {
        self.runtime.as_ref()?.bindings.as_ref().map(|bindings| bindings.logging().registry())
    }

    pub(crate) async fn publish_instance(
        &self,
        id: String,
        instance: Arc<DrasiLib>,
    ) -> anyhow::Result<()> {
        let mut instances = self.instances.write().await;
        let publish = || {
            anyhow::ensure!(!instances.contains_key(&id), "Instance already exists");
            instances.insert(id, instance);
            Ok(())
        };
        if let Some(context) = &self.runtime {
            if let Some(request) = ManagementRequest::current()
                .filter(|request| request.belongs_to(&context.cleanup))
            {
                request.publish(publish)
            } else {
                context.cleanup.publish(publish)
            }
        } else {
            publish()
        }
    }

    pub(crate) fn configuration_stores(
        &self,
    ) -> &crate::managed_configuration::ConfigurationStores {
        &self.configuration_stores
    }

    /// Get an instance by ID.
    pub async fn get(&self, id: &str) -> Option<Arc<DrasiLib>> {
        let instances = self.instances.read().await;
        instances.get(id).cloned()
    }

    /// List all instance IDs.
    pub async fn list_ids(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        instances.keys().cloned().collect()
    }

    /// List all instances as (id, instance) pairs.
    pub async fn list(&self) -> Vec<(String, Arc<DrasiLib>)> {
        let instances = self.instances.read().await;
        instances
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Check if an instance exists.
    pub async fn contains(&self, id: &str) -> bool {
        let instances = self.instances.read().await;
        instances.contains_key(id)
    }

    /// Add a new instance to the registry.
    ///
    /// Returns an error if an instance with the same ID already exists.
    pub async fn add(&self, id: String, instance: Arc<DrasiLib>) -> Result<(), String> {
        let mut instances = self.instances.write().await;
        if instances.contains_key(&id) {
            return Err(format!("Instance '{id}' already exists"));
        }
        instances.insert(id, instance);
        Ok(())
    }

    /// Remove an instance from the registry.
    ///
    /// Returns the removed instance if it existed.
    pub async fn remove(&self, id: &str) -> Option<Arc<DrasiLib>> {
        let mut instances = self.instances.write().await;
        instances.shift_remove(id)
    }

    /// Get the number of instances.
    pub async fn len(&self) -> usize {
        let instances = self.instances.read().await;
        instances.len()
    }

    /// Check if the registry is empty.
    pub async fn is_empty(&self) -> bool {
        let instances = self.instances.read().await;
        instances.is_empty()
    }

    /// Record the top-level bootstrap provider configs for an instance.
    ///
    /// Keyed by bootstrap provider `id`. Overwrites any existing entry for
    /// the instance. Called at startup once the instance's configured
    /// `bootstrapProviders` have been collected.
    pub async fn set_bootstrap_providers(
        &self,
        instance_id: String,
        providers: HashMap<String, BootstrapProviderConfig>,
    ) {
        let mut map = self.bootstrap_providers.write().await;
        map.insert(instance_id, providers);
    }

    /// Get the top-level bootstrap provider configs for an instance (keyed by
    /// provider `id`). Returns an empty map when the instance declared none
    /// (including dynamically created instances).
    pub async fn bootstrap_providers(
        &self,
        instance_id: &str,
    ) -> HashMap<String, BootstrapProviderConfig> {
        let map = self.bootstrap_providers.read().await;
        map.get(instance_id).cloned().unwrap_or_default()
    }
}

impl Default for InstanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
