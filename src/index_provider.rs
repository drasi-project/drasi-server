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

use drasi_index_rocksdb::RocksDbIndexProvider;
use drasi_lib::DrasiLibBuilder;
use log::info;

/// Name under which drasi-server registers its persistent (RocksDB) index
/// provider when `persist_index` is enabled.
///
/// Queries with no explicit `storageBackend` are backed by this provider via
/// [`DrasiLibBuilder::with_default_index_provider`], and per-query
/// `storageBackend` overrides that reference a named provider must use this
/// same name.
pub const PERSISTENT_INDEX_PROVIDER_NAME: &str = "rocksdb";

/// Compute the on-disk RocksDB index directory for an instance.
///
/// The instance id is sanitized for filesystem safety: `/`, `\`, and `..` are
/// each replaced with `_` so the path cannot escape `./data`.
pub(crate) fn instance_index_dir(instance_id: &str) -> PathBuf {
    let safe_id = instance_id.replace(['/', '\\'], "_").replace("..", "_");
    PathBuf::from(format!("./data/{safe_id}/index"))
}

/// Register the persistent RocksDB index provider as the instance default on
/// `builder`.
///
/// Centralizes the id sanitization, path construction, and provider wiring used
/// by both server startup and the create-instance API handler. Every query in
/// the instance without an explicit `storageBackend` is persisted to
/// `./data/<instanceId>/index` (see [`instance_index_dir`]).
pub(crate) fn apply_rocksdb_index(builder: DrasiLibBuilder, instance_id: &str) -> DrasiLibBuilder {
    let index_path = instance_index_dir(instance_id);
    info!(
        "Enabling persistent indexing for instance '{instance_id}' with RocksDB at: {}",
        index_path.display()
    );
    let provider = RocksDbIndexProvider::new(
        index_path, true,  // enable_archive - support for past() function
        false, // direct_io - use OS page cache
    );
    builder.with_default_index_provider(PERSISTENT_INDEX_PROVIDER_NAME, Arc::new(provider))
}
