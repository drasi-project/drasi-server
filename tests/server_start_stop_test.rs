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

//! Server start/stop tests
//!
//! These tests verify the basic server lifecycle operations.
//!
//! Note: Sources and reactions must be provided as instances when building DrasiLib.
//! Dynamic creation via config is not supported.

mod test_support;

use anyhow::Result;
use drasi_lib::Query;
use drasi_server::DrasiLib;
use std::sync::Arc;
use test_support::mock_components::create_mock_source;

#[tokio::test]
async fn test_server_start_stop_cycle() -> Result<()> {
    // Create a minimal runtime config
    let server_id = uuid::Uuid::new_v4().to_string();

    // Create a mock source instance
    let test_source = create_mock_source("test-source");

    // Build the core using the new builder API
    let core = DrasiLib::builder()
        .with_id(&server_id)
        .with_source(test_source)
        .build()
        .await?;

    // Convert to Arc for repeated use
    let core = Arc::new(core);

    // Server should not be running initially
    assert!(!core.is_running().await);

    // Start the server
    core.start().await?;
    assert!(core.is_running().await);

    // Try to start again (should fail)
    assert!(core.start().await.is_err());

    // Stop the server
    core.stop().await?;
    assert!(!core.is_running().await);

    // Try to stop again (should fail)
    assert!(core.stop().await.is_err());

    // Start again
    core.start().await?;
    assert!(core.is_running().await);

    // Stop again
    core.stop().await?;
    assert!(!core.is_running().await);

    Ok(())
}

#[tokio::test]
async fn test_server_with_query() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Create source instance
    let test_source = create_mock_source("test-source");

    // Build the core with a query
    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id(&server_id)
        .with_source(test_source)
        .with_query(query)
        .build()
        .await?;

    let core = Arc::new(core);

    // Server should not be running initially
    assert!(!core.is_running().await);

    // Start the server
    core.start().await?;
    assert!(core.is_running().await);

    // Stop the server
    core.stop().await?;
    assert!(!core.is_running().await);

    Ok(())
}
