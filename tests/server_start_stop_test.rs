use anyhow::Result;
use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentStatus, SubscriptionResponse};
use drasi_lib::plugin_core::{ReactionRegistry, SourceRegistry};
use drasi_lib::reactions::common::base::QuerySubscriber;
use drasi_lib::reactions::Reaction as ReactionTrait;
use drasi_lib::sources::Source as SourceTrait;
use drasi_lib::{Query, Reaction, ReactionConfig, Source, SourceConfig};
use drasi_server::DrasiLib;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

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
fn create_mock_source_registry() -> SourceRegistry {
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
fn create_mock_reaction_registry() -> ReactionRegistry {
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
async fn test_auto_start_components() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with auto-start components
    let source = Source::mock("test-source").auto_start(true).build();
    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();
    let reaction = Reaction::log("test-reaction")
        .subscribe_to("test-query")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id(&server_id)
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_source(source)
        .add_query(query)
        .add_reaction(reaction)
        .build()
        .await?;

    let core = Arc::new(core);

    // Components are configured but not running before server start
    assert!(!core.is_running().await);

    // Start the server
    core.start().await?;

    // Wait a bit for components to start
    sleep(Duration::from_millis(100)).await;

    // All auto-start components should be running
    assert!(core.is_running().await);

    // Stop the server
    core.stop().await?;

    // All components should be stopped
    assert!(!core.is_running().await);

    // Start again - auto-start components should restart
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    assert!(core.is_running().await);

    Ok(())
}

#[tokio::test]
async fn test_manual_vs_auto_start_components() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with mixed auto-start settings
    let auto_source = Source::mock("auto-source").auto_start(true).build();
    let manual_source = Source::mock("manual-source").auto_start(false).build();

    let auto_query = Query::cypher("auto-query")
        .query("MATCH (n) RETURN n")
        .from_source("auto-source")
        .auto_start(true)
        .build();

    let manual_query = Query::cypher("manual-query")
        .query("MATCH (n) RETURN n")
        .from_source("manual-source")
        .auto_start(false)
        .build();

    let core = DrasiLib::builder()
        .with_id(&server_id)
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_source(auto_source)
        .add_source(manual_source)
        .add_query(auto_query)
        .add_query(manual_query)
        .build()
        .await?;

    let core = Arc::new(core);

    // Start the server
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    // Auto-start components should be running
    assert!(core.is_running().await);

    // Stop the server
    core.stop().await?;

    // All components should be stopped
    assert!(!core.is_running().await);

    // Start the server again
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    // Auto-start components should restart
    assert!(core.is_running().await);

    Ok(())
}

#[tokio::test]
async fn test_component_startup_sequence() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with components that have dependencies
    let source1 = Source::mock("source1").auto_start(true).build();
    let source2 = Source::mock("source2").auto_start(true).build();

    let query1 = Query::cypher("query1")
        .query("MATCH (n) RETURN n")
        .from_source("source1")
        .auto_start(true)
        .build();

    let query2 = Query::cypher("query2")
        .query("MATCH (n) RETURN n")
        .from_source("source2")
        .auto_start(true)
        .build();

    let reaction1 = Reaction::log("reaction1")
        .subscribe_to("query1")
        .auto_start(true)
        .build();

    let reaction2 = Reaction::log("reaction2")
        .subscribe_to("query2")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id(&server_id)
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_source(source1)
        .add_source(source2)
        .add_query(query1)
        .add_query(query2)
        .add_reaction(reaction1)
        .add_reaction(reaction2)
        .build()
        .await?;

    let core = Arc::new(core);

    // Start the server
    core.start().await?;

    // Give components time to start in sequence
    sleep(Duration::from_millis(200)).await;

    // Verify all components are running
    assert!(core.is_running().await);

    Ok(())
}
