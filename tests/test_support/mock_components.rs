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

    async fn initialize(&self, _context: SourceRuntimeContext) {
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

    async fn initialize(&self, _context: ReactionRuntimeContext) {
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
}

/// Create a mock source for testing
pub fn create_mock_source(id: &str) -> MockSource {
    MockSource::new(id)
}

/// Create a mock reaction for testing
pub fn create_mock_reaction(id: &str, queries: Vec<String>) -> MockReaction {
    MockReaction::new(id, queries)
}
