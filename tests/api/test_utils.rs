//! Shared test utilities for API tests
//!
//! Provides mock registries for testing DrasiLib with mock sources and reactions.

use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::{ReactionRegistry, SourceRegistry};
use drasi_lib::reactions::common::base::QuerySubscriber;
use drasi_lib::reactions::Reaction as ReactionTrait;
use drasi_lib::sources::Source as SourceTrait;
use drasi_lib::{ReactionConfig, SourceConfig};
use std::sync::Arc;
use tokio::sync::RwLock;

/// A mock source for testing
struct MockSource {
    config: SourceConfig,
    status: Arc<RwLock<ComponentStatus>>,
}

#[async_trait]
impl SourceTrait for MockSource {
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

    fn get_config(&self) -> &SourceConfig {
        &self.config
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
            source_id: self.config.id.clone(),
            receiver,
            bootstrap_receiver: None,
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A mock reaction for testing
struct MockReaction {
    config: ReactionConfig,
    status: Arc<RwLock<ComponentStatus>>,
}

#[async_trait]
impl ReactionTrait for MockReaction {
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

    fn get_config(&self) -> &ReactionConfig {
        &self.config
    }
}

/// Create a mock source registry for testing
pub fn create_mock_source_registry() -> SourceRegistry {
    let mut registry = SourceRegistry::new();
    registry.register("mock".to_string(), |config, _event_tx| {
        let source = MockSource {
            config,
            status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
        };
        Ok(Arc::new(source) as Arc<dyn SourceTrait>)
    });
    registry
}

/// Create a mock reaction registry for testing
pub fn create_mock_reaction_registry() -> ReactionRegistry {
    let mut registry = ReactionRegistry::new();
    registry.register("log".to_string(), |config, _event_tx| {
        let reaction = MockReaction {
            config,
            status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
        };
        Ok(Arc::new(reaction) as Arc<dyn ReactionTrait>)
    });
    registry
}
