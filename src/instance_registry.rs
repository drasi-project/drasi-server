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
use std::sync::Arc;
use tokio::sync::RwLock;

use drasi_lib::DrasiLib;

/// Thread-safe registry for managing DrasiLib instances.
///
/// Supports dynamic instance creation and lookup at runtime.
#[derive(Clone)]
pub struct InstanceRegistry {
    instances: Arc<RwLock<IndexMap<String, Arc<DrasiLib>>>>,
}

impl InstanceRegistry {
    /// Create a new empty instance registry.
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(IndexMap::new())),
        }
    }

    /// Create a registry from an existing instance map.
    pub fn from_map(instances: IndexMap<String, Arc<DrasiLib>>) -> Self {
        Self {
            instances: Arc::new(RwLock::new(instances)),
        }
    }

    /// Get an instance by ID.
    pub async fn get(&self, id: &str) -> Option<Arc<DrasiLib>> {
        let instances = self.instances.read().await;
        instances.get(id).cloned()
    }

    /// Get the first (default) instance.
    pub async fn get_default(&self) -> Option<(String, Arc<DrasiLib>)> {
        let instances = self.instances.read().await;
        instances.iter().next().map(|(k, v)| (k.clone(), v.clone()))
    }

    /// List all instance IDs.
    pub async fn list_ids(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        instances.keys().cloned().collect()
    }

    /// List all instances as (id, instance) pairs.
    pub async fn list(&self) -> Vec<(String, Arc<DrasiLib>)> {
        let instances = self.instances.read().await;
        instances.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
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
            return Err(format!("Instance '{}' already exists", id));
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
}

impl Default for InstanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
