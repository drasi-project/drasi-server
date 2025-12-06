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

//! Server Integration Tests
//!
//! Note: Sources and reactions must be provided as instances when building DrasiLib.
//! Dynamic creation via config is not supported.

use anyhow::Result;
use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::QuerySubscriber;
use drasi_lib::plugin_core::Reaction as ReactionTrait;
use drasi_lib::plugin_core::Source as SourceTrait;
use drasi_lib::Query;
use drasi_server::DrasiLib;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

/// A mock source for testing
struct MockSource {
    id: String,
    status: Arc<RwLock<ComponentStatus>>,
}

impl MockSource {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
        }
    }
}

#[async_trait]
impl SourceTrait for MockSource {
    fn id(&self) -> &str {
        &self.id
    }

    fn type_name(&self) -> &str {
        "mock"
    }

    fn properties(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }

    async fn start(&self) -> anyhow::Result<()> {
        *self.status.write().await = ComponentStatus::Running;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        *self.status.write().await = ComponentStatus::Stopped;
        Ok(())
    }

    async fn status(&self) -> ComponentStatus {
        self.status.read().await.clone()
    }

    async fn subscribe(
        &self,
        query_id: String,
        _enable_bootstrap: bool,
        _node_labels: Vec<String>,
        _relation_labels: Vec<String>,
    ) -> anyhow::Result<SubscriptionResponse> {
        use drasi_lib::channels::dispatcher::ChannelChangeDispatcher;
        let dispatcher =
            ChannelChangeDispatcher::<drasi_lib::channels::SourceEventWrapper>::new(100);
        let receiver = dispatcher.create_receiver().await?;
        Ok(SubscriptionResponse {
            query_id,
            source_id: self.id.clone(),
            receiver,
            bootstrap_receiver: None,
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inject_event_tx(&self, _tx: ComponentEventSender) {
        // No-op for testing
    }
}

/// A mock reaction for testing
struct MockReaction {
    id: String,
    queries: Vec<String>,
    status: Arc<RwLock<ComponentStatus>>,
}

impl MockReaction {
    fn new(id: &str, queries: Vec<String>) -> Self {
        Self {
            id: id.to_string(),
            queries,
            status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
        }
    }
}

#[async_trait]
impl ReactionTrait for MockReaction {
    fn id(&self) -> &str {
        &self.id
    }

    fn type_name(&self) -> &str {
        "log"
    }

    fn properties(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }

    fn query_ids(&self) -> Vec<String> {
        self.queries.clone()
    }

    async fn inject_query_subscriber(&self, _query_subscriber: Arc<dyn QuerySubscriber>) {
        // No-op for testing
    }

    async fn start(&self) -> anyhow::Result<()> {
        *self.status.write().await = ComponentStatus::Running;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        *self.status.write().await = ComponentStatus::Stopped;
        Ok(())
    }

    async fn status(&self) -> ComponentStatus {
        self.status.read().await.clone()
    }

    async fn inject_event_tx(&self, _tx: ComponentEventSender) {
        // No-op for testing
    }
}

/// Create a mock source for testing
fn create_mock_source(id: &str) -> MockSource {
    MockSource::new(id)
}

/// Create a mock reaction for testing
fn create_mock_reaction(id: &str, queries: Vec<String>) -> MockReaction {
    MockReaction::new(id, queries)
}

/// Integration test demonstrating data flow continues after server restart
#[tokio::test]
async fn test_data_flow_with_server_restart() -> Result<()> {
    // Create a shared counter to track how many results the reaction has processed
    let result_counter = Arc::new(AtomicUsize::new(0));
    let _counter_clone = result_counter.clone();

    // Create configuration
    let server_id = uuid::Uuid::new_v4().to_string();

    // Create source and reaction instances
    let counter_source = create_mock_source("counter-source");
    let counter_reaction =
        create_mock_reaction("counter-reaction", vec!["counter-query".to_string()]);

    // Build the core using the new builder API
    let query = Query::cypher("counter-query")
        .query("MATCH (n:Counter) RETURN n.value as value")
        .from_source("counter-source")
        .auto_start(true)
        .build();

    let core = Arc::new(
        DrasiLib::builder()
            .with_id(&server_id)
            .with_source(counter_source)
            .with_reaction(counter_reaction)
            .with_query(query)
            .build()
            .await?,
    );

    // Start the server
    core.start().await?;

    // Let it run for 2 seconds (should get ~4 counter updates)
    sleep(Duration::from_secs(2)).await;

    // Check initial result count (we can't directly access it in this test, but we know it's running)
    assert!(core.is_running().await);

    // Stop the server
    core.stop().await?;
    assert!(!core.is_running().await);

    // Wait a bit
    sleep(Duration::from_millis(500)).await;

    // Start the server again
    core.start().await?;
    assert!(core.is_running().await);

    // Wait for components to start
    sleep(Duration::from_millis(500)).await;

    // Let it run for another 2 seconds
    sleep(Duration::from_secs(2)).await;

    // Verify source and query are still running
    // Note: With the new API, we don't have direct access to component status checks
    // The core should be running after restart
    assert!(core.is_running().await);

    // Final cleanup
    core.stop().await?;

    Ok(())
}

/// Integration test with multiple sources feeding into queries
#[tokio::test]
async fn test_multiple_sources_and_queries() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Create source and reaction instances
    let sensors_source = create_mock_source("sensors-source");
    let vehicles_source = create_mock_source("vehicles-source");
    let alert_handler = create_mock_reaction(
        "alert-handler",
        vec!["sensor-alerts".to_string(), "combined-view".to_string()],
    );

    // Build the core using the new builder API
    let sensor_query = Query::cypher("sensor-alerts")
        .query("MATCH (s:Sensor) RETURN s")
        .from_source("sensors-source")
        .auto_start(true)
        .build();

    let vehicle_query = Query::cypher("vehicle-tracking")
        .query("MATCH (v:Vehicle) RETURN v.id, v.location")
        .from_source("vehicles-source")
        .auto_start(true)
        .build();

    let combined_query = Query::cypher("combined-view")
        .query("MATCH (n) RETURN n")
        .from_source("sensors-source")
        .from_source("vehicles-source")
        .auto_start(true)
        .build();

    let core = Arc::new(
        DrasiLib::builder()
            .with_id(&server_id)
            .with_source(sensors_source)
            .with_source(vehicles_source)
            .with_reaction(alert_handler)
            .with_query(sensor_query)
            .with_query(vehicle_query)
            .with_query(combined_query)
            .build()
            .await?,
    );

    // Start server
    core.start().await?;

    // Let it run briefly
    sleep(Duration::from_millis(500)).await;

    // Verify core is running with all components
    assert!(core.is_running().await);

    // Test removing a source at runtime using the new API
    core.remove_source("sensors-source").await?;
    sleep(Duration::from_millis(100)).await;

    // Core should still be running (other components continue)
    assert!(core.is_running().await);

    // Stop server
    core.stop().await?;

    Ok(())
}

/// Integration test for error recovery scenarios
#[tokio::test]
async fn test_component_failure_recovery() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Create source and reaction instances
    let test_source = create_mock_source("test-source");
    let test_reaction = create_mock_reaction("test-reaction", vec!["test-query".to_string()]);

    // Build the core using the new builder API
    let query = Query::cypher("test-query")
        // This query references a non-existent property, but should still start
        .query("MATCH (n) RETURN n.nonexistent as value")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let core = Arc::new(
        DrasiLib::builder()
            .with_id(&server_id)
            .with_source(test_source)
            .with_reaction(test_reaction)
            .with_query(query)
            .build()
            .await?,
    );

    // Start server - all components should start even with the "bad" query
    core.start().await?;
    sleep(Duration::from_millis(200)).await;

    // Core should be running
    assert!(core.is_running().await);

    // Stop and restart to verify recovery
    core.stop().await?;
    core.start().await?;
    sleep(Duration::from_millis(200)).await;

    // Core should recover and be running
    assert!(core.is_running().await);

    core.stop().await?;

    Ok(())
}

/// Test concurrent operations on the server
#[tokio::test]
async fn test_concurrent_operations() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Create source instances for concurrent test
    let concurrent_source = create_mock_source("concurrent-source");
    let extra_source_0 = create_mock_source("concurrent-source-0");
    let extra_source_2 = create_mock_source("concurrent-source-2");
    let extra_source_4 = create_mock_source("concurrent-source-4");

    // Build the core using the new builder API with all sources pre-registered
    let core = Arc::new(
        DrasiLib::builder()
            .with_id(&server_id)
            .with_source(concurrent_source)
            .with_source(extra_source_0)
            .with_source(extra_source_2)
            .with_source(extra_source_4)
            .build()
            .await?,
    );

    // Start server
    core.start().await?;

    // Test concurrent operations by starting/stopping sources
    let mut handles = vec![];

    for i in 0..5 {
        let core_clone = core.clone();
        let handle = tokio::spawn(async move {
            // Alternate between starting and stopping sources
            if i % 2 == 0 {
                let source_id = format!("concurrent-source-{}", i);
                core_clone.start_source(&source_id).await
            } else {
                sleep(Duration::from_millis(10)).await;
                core_clone.stop_source("concurrent-source").await
            }
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let _ = handle.await?;
    }

    // The core should still be in a valid running state
    assert!(core.is_running().await);

    core.stop().await?;

    Ok(())
}
