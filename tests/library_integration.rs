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

use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::{QuerySubscriber, ReactionRegistry, SourceRegistry};
use drasi_lib::plugin_core::Reaction as ReactionTrait;
use drasi_lib::plugin_core::Source as SourceTrait;
use drasi_lib::{ReactionConfig, SourceConfig};
use drasi_server::DrasiServerBuilder;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

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

#[tokio::test]
async fn test_basic_server_lifecycle() {
    // Create a basic server using the new builder API
    let server = DrasiServerBuilder::new()
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Verify it's running
    assert!(server.is_running().await);

    // Shutdown
    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_server_with_components() {
    // Create server with components using the new builder API
    let server = DrasiServerBuilder::new()
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .with_simple_source("test_source", "mock")
        .with_simple_query(
            "test_query",
            "MATCH (n) RETURN n",
            vec!["test_source".to_string()],
        )
        .with_log_reaction("test_reaction", vec!["test_query".to_string()])
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Wait a bit for components to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check that server is running with all components
    assert!(server.is_running().await);

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_dynamic_component_management() {
    // Start with empty server using the new builder API
    let server = DrasiServerBuilder::new()
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Add source dynamically using the new runtime API
    let source_config = SourceConfig::new("dynamic_source", "mock")
        .with_auto_start(true);

    server
        .create_source(source_config)
        .await
        .expect("Failed to add source");

    // Wait for source to start (auto_start is true)
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Server should still be running with the new source
    assert!(server.is_running().await);

    // Remove the source using the new API
    server
        .remove_source("dynamic_source")
        .await
        .expect("Failed to remove source");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_server_with_api() {
    // Create server with API
    let _server = DrasiServerBuilder::new()
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .with_port(0) // Use port 0 for random available port
        .build()
        .await
        .expect("Failed to build server");

    // Note: We can't easily test the full API server in unit tests
    // as it requires running the blocking server.run() method.
    // This test just verifies the builder works correctly.

    // Test passes if builder completes without error
}

#[tokio::test]
async fn test_config_persistence() {
    let config_file = "test_config_persistence.yaml";

    // Create server with config persistence
    let _server = DrasiServerBuilder::new()
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .with_simple_source("persist_source", "mock")
        .with_config_file(config_file)
        .build()
        .await
        .expect("Failed to build server");

    // Clean up test file if it exists
    let _ = std::fs::remove_file(config_file);

    // Note: Full persistence testing would require more setup
    // Test passes if builder completes without error
}

#[tokio::test]
async fn test_concurrent_operations() {
    // Start with empty server using the new builder API
    let server = DrasiServerBuilder::new()
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Spawn multiple tasks that add sources concurrently using the new runtime API
    let mut tasks = vec![];
    for i in 0..5 {
        let server_clone = server.clone();
        let task = tokio::spawn(async move {
            let config = SourceConfig::new(format!("concurrent_source_{}", i), "mock")
                .with_auto_start(false);
            server_clone.create_source(config).await
        });
        tasks.push(task);
    }

    // Wait for all tasks
    for task in tasks {
        task.await
            .expect("Task panicked")
            .expect("Failed to add source");
    }

    // Server should still be running with all sources
    assert!(server.is_running().await);

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_graceful_shutdown_timeout() {
    // Create server with a source using the new builder API
    let server = DrasiServerBuilder::new()
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .with_simple_source("timeout_source", "mock")
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Shutdown should complete within reasonable time
    let shutdown_result = timeout(Duration::from_secs(5), server.stop()).await;

    assert!(shutdown_result.is_ok(), "Shutdown timed out");
    shutdown_result.expect("Timeout").expect("Shutdown failed");
}
