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

//! Mock source and reaction implementations for testing DrasiLib.

use async_trait::async_trait;
use drasi_lib::channels::dispatcher::ChangeDispatcher;
use drasi_lib::channels::{ComponentStatus, SubscriptionResponse};
use drasi_lib::context::{ReactionRuntimeContext, SourceRuntimeContext};
use drasi_lib::Reaction as ReactionTrait;
use drasi_lib::Source as SourceTrait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A mock source for testing
#[derive(Clone)]
pub struct MockSource {
    inner: Arc<MockSourceInner>,
}

struct MockSourceInner {
    id: String,
    status: RwLock<ComponentStatus>,
    instance_id: RwLock<String>,
}

impl MockSource {
    pub fn new(id: &str) -> Self {
        Self {
            inner: Arc::new(MockSourceInner {
                id: id.to_string(),
                status: RwLock::new(ComponentStatus::Stopped),
                instance_id: RwLock::new(String::new()),
            }),
        }
    }

    pub async fn emit_log(&self, message: &str) {
        let instance_id = self.inner.instance_id.read().await.clone();
        let span = tracing::info_span!(
            "mock_source_log",
            instance_id = %instance_id,
            component_id = %self.inner.id,
            component_type = "source"
        );
        let _guard = span.enter();
        tracing::info!("{}", message);
    }
}

#[async_trait]
impl SourceTrait for MockSource {
    fn id(&self) -> &str {
        &self.inner.id
    }

    fn type_name(&self) -> &str {
        "mock"
    }

    fn properties(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }

    async fn start(&self) -> anyhow::Result<()> {
        *self.inner.status.write().await = ComponentStatus::Running;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        *self.inner.status.write().await = ComponentStatus::Stopped;
        Ok(())
    }

    async fn status(&self) -> ComponentStatus {
        self.inner.status.read().await.clone()
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
            source_id: self.inner.id.clone(),
            receiver,
            bootstrap_receiver: None,
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn initialize(&self, context: SourceRuntimeContext) {
        *self.inner.instance_id.write().await = context.instance_id.clone();
    }
}

/// A mock reaction for testing
#[derive(Clone)]
pub struct MockReaction {
    inner: Arc<MockReactionInner>,
}

struct MockReactionInner {
    id: String,
    queries: Vec<String>,
    status: RwLock<ComponentStatus>,
    instance_id: RwLock<String>,
}

impl MockReaction {
    pub fn new(id: &str, queries: Vec<String>) -> Self {
        Self {
            inner: Arc::new(MockReactionInner {
                id: id.to_string(),
                queries,
                status: RwLock::new(ComponentStatus::Stopped),
                instance_id: RwLock::new(String::new()),
            }),
        }
    }

    pub async fn emit_log(&self, message: &str) {
        let instance_id = self.inner.instance_id.read().await.clone();
        let span = tracing::info_span!(
            "mock_reaction_log",
            instance_id = %instance_id,
            component_id = %self.inner.id,
            component_type = "reaction"
        );
        let _guard = span.enter();
        tracing::info!("{}", message);
    }
}

#[async_trait]
impl ReactionTrait for MockReaction {
    fn id(&self) -> &str {
        &self.inner.id
    }

    fn type_name(&self) -> &str {
        "log"
    }

    fn properties(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }

    fn query_ids(&self) -> Vec<String> {
        self.inner.queries.clone()
    }

    async fn initialize(&self, context: ReactionRuntimeContext) {
        *self.inner.instance_id.write().await = context.instance_id.clone();
    }

    async fn start(&self) -> anyhow::Result<()> {
        *self.inner.status.write().await = ComponentStatus::Running;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        *self.inner.status.write().await = ComponentStatus::Stopped;
        Ok(())
    }

    async fn status(&self) -> ComponentStatus {
        self.inner.status.read().await.clone()
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
