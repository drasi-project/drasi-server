// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::sync::Arc;

use drasi_lib::{computation::v1::ComponentFactory, DrasiLib};
use tokio::sync::RwLock;

use crate::plugin_registry::PluginRegistry;

#[derive(Debug, thiserror::Error)]
#[error("The native query transformer plugin is not loaded. Load a matching drasi-computation-transformer-query library before creating ordinary queries.")]
pub(crate) struct MissingQueryPlugin;

pub(crate) fn factory(registry: &PluginRegistry) -> anyhow::Result<Arc<dyn ComponentFactory>> {
    registry
        .query_factory()?
        .ok_or_else(|| MissingQueryPlugin.into())
}

pub(crate) async fn ensure_registered(
    core: &DrasiLib,
    registry: &RwLock<PluginRegistry>,
) -> anyhow::Result<()> {
    if core.query_factory_implementation().is_none() {
        let factory = factory(&*registry.read().await)?;
        core.register_query_factory(factory)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_plugin_never_falls_back_and_later_registration_is_supported(
    ) -> anyhow::Result<()> {
        let registry = RwLock::new(PluginRegistry::new());
        let core = DrasiLib::builder().build().await?;
        let missing = ensure_registered(&core, &registry)
            .await
            .expect_err("missing query plugin");
        assert!(missing.downcast_ref::<MissingQueryPlugin>().is_some());
        assert!(core.query_factory_implementation().is_none());
        let plugin = drasi_host_sdk::computation::NativePlugin::from_definition(
            drasi_computation_transformer_query::plugin()?,
        )?;
        registry.write().await.register_computation_plugin(plugin)?;
        ensure_registered(&core, &registry).await?;
        let selected = core
            .query_factory_implementation()
            .expect("registered query factory");
        assert_eq!(
            selected.name.as_ref(),
            "drasi.experimental/indexed-query/ordinary"
        );
        ensure_registered(&core, &registry).await?;
        assert_eq!(core.query_factory_implementation(), Some(selected));
        core.shutdown().await?;
        Ok(())
    }
}
