//! Shared test utilities for API tests
//!
//! Provides mock registries for testing DrasiLib with mock sources and reactions.

use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::{QuerySubscriber, ReactionRegistry, SourceRegistry};
use drasi_lib::plugin_core::Reaction as ReactionTrait;
use drasi_lib::plugin_core::Source as SourceTrait;
use drasi_lib::{ReactionConfig, SourceConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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
pub fn create_mock_source_registry() -> SourceRegistry {
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
pub fn create_mock_reaction_registry() -> ReactionRegistry {
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
