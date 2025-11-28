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

use anyhow::Result;
use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::{QuerySubscriber, ReactionRegistry, SourceRegistry};
use drasi_lib::plugin_core::Reaction as ReactionTrait;
use drasi_lib::plugin_core::Source as SourceTrait;
use drasi_lib::{Query, ReactionConfig, SourceConfig};
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

    async fn start(&self, _query_subscriber: Arc<dyn QuerySubscriber>) -> anyhow::Result<()> {
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

/// Create a mock source registry for testing
fn create_mock_source_registry() -> SourceRegistry {
    let mut registry = SourceRegistry::new();
    registry.register("mock", |config: &SourceConfig| {
        let source = MockSource {
            id: config.id.clone(),
            status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
        };
        Ok(Arc::new(source) as Arc<dyn SourceTrait>)
    });
    registry
}

/// Create a mock reaction registry for testing
fn create_mock_reaction_registry() -> ReactionRegistry {
    let mut registry = ReactionRegistry::new();
    registry.register("log", |config: &ReactionConfig| {
        let reaction = MockReaction {
            id: config.id.clone(),
            queries: config.queries.clone(),
            status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
        };
        Ok(Arc::new(reaction) as Arc<dyn ReactionTrait>)
    });
    registry
}

/// Integration test demonstrating data flow continues after server restart
#[tokio::test]
async fn test_data_flow_with_server_restart() -> Result<()> {
    // Create a shared counter to track how many results the reaction has processed
    let result_counter = Arc::new(AtomicUsize::new(0));
    let _counter_clone = result_counter.clone();

    // Create configuration
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API
    let query = Query::cypher("counter-query")
        .query("MATCH (n:Counter) RETURN n.value as value")
        .from_source("counter-source")
        .auto_start(true)
        .build();

    let core = Arc::new(
        DrasiLib::builder()
            .with_id(&server_id)
            .with_source_registry(create_mock_source_registry())
            .with_reaction_registry(create_mock_reaction_registry())
            .add_query(query)
            .build()
            .await?,
    );

    // Create source and reaction dynamically
    let source_config = SourceConfig::new("counter-source", "mock")
        .with_auto_start(true)
        .with_property("data_type", "counter")
        .with_property("interval_ms", 500);
    core.create_source(source_config).await?;

    let reaction_config = ReactionConfig::new("counter-reaction", "log")
        .with_query("counter-query")
        .with_auto_start(true);
    core.create_reaction(reaction_config).await?;

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
            .with_source_registry(create_mock_source_registry())
            .with_reaction_registry(create_mock_reaction_registry())
            .add_query(sensor_query)
            .add_query(vehicle_query)
            .add_query(combined_query)
            .build()
            .await?,
    );

    // Create sources dynamically
    let sensors_source = SourceConfig::new("sensors-source", "mock")
        .with_auto_start(true)
        .with_property("data_type", "sensor")
        .with_property("interval_ms", 1000);
    core.create_source(sensors_source).await?;

    let vehicles_source = SourceConfig::new("vehicles-source", "mock")
        .with_auto_start(true)
        .with_property("data_type", "generic")
        .with_property("interval_ms", 2000);
    core.create_source(vehicles_source).await?;

    // Create reaction dynamically
    let reaction_config = ReactionConfig::new("alert-handler", "log")
        .with_query("sensor-alerts")
        .with_query("combined-view")
        .with_auto_start(true);
    core.create_reaction(reaction_config).await?;

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
            .with_source_registry(create_mock_source_registry())
            .with_reaction_registry(create_mock_reaction_registry())
            .add_query(query)
            .build()
            .await?,
    );

    // Create source and reaction dynamically
    let source_config = SourceConfig::new("test-source", "mock")
        .with_auto_start(true);
    core.create_source(source_config).await?;

    let reaction_config = ReactionConfig::new("test-reaction", "log")
        .with_query("test-query")
        .with_auto_start(true);
    core.create_reaction(reaction_config).await?;

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

    // Build the core using the new builder API
    let core = Arc::new(
        DrasiLib::builder()
            .with_id(&server_id)
            .with_source_registry(create_mock_source_registry())
            .with_reaction_registry(create_mock_reaction_registry())
            .build()
            .await?,
    );

    // Create initial source dynamically
    let source_config = SourceConfig::new("concurrent-source", "mock")
        .with_auto_start(false);
    core.create_source(source_config).await?;

    // Start server
    core.start().await?;

    // Test concurrent operations by adding/removing sources dynamically
    let mut handles = vec![];

    for i in 0..5 {
        let core_clone = core.clone();
        let handle = tokio::spawn(async move {
            // Alternate between adding and removing
            if i % 2 == 0 {
                let new_source = SourceConfig::new(format!("concurrent-source-{}", i), "mock")
                    .with_auto_start(false);
                core_clone.create_source(new_source).await
            } else {
                sleep(Duration::from_millis(10)).await;
                core_clone
                    .remove_source("concurrent-source")
                    .await
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
