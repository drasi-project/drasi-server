// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

#![allow(dead_code)]

use std::sync::{Arc, OnceLock};

pub fn plugin() -> Arc<drasi_host_sdk::computation::NativePlugin> {
    static PLUGIN: OnceLock<Arc<drasi_host_sdk::computation::NativePlugin>> = OnceLock::new();
    PLUGIN
        .get_or_init(|| {
            drasi_host_sdk::computation::NativePlugin::from_definition(
                drasi_computation_transformer_query::plugin().expect("query plugin definition"),
            )
            .expect("query plugin ABI")
        })
        .clone()
}

pub fn factory() -> Arc<dyn drasi_lib::computation::v1::ComponentFactory> {
    plugin()
        .factories()
        .iter()
        .find(|factory| factory.supports_ordinary_queries())
        .expect("indexed query factory")
        .for_ordinary_queries()
        .expect("query authoring")
}

pub fn builder() -> drasi_lib::DrasiLibBuilder {
    drasi_lib::DrasiLib::builder().with_query_factory(factory())
}

pub fn registry() -> Arc<tokio::sync::RwLock<drasi_host_sdk::PluginRegistry>> {
    let mut registry = drasi_host_sdk::PluginRegistry::new();
    registry
        .register_computation_plugin(plugin())
        .expect("query plugin registration");
    Arc::new(tokio::sync::RwLock::new(registry))
}
