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

use anyhow::Result;
use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::{Reaction as ReactionTrait, ReactionRegistry, Source as SourceTrait, SourceRegistry};
use drasi_lib::plugin_core::QuerySubscriber;
use drasi_lib::{Query, ReactionConfig, SourceConfig};
use drasi_server::DrasiLib;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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
        let source = MockSource::new(&config.id);
        Ok(Arc::new(source) as Arc<dyn SourceTrait>)
    });
    registry
}

/// Create a mock reaction registry for testing
fn create_mock_reaction_registry() -> ReactionRegistry {
    let mut registry = ReactionRegistry::new();
    registry.register("log", |config: &ReactionConfig| {
        let reaction = MockReaction::new(&config.id, config.queries.clone());
        Ok(Arc::new(reaction) as Arc<dyn ReactionTrait>)
    });
    registry
}

#[tokio::test]
async fn test_server_start_stop_cycle() -> Result<()> {
    // Create a minimal runtime config
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API
    let core = DrasiLib::builder()
        .with_id(&server_id)
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
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

    // Build the core with a query
    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id(&server_id)
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_query(query)
        .build()
        .await?;

    let core = Arc::new(core);

    // Create the source that the query references (sources are created via registry)
    let source_config = SourceConfig::new("test-source", "mock").with_auto_start(true);
    core.create_source(source_config).await?;

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
