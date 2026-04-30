//! Minimal test plugin for upgrade integration testing.
//!
//! This plugin provides a "upgrade-test" source kind with a noop implementation.
//! It's built twice (with different PLUGIN_VERSION env vars) to produce v1 and v2
//! binaries for testing the rolling upgrade engine.

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use drasi_lib::channels::events::SubscriptionResponse;
use drasi_lib::config::SourceSubscriptionSettings;
use drasi_lib::context::SourceRuntimeContext;
use drasi_lib::sources::base::{SourceBase, SourceBaseParams};
use drasi_lib::ComponentStatus;
use drasi_lib::Source;
use drasi_plugin_sdk::prelude::*;

// ── Noop Source ──────────────────────────────────────────────────────────────

/// A minimal source that does nothing — used solely for upgrade testing.
pub struct NoopSource {
    base: SourceBase,
    version: &'static str,
}

impl NoopSource {
    pub fn new(id: &str, auto_start: bool, version: &'static str) -> Result<Self> {
        let mut params = SourceBaseParams::new(id);
        params.auto_start = auto_start;
        Ok(Self {
            base: SourceBase::new(params)?,
            version,
        })
    }
}

#[async_trait]
impl Source for NoopSource {
    fn id(&self) -> &str {
        &self.base.id
    }

    fn type_name(&self) -> &str {
        "upgrade-test"
    }

    fn properties(&self) -> HashMap<String, serde_json::Value> {
        let mut map = HashMap::new();
        map.insert(
            "version".to_string(),
            serde_json::Value::String(self.version.to_string()),
        );
        map
    }

    fn auto_start(&self) -> bool {
        self.base.get_auto_start()
    }

    async fn start(&self) -> Result<()> {
        self.base
            .set_status(ComponentStatus::Running, Some("Noop source running".to_string()))
            .await;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.base
            .set_status(ComponentStatus::Stopped, Some("Noop source stopped".to_string()))
            .await;
        Ok(())
    }

    async fn status(&self) -> ComponentStatus {
        self.base.get_status().await
    }

    async fn subscribe(
        &self,
        settings: SourceSubscriptionSettings,
    ) -> Result<SubscriptionResponse> {
        self.base
            .subscribe_with_bootstrap(&settings, "NoopUpgradeTest")
            .await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn initialize(&self, context: SourceRuntimeContext) {
        self.base.initialize(context).await;
    }
}

// ── Source Descriptor ────────────────────────────────────────────────────────

pub struct UpgradeTestSourceDescriptor;

#[async_trait]
impl SourcePluginDescriptor for UpgradeTestSourceDescriptor {
    fn kind(&self) -> &str {
        "upgrade-test"
    }

    fn config_version(&self) -> &str {
        "1.0.0"
    }

    fn config_schema_name(&self) -> &str {
        "source.upgrade-test.NoopConfig"
    }

    fn config_schema_json(&self) -> String {
        "{}".to_string()
    }

    async fn create_source(
        &self,
        id: &str,
        _config_json: &serde_json::Value,
        auto_start: bool,
    ) -> anyhow::Result<Box<dyn Source>> {
        let source = NoopSource::new(id, auto_start, env!("PLUGIN_VERSION"))?;
        Ok(Box::new(source))
    }
}

// ── Plugin Entry Point ───────────────────────────────────────────────────────

drasi_plugin_sdk::export_plugin!(
    plugin_id = "upgrade-test-source",
    core_version = env!("CARGO_PKG_VERSION"),
    lib_version = env!("CARGO_PKG_VERSION"),
    plugin_version = env!("PLUGIN_VERSION"),
    source_descriptors = [UpgradeTestSourceDescriptor],
    reaction_descriptors = [],
    bootstrap_descriptors = [],
);
