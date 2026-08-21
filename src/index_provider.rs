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

//! Persistent (RocksDB) index provider wiring shared across the server.
//!
//! This module is the single home for the persistent index provider name and
//! for constructing/registering the RocksDB provider, so that server startup
//! (`server.rs`) and the create-instance API handler (`instance_handlers.rs`)
//! stay in sync. The builder imports the name from here rather than the other
//! way around, keeping the dependency direction sensible.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use drasi_index_rocksdb::RocksDbIndexProvider;
use drasi_lib::DrasiLibBuilder;
use log::info;

use crate::instance_paths::instance_storage_key;

/// Name under which drasi-server registers its persistent (RocksDB) index
/// provider when `persist_index` is enabled.
///
/// Queries with no explicit `storageBackend` are backed by this provider via
/// [`DrasiLibBuilder::with_default_index_provider`], and per-query
/// `storageBackend` overrides that reference a named provider must use this
/// same name.
pub const PERSISTENT_INDEX_PROVIDER_NAME: &str = "rocksdb";

const BYTES_PER_MIB: usize = 1024 * 1024;

/// Validate an optional per-instance RocksDB memory budget and convert it to bytes.
pub(crate) fn memory_budget_bytes(
    persist_index: bool,
    memory_budget_mib: Option<usize>,
) -> Result<Option<usize>> {
    let Some(memory_budget_mib) = memory_budget_mib else {
        return Ok(None);
    };

    if !persist_index {
        bail!("memoryBudgetMiB requires persistIndex: true");
    }
    if memory_budget_mib == 0 {
        bail!("memoryBudgetMiB must be greater than zero");
    }

    memory_budget_mib
        .checked_mul(BYTES_PER_MIB)
        .map(Some)
        .context("memoryBudgetMiB is too large")
}

/// Compute the on-disk RocksDB index directory for an instance.
///
/// The instance id is converted to a filesystem-safe, path-traversal-safe
/// storage key via [`instance_storage_key`], shared with the WAL directory so
/// both live under the same `./data/<storage-key>/` parent.
pub(crate) fn instance_index_dir(instance_id: &str) -> PathBuf {
    let safe_id = instance_storage_key(instance_id);
    PathBuf::from(format!("./data/{safe_id}/index"))
}

/// Register the persistent RocksDB index provider as the instance default on
/// `builder`.
///
/// Centralizes the id sanitization, path construction, and provider wiring used
/// by both server startup and the create-instance API handler. Every query in
/// the instance without an explicit `storageBackend` is persisted to
/// `./data/<instance-key>/index` (see [`instance_index_dir`]).
pub(crate) fn apply_rocksdb_index(
    builder: DrasiLibBuilder,
    instance_id: &str,
    enable_archive: bool,
    memory_budget_bytes: Option<usize>,
) -> Result<DrasiLibBuilder> {
    let index_path = instance_index_dir(instance_id);
    let direct_io = false; // use OS page cache
    let mut provider = RocksDbIndexProvider::new(index_path, enable_archive, direct_io);
    if let Some(memory_budget_bytes) = memory_budget_bytes {
        provider = provider
            .with_memory_budget_bytes(memory_budget_bytes)
            .context("invalid RocksDB memory budget")?;
    }

    match memory_budget_bytes {
        Some(memory_budget_bytes) => info!(
            "Enabling persistent indexing for instance '{instance_id}' with RocksDB at: {} \
             (archive: {enable_archive}, memory budget: {} MiB)",
            provider.path().display(),
            memory_budget_bytes / BYTES_PER_MIB,
        ),
        None => info!(
            "Enabling persistent indexing for instance '{instance_id}' with RocksDB at: {} \
             (archive: {enable_archive}, default memory budget)",
            provider.path().display(),
        ),
    }

    Ok(builder.with_default_index_provider(PERSISTENT_INDEX_PROVIDER_NAME, Arc::new(provider)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_memory_budget_mib_to_bytes() {
        assert_eq!(
            memory_budget_bytes(true, Some(512)).unwrap(),
            Some(512 * BYTES_PER_MIB)
        );
    }

    #[test]
    fn rejects_memory_budget_without_persistent_index() {
        let error = memory_budget_bytes(false, Some(512)).unwrap_err();
        assert!(error
            .to_string()
            .contains("memoryBudgetMiB requires persistIndex: true"));
    }

    #[test]
    fn rejects_zero_memory_budget() {
        let error = memory_budget_bytes(true, Some(0)).unwrap_err();
        assert!(error
            .to_string()
            .contains("memoryBudgetMiB must be greater than zero"));
    }

    #[test]
    fn rejects_memory_budget_overflow() {
        let error = memory_budget_bytes(true, Some(usize::MAX)).unwrap_err();
        assert!(error.to_string().contains("memoryBudgetMiB is too large"));
    }
}
