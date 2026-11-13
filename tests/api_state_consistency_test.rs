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

use drasi_lib::{channels::ComponentStatus, ComponentName, Query};
use std::sync::Arc;

#[tokio::test]
async fn test_server_start_stop_cycle() {
    let test_source = create_mock_source("test-source");

    let core = crate::query_plugin::builder()
        .with_id("test-server")
        .with_source(ComponentName::try_new("test-source").unwrap(), test_source)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Initially not running
    assert!(!core.is_running().await);

    // Start server
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Wait for source to reach Running before stopping
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("test-source").unwrap(),
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("test-source should reach Running");

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);

    // Wait for source to reach Stopped before restarting
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("test-source").unwrap(),
        &[drasi_lib::channels::ComponentStatus::Stopped],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("test-source should reach Stopped");

    // Can start again
    core.start().await.expect("Failed to restart");
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_components_with_auto_start() {
    let test_source = create_mock_source("test-source");
    let test_reaction = create_mock_reaction("test-reaction", vec!["test-query".to_string()]);

    let query = Query::cypher(ComponentName::try_new("test-query").unwrap())
        .query("MATCH (n) RETURN n")
        .from_input_by_name(ComponentName::try_new("test-source").unwrap())
        .auto_start(true)
        .build()
        .unwrap();

    let core = crate::query_plugin::builder()
        .with_id("test-server")
        .with_source(ComponentName::try_new("test-source").unwrap(), test_source)
        .with_reaction(
            ComponentName::try_new("test-reaction").unwrap(),
            test_reaction,
        )
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server - components should auto-start
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Wait for source to reach Running
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("test-source").unwrap(),
        &[ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("test-source should reach Running");

    // Components should be running (we can't check individual status through public API)
    assert!(core.is_running().await);

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);
}

#[tokio::test]
async fn test_components_without_auto_start() {
    let test_source = create_mock_source("test-source").with_auto_start(false);
    let test_reaction = create_mock_reaction("test-reaction", vec!["test-query".to_string()])
        .with_auto_start(false);

    let query = Query::cypher(ComponentName::try_new("test-query").unwrap())
        .query("MATCH (n) RETURN n")
        .from_input_by_name(ComponentName::try_new("test-source").unwrap())
        .auto_start(false)
        .build()
        .unwrap();

    let core = crate::query_plugin::builder()
        .with_id("test-server")
        .with_source(ComponentName::try_new("test-source").unwrap(), test_source)
        .with_reaction(
            ComponentName::try_new("test-reaction").unwrap(),
            test_reaction,
        )
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server - components should NOT auto-start
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    for id in ["test-source", "test-query", "test-reaction"] {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            core.component_by_name(&ComponentName::try_new(id).unwrap())
                .unwrap()
                .wait_created(),
        )
        .await
        .unwrap()
        .unwrap();
    }

    // Never-started components retain the Added presentation after native creation.
    assert_eq!(
        core.get_source_status_by_name(&ComponentName::try_new("test-source").unwrap())
            .await
            .expect("source status"),
        ComponentStatus::Added
    );
    assert_eq!(
        core.get_query_status_by_name(&ComponentName::try_new("test-query").unwrap())
            .await
            .expect("query status"),
        ComponentStatus::Added
    );
    assert_eq!(
        core.get_reaction_status_by_name(&ComponentName::try_new("test-reaction").unwrap())
            .await
            .expect("reaction status"),
        ComponentStatus::Added
    );

    // Server should still be running
    assert!(core.is_running().await);
    assert_eq!(
        core.get_query_status_by_name(&ComponentName::try_new("test-query").unwrap())
            .await
            .unwrap(),
        ComponentStatus::Added
    );
    assert_eq!(
        core.get_reaction_status_by_name(&ComponentName::try_new("test-reaction").unwrap())
            .await
            .unwrap(),
        ComponentStatus::Added
    );

    core.stop().await.ok();
}

#[tokio::test]
async fn test_restart_with_components() {
    let restart_source = create_mock_source("restart-source");

    let query = Query::cypher(ComponentName::try_new("restart-query").unwrap())
        .query("MATCH (n) RETURN n")
        .from_input_by_name(ComponentName::try_new("restart-source").unwrap())
        .auto_start(true)
        .build()
        .unwrap();

    let core = crate::query_plugin::builder()
        .with_id("test-server")
        .with_source(
            ComponentName::try_new("restart-source").unwrap(),
            restart_source,
        )
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Wait for source to reach Running
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("restart-source").unwrap(),
        &[ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("restart-source should reach Running");

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);

    // Wait for source to reach Stopped before restarting
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("restart-source").unwrap(),
        &[ComponentStatus::Stopped],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("restart-source should reach Stopped");

    // Restart server
    core.start().await.expect("Failed to restart");
    assert!(core.is_running().await);

    // Wait for source to reach Running after restart
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("restart-source").unwrap(),
        &[ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("restart-source should reach Running after restart");

    // Server should still be running after restart
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_multiple_query_sources() {
    let source1 = create_mock_source("source1");
    let source2 = create_mock_source("source2");

    let query = Query::cypher(ComponentName::try_new("multi-source-query").unwrap())
        .query("MATCH (n) RETURN n")
        .from_input_by_name(ComponentName::try_new("source1").unwrap())
        .from_input_by_name(ComponentName::try_new("source2").unwrap())
        .auto_start(true)
        .build()
        .unwrap();

    let core = crate::query_plugin::builder()
        .with_id("test-server")
        .with_source(ComponentName::try_new("source1").unwrap(), source1)
        .with_source(ComponentName::try_new("source2").unwrap(), source2)
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with multiple sources
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Wait for sources to reach Running
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("source1").unwrap(),
        &[ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("source1 should reach Running");

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

    let query1 = Query::cypher(ComponentName::try_new("query1").unwrap())
        .query("MATCH (n) RETURN n")
        .from_input_by_name(ComponentName::try_new("test-source").unwrap())
        .auto_start(true)
        .build()
        .unwrap();
    let query2 = Query::cypher(ComponentName::try_new("query2").unwrap())
        .query("MATCH (m) RETURN m")
        .from_input_by_name(ComponentName::try_new("test-source").unwrap())
        .auto_start(true)
        .build()
        .unwrap();

    let core = crate::query_plugin::builder()
        .with_id("test-server")
        .with_source(ComponentName::try_new("test-source").unwrap(), test_source)
        .with_reaction(
            ComponentName::try_new("multi-query-reaction").unwrap(),
            multi_query_reaction,
        )
        .with_query(query1)
        .with_query(query2)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with reaction subscribing to multiple queries
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Wait for source to reach Running
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("test-source").unwrap(),
        &[ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("test-source should reach Running");

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
    let query = Query::cypher(ComponentName::try_new("join-query").unwrap())
        .query("MATCH (a:TypeA)<-[:LINKED]-(b:TypeB) RETURN a, b")
        .from_input_by_name(ComponentName::try_new("join-source1").unwrap())
        .from_input_by_name(ComponentName::try_new("join-source2").unwrap())
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
        .build()
        .unwrap();

    let core = crate::query_plugin::builder()
        .with_id("test-server")
        .with_source(
            ComponentName::try_new("join-source1").unwrap(),
            join_source1,
        )
        .with_source(
            ComponentName::try_new("join-source2").unwrap(),
            join_source2,
        )
        .with_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with query that has joins
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Wait for source to reach Running
    drasi_lib::wait_for_status_by_name(
        &core.component_graph(),
        &ComponentName::try_new("join-source1").unwrap(),
        &[ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("join-source1 should reach Running");

    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[path = "query_plugin/mod.rs"]
mod query_plugin;
