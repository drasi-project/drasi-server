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

//! Registration of all statically-linked built-in plugins.
//!
//! This module registers every source, bootstrapper, and reaction plugin that
//! ships with the Drasi Server binary.  The [`register_builtin_plugins`]
//! function is called once at startup to populate a [`PluginRegistry`].

use crate::plugin_registry::PluginRegistry;
use drasi_plugin_sdk::{BootstrapPluginDescriptor, ReactionPluginDescriptor, SourcePluginDescriptor};
use log::info;
use std::sync::Arc;

/// Register all built-in (statically-linked) plugin descriptors.
pub fn register_builtin_plugins(registry: &mut PluginRegistry) {
    info!("Loading built-in plugins (static)...");

    // Sources
    let desc = drasi_source_mock::descriptor::MockSourceDescriptor;
    info!("  [static] source: {}", desc.kind());
    registry.register_source(Arc::new(desc));

    let desc = drasi_source_http::descriptor::HttpSourceDescriptor;
    info!("  [static] source: {}", desc.kind());
    registry.register_source(Arc::new(desc));

    let desc = drasi_source_grpc::descriptor::GrpcSourceDescriptor;
    info!("  [static] source: {}", desc.kind());
    registry.register_source(Arc::new(desc));

    let desc = drasi_source_postgres::descriptor::PostgresSourceDescriptor;
    info!("  [static] source: {}", desc.kind());
    registry.register_source(Arc::new(desc));

    let desc = drasi_source_mssql::descriptor::MsSqlSourceDescriptor;
    info!("  [static] source: {}", desc.kind());
    registry.register_source(Arc::new(desc));

    // Bootstrappers
    let desc = drasi_bootstrap_postgres::descriptor::PostgresBootstrapDescriptor;
    info!("  [static] bootstrap: {}", desc.kind());
    registry.register_bootstrapper(Arc::new(desc));

    let desc = drasi_bootstrap_scriptfile::descriptor::ScriptFileBootstrapDescriptor;
    info!("  [static] bootstrap: {}", desc.kind());
    registry.register_bootstrapper(Arc::new(desc));

    let desc = drasi_bootstrap_mssql::descriptor::MsSqlBootstrapDescriptor;
    info!("  [static] bootstrap: {}", desc.kind());
    registry.register_bootstrapper(Arc::new(desc));

    // Reactions
    let desc = drasi_reaction_log::descriptor::LogReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_http::descriptor::HttpReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_http_adaptive::descriptor::HttpAdaptiveReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_grpc::descriptor::GrpcReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_grpc_adaptive::descriptor::GrpcAdaptiveReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_sse::descriptor::SseReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_profiler::descriptor::ProfilerReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_storedproc_postgres::descriptor::PostgresStoredProcReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_storedproc_mysql::descriptor::MySqlStoredProcReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));

    let desc = drasi_reaction_storedproc_mssql::descriptor::MsSqlStoredProcReactionDescriptor;
    info!("  [static] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));
}
