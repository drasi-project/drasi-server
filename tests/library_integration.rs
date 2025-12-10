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

//! Library Integration Tests
//!
//! Note: Sources and reactions must be provided as instances when building DrasiLib.
//! Dynamic creation via config is not supported.

use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::QuerySubscriber;
use drasi_lib::plugin_core::Reaction as ReactionTrait;
use drasi_lib::plugin_core::Source as SourceTrait;
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

#[tokio::test]
async fn test_basic_server_lifecycle() {
    // Create source instance
    let test_source = create_mock_source("test-source");

    // Create a basic server using the new builder API
    let server = DrasiServerBuilder::new()
        .with_source(test_source)
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
    // Create source and reaction instances
    let test_source = create_mock_source("test_source");
    let test_reaction = create_mock_reaction("test_reaction", vec!["test_query".to_string()]);

    // Create server with components using the new builder API
    let server = DrasiServerBuilder::new()
        .with_source(test_source)
        .with_reaction(test_reaction)
        .with_query_config(
            "test_query",
            "MATCH (n) RETURN n",
            vec!["test_source".to_string()],
        )
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
async fn test_source_lifecycle_operations() {
    // Create source instance
    let test_source = create_mock_source("lifecycle_source");

    // Start with server with source
    let server = DrasiServerBuilder::new()
        .with_source(test_source)
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Source is already running (auto-started on first startup)
    // Wait briefly for startup to complete
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Server should still be running
    assert!(server.is_running().await);

    // Stop the source
    server
        .stop_source("lifecycle_source")
        .await
        .expect("Failed to stop source");

    // Remove the source
    server
        .remove_source("lifecycle_source")
        .await
        .expect("Failed to remove source");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_server_with_api() {
    // Create source instance
    let test_source = create_mock_source("api_source");

    // Create server with API
    let _server = DrasiServerBuilder::new()
        .with_source(test_source)
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

    // Create source instance
    let persist_source = create_mock_source("persist_source");

    // Create server with config persistence
    let _server = DrasiServerBuilder::new()
        .with_source(persist_source)
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
async fn test_concurrent_start_stop_operations() {
    // Create multiple source instances
    let source1 = create_mock_source("concurrent_source_1");
    let source2 = create_mock_source("concurrent_source_2");
    let source3 = create_mock_source("concurrent_source_3");
    let source4 = create_mock_source("concurrent_source_4");
    let source5 = create_mock_source("concurrent_source_5");

    // Start with server with all sources pre-registered
    let server = DrasiServerBuilder::new()
        .with_source(source1)
        .with_source(source2)
        .with_source(source3)
        .with_source(source4)
        .with_source(source5)
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Sources are now auto-started on first startup
    // Test concurrent stop operations instead
    let mut stop_tasks = vec![];
    for i in 1..=5 {
        let server_clone = server.clone();
        let task = tokio::spawn(async move {
            let source_id = format!("concurrent_source_{i}");
            server_clone.stop_source(&source_id).await
        });
        stop_tasks.push(task);
    }

    // Wait for all stop tasks
    for task in stop_tasks {
        task.await
            .expect("Task panicked")
            .expect("Failed to stop source");
    }

    // Now test concurrent start operations
    let mut start_tasks = vec![];
    for i in 1..=5 {
        let server_clone = server.clone();
        let task = tokio::spawn(async move {
            let source_id = format!("concurrent_source_{i}");
            server_clone.start_source(&source_id).await
        });
        start_tasks.push(task);
    }

    // Wait for all start tasks
    for task in start_tasks {
        task.await
            .expect("Task panicked")
            .expect("Failed to start source");
    }

    // Server should still be running with all sources
    assert!(server.is_running().await);

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_graceful_shutdown_timeout() {
    // Create source instance
    let timeout_source = create_mock_source("timeout_source");

    // Create server with a source
    let server = DrasiServerBuilder::new()
        .with_source(timeout_source)
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
