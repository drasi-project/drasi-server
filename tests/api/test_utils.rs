//! Shared test utilities for API tests
//!
//! Provides mock sources and reactions for testing DrasiLib.

use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::QuerySubscriber;
use drasi_lib::plugin_core::Reaction as ReactionTrait;
use drasi_lib::plugin_core::Source as SourceTrait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A mock source for testing
pub struct MockSource {
    id: String,
    status: Arc<RwLock<ComponentStatus>>,
}

impl MockSource {
    pub fn new(id: &str) -> Self {
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
        settings: drasi_lib::config::SourceSubscriptionSettings,
    ) -> anyhow::Result<SubscriptionResponse> {
        use drasi_lib::channels::dispatcher::ChannelChangeDispatcher;
        let dispatcher =
            ChannelChangeDispatcher::<drasi_lib::channels::SourceEventWrapper>::new(100);
        let receiver = dispatcher.create_receiver().await?;
        Ok(SubscriptionResponse {
            query_id: settings.query_id,
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
pub struct MockReaction {
    id: String,
    queries: Vec<String>,
    status: Arc<RwLock<ComponentStatus>>,
}

impl MockReaction {
    pub fn new(id: &str, queries: Vec<String>) -> Self {
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
pub fn create_mock_source(id: &str) -> MockSource {
    MockSource::new(id)
}

/// Create a mock reaction for testing
pub fn create_mock_reaction(id: &str, queries: Vec<String>) -> MockReaction {
    MockReaction::new(id, queries)
}
