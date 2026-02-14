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

//! Tests that bootstrap data appears in query results.
//!
//! Regression test for a bug where the bootstrap processor discarded results
//! instead of applying them to `current_results`.

mod test_support;

use async_trait::async_trait;
use drasi_core::models::{
    Element, ElementMetadata, ElementPropertyMap, ElementReference, ElementValue, SourceChange,
};
use drasi_lib::channels::dispatcher::{ChangeDispatcher, ChannelChangeDispatcher};
use drasi_lib::channels::{BootstrapEvent, ComponentStatus, SubscriptionResponse};
use drasi_lib::config::SourceSubscriptionSettings;
use drasi_lib::context::SourceRuntimeContext;
use drasi_lib::Source as SourceTrait;
use drasi_server::DrasiServerBuilder;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::Duration;

/// A mock source that sends bootstrap data when subscribed.
struct BootstrapMockSource {
    id: String,
    status: Arc<RwLock<ComponentStatus>>,
    /// Elements to send during bootstrap.
    bootstrap_elements: Vec<Element>,
}

impl BootstrapMockSource {
    fn new(id: &str, elements: Vec<Element>) -> Self {
        Self {
            id: id.to_string(),
            status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
            bootstrap_elements: elements,
        }
    }
}

#[async_trait]
impl SourceTrait for BootstrapMockSource {
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
        settings: SourceSubscriptionSettings,
    ) -> anyhow::Result<SubscriptionResponse> {
        // Create the change event channel (for streaming CDC events)
        let dispatcher =
            ChannelChangeDispatcher::<drasi_lib::channels::SourceEventWrapper>::new(100);
        let receiver = dispatcher.create_receiver().await?;

        // Create a bootstrap channel and send bootstrap elements
        let (bootstrap_tx, bootstrap_rx) = mpsc::channel::<BootstrapEvent>(100);
        let elements = self.bootstrap_elements.clone();
        let source_id = self.id.clone();

        tokio::spawn(async move {
            for (i, element) in elements.into_iter().enumerate() {
                let event = BootstrapEvent {
                    source_id: source_id.clone(),
                    change: SourceChange::Insert { element },
                    timestamp: chrono::Utc::now(),
                    sequence: i as u64,
                };
                if bootstrap_tx.send(event).await.is_err() {
                    break;
                }
            }
            // Dropping bootstrap_tx signals completion
        });

        Ok(SubscriptionResponse {
            query_id: settings.query_id,
            source_id: self.id.clone(),
            receiver,
            bootstrap_receiver: Some(bootstrap_rx),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn initialize(&self, _context: SourceRuntimeContext) {
        // No-op
    }
}

/// Helper to create a test element with a given id and name property.
fn make_element(source_id: &str, id: &str, name: &str) -> Element {
    let mut props = ElementPropertyMap::new();
    props.insert("name", ElementValue::String(name.into()));

    Element::Node {
        metadata: ElementMetadata {
            reference: ElementReference::new(source_id, id),
            labels: vec!["Item".into()].into(),
            effective_from: 1000,
        },
        properties: props,
    }
}

#[tokio::test]
async fn test_bootstrap_data_appears_in_query_results() {
    // Create 3 elements that will be delivered via bootstrap
    let elements = vec![
        make_element("bootstrap-src", "item-1", "Alpha"),
        make_element("bootstrap-src", "item-2", "Beta"),
        make_element("bootstrap-src", "item-3", "Gamma"),
    ];

    let source = BootstrapMockSource::new("bootstrap-src", elements);

    let server = DrasiServerBuilder::new()
        .with_source(source)
        .with_query_config(
            "bootstrap-query",
            "MATCH (i:Item) RETURN i.name AS name",
            vec!["bootstrap-src".to_string()],
        )
        .build_core()
        .await
        .expect("Failed to build server");

    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Wait for bootstrap to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    let results = server
        .get_query_results("bootstrap-query")
        .await
        .expect("Failed to get query results");

    assert_eq!(
        results.len(),
        3,
        "Expected 3 bootstrap results, got {}: {:?}",
        results.len(),
        results
    );

    // Verify result content
    let names: Vec<String> = results
        .iter()
        .filter_map(|r| {
            r.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert!(
        names.contains(&"Alpha".to_string()),
        "Missing Alpha in {names:?}",
    );
    assert!(
        names.contains(&"Beta".to_string()),
        "Missing Beta in {names:?}",
    );
    assert!(
        names.contains(&"Gamma".to_string()),
        "Missing Gamma in {names:?}",
    );

    server.stop().await.expect("Failed to stop server");
}
