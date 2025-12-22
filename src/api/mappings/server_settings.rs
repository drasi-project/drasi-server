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

//! Server settings mapper

use crate::api::mappings::core::mapper::DtoMapper;
use crate::config::types::DrasiServerConfig;
use anyhow::Result;

/// Resolved server settings with actual values (no ConfigValue wrappers)
#[derive(Debug, Clone)]
pub struct ResolvedServerSettings {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub disable_persistence: bool,
}

/// Maps DrasiServerConfig to ResolvedServerSettings domain model
pub fn map_server_settings(
    config: &DrasiServerConfig,
    mapper: &DtoMapper,
) -> Result<ResolvedServerSettings> {
    Ok(ResolvedServerSettings {
        host: mapper.resolve_typed(&config.host)?,
        port: mapper.resolve_typed(&config.port)?,
        log_level: mapper.resolve_typed(&config.log_level)?,
        disable_persistence: config.disable_persistence,
    })
}
