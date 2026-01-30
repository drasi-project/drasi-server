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

//! MSSQL stored procedure reaction configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::{
    MssqlStoredProcReactionConfigDto, StoredProcQueryConfigDto, StoredProcTemplateSpecDto,
};
use drasi_lib::reactions::common::{QueryConfig, TemplateSpec};
use drasi_reaction_storedproc_mssql::MsSqlStoredProcReactionConfig;
use std::collections::HashMap;

pub struct MsSqlStoredProcReactionConfigMapper;

impl ConfigMapper<MssqlStoredProcReactionConfigDto, MsSqlStoredProcReactionConfig>
    for MsSqlStoredProcReactionConfigMapper
{
    fn map(
        &self,
        dto: &MssqlStoredProcReactionConfigDto,
        resolver: &DtoMapper,
    ) -> Result<MsSqlStoredProcReactionConfig, MappingError> {
        let hostname = resolver.resolve_string(&dto.hostname)?;
        let port = dto
            .port
            .as_ref()
            .map(|p| resolver.resolve_typed(p))
            .transpose()?;
        let database = resolver.resolve_string(&dto.database)?;
        let command_timeout_ms = resolver.resolve_typed(&dto.command_timeout_ms)?;
        let retry_attempts = resolver.resolve_typed(&dto.retry_attempts)?;

        // Map routes
        let routes: HashMap<String, QueryConfig> = dto
            .routes
            .iter()
            .map(|(query_id, query_config)| {
                map_query_config(query_config, resolver).map(|config| (query_id.clone(), config))
            })
            .collect::<Result<_, _>>()?;

        // Map default template
        let default_template = dto
            .default_template
            .as_ref()
            .map(|t| map_query_config(t, resolver))
            .transpose()?;

        // Handle authentication - legacy user/password or identity_provider_id
        let (user, password) = if let (Some(user_val), Some(pass_val)) =
            (dto.user.as_ref(), dto.password.as_ref())
        {
            (
                resolver.resolve_string(user_val)?,
                resolver.resolve_string(pass_val)?,
            )
        } else {
            (String::new(), String::new())
        };

        Ok(MsSqlStoredProcReactionConfig {
            hostname,
            port,
            user,
            password,
            database,
            ssl: dto.ssl,
            identity_provider: None,
            routes,
            default_template,
            command_timeout_ms,
            retry_attempts,
        })
    }
}

/// Map a QueryConfigDto to QueryConfig
fn map_query_config(
    dto: &StoredProcQueryConfigDto,
    resolver: &DtoMapper,
) -> Result<QueryConfig, MappingError> {
    Ok(QueryConfig {
        added: dto
            .added
            .as_ref()
            .map(|t| map_template_spec(t, resolver))
            .transpose()?,
        updated: dto
            .updated
            .as_ref()
            .map(|t| map_template_spec(t, resolver))
            .transpose()?,
        deleted: dto
            .deleted
            .as_ref()
            .map(|t| map_template_spec(t, resolver))
            .transpose()?,
    })
}

/// Map a TemplateSpecDto to TemplateSpec
fn map_template_spec(
    dto: &StoredProcTemplateSpecDto,
    resolver: &DtoMapper,
) -> Result<TemplateSpec, MappingError> {
    let command = resolver.resolve_string(&dto.command)?;
    Ok(TemplateSpec::new(&command))
}
