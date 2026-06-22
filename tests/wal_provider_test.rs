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

//! Integration tests for the redb-backed WAL provider.
//!
//! The WAL provider is the primary feature added in this PR and is wired into
//! every instance, so these tests verify:
//! - `RedbWalProvider` can be created from a directory path without panicking
//! - `DrasiLib::builder().with_wal_provider(...)` accepts the provider and the
//!   instance reaches Running and stops cleanly
//!
//! Per-source WAL file/directory creation is exercised by the `drasi-wal-redb`
//! crate's own tests, since it only happens once a source appends events.

use anyhow::Result;
use drasi_lib::DrasiLib;
use drasi_wal_redb::RedbWalProvider;
use std::sync::Arc;
use tempfile::TempDir;

/// Test that RedbWalProvider can be created with a valid directory path.
#[test]
fn test_redb_wal_provider_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().join("wal");

    let provider = RedbWalProvider::new(&wal_path);

    // Provider should be created successfully.
    drop(provider);
}

/// Test that DrasiLib builder accepts a redb WAL provider and the instance
/// starts and stops cleanly.
#[tokio::test]
async fn test_drasi_lib_builder_with_redb_wal_provider() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("wal");

    let provider = RedbWalProvider::new(&wal_path);

    let core = DrasiLib::builder()
        .with_id("test-wal-provider")
        .with_wal_provider(Arc::new(provider))
        .build()
        .await?;

    core.start().await?;
    assert!(core.is_running().await);
    drasi_lib::wait_for_status(
        &core.component_graph(),
        "__component_graph__",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("component graph should reach Running");

    core.stop().await?;
    assert!(!core.is_running().await);

    Ok(())
}
