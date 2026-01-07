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

//! API State Consistency Tests
//!
//! These tests ensure API operations maintain consistent state across components,
//! testing the public API for component lifecycle management.
//!
//! Note: Sources and reactions must be provided as instances when building DrasiLib.
//! Dynamic creation via config is not supported.

mod test_support;

use test_support::{create_mock_reaction, create_mock_source};

use drasi_lib::{DrasiLib, Query};
use std::sync::Arc;

#[tokio::test]
async fn test_server_start_stop_cycle() {
    let test_source = create_mock_source("test-source");

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(test_source)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Initially not running
    assert!(!core.is_running().await);

    // Start server
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);

    // Can start again
    core.start().await.expect("Failed to restart");
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_components_with_auto_start() {
    let test_source = create_mock_source("test-source");
    let test_reaction = create_mock_reaction("test-reaction", vec!["test-query".to_string()]);

    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(test_source)
        .with_reaction(test_reaction)
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server - components should auto-start
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Components should be running (we can't check individual status through public API)
    assert!(core.is_running().await);

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);
}

#[tokio::test]
async fn test_components_without_auto_start() {
    let test_source = create_mock_source("test-source");
    let test_reaction = create_mock_reaction("test-reaction", vec!["test-query".to_string()]);

    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(false)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(test_source)
        .with_reaction(test_reaction)
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server - components should NOT auto-start
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Components exist but are not started (we can't check individual status through public API)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Server should still be running
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_restart_with_components() {
    let restart_source = create_mock_source("restart-source");

    let query = Query::cypher("restart-query")
        .query("MATCH (n) RETURN n")
        .from_source("restart-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(restart_source)
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components start
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);

    // Restart server
    core.start().await.expect("Failed to restart");
    assert!(core.is_running().await);

    // Let components restart
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Server should still be running after restart
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_multiple_query_sources() {
    let source1 = create_mock_source("source1");
    let source2 = create_mock_source("source2");

    let query = Query::cypher("multi-source-query")
        .query("MATCH (n) RETURN n")
        .from_source("source1")
        .from_source("source2")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(source1)
        .with_source(source2)
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with multiple sources
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_multiple_reaction_queries() {
    let test_source = create_mock_source("test-source");
    let multi_query_reaction = create_mock_reaction(
        "multi-query-reaction",
        vec!["query1".to_string(), "query2".to_string()],
    );

    let query1 = Query::cypher("query1")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();
    let query2 = Query::cypher("query2")
        .query("MATCH (m) RETURN m")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(test_source)
        .with_reaction(multi_query_reaction)
        .with_query(query1)
        .with_query(query2)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with reaction subscribing to multiple queries
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_query_with_joins() {
    let join_source1 = create_mock_source("join-source1");
    let join_source2 = create_mock_source("join-source2");

    // For joins, we need to use the lower-level QueryConfig since the builder API
    // may not support join configuration yet
    use drasi_lib::config::{QueryJoinConfig, QueryJoinKeyConfig};
    let query = Query::cypher("join-query")
        .query("MATCH (a:TypeA)<-[:LINKED]-(b:TypeB) RETURN a, b")
        .from_source("join-source1")
        .from_source("join-source2")
        .auto_start(true)
        .with_joins(vec![QueryJoinConfig {
            id: "LINKED".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "TypeA".to_string(),
                    property: "id".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "TypeB".to_string(),
                    property: "type_a_id".to_string(),
                },
            ],
        }])
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(join_source1)
        .with_source(join_source2)
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with query that has joins
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(core.is_running().await);

    core.stop().await.ok();
}
