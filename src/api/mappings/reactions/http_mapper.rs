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

//! HTTP reaction configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::*;
use drasi_reaction_http::{CallSpec, HttpReactionConfig, QueryConfig};
use std::collections::HashMap;

pub struct HttpReactionConfigMapper;

impl ConfigMapper<HttpReactionConfigDto, HttpReactionConfig> for HttpReactionConfigMapper {
    fn map(
        &self,
        dto: &HttpReactionConfigDto,
        resolver: &DtoMapper,
    ) -> Result<HttpReactionConfig, MappingError> {
        let mut routes = HashMap::new();
        for (key, query_dto) in &dto.routes {
            let added = if let Some(call_dto) = &query_dto.added {
                Some(CallSpec {
                    url: resolver.resolve_string(&call_dto.url)?,
                    method: resolver.resolve_string(&call_dto.method)?,
                    body: resolver.resolve_string(&call_dto.body)?,
                    headers: resolve_hashmap(&call_dto.headers, resolver)?,
                })
            } else {
                None
            };

            let updated = if let Some(call_dto) = &query_dto.updated {
                Some(CallSpec {
                    url: resolver.resolve_string(&call_dto.url)?,
                    method: resolver.resolve_string(&call_dto.method)?,
                    body: resolver.resolve_string(&call_dto.body)?,
                    headers: resolve_hashmap(&call_dto.headers, resolver)?,
                })
            } else {
                None
            };

            let deleted = if let Some(call_dto) = &query_dto.deleted {
                Some(CallSpec {
                    url: resolver.resolve_string(&call_dto.url)?,
                    method: resolver.resolve_string(&call_dto.method)?,
                    body: resolver.resolve_string(&call_dto.body)?,
                    headers: resolve_hashmap(&call_dto.headers, resolver)?,
                })
            } else {
                None
            };

            routes.insert(
                key.clone(),
                QueryConfig {
                    added,
                    updated,
                    deleted,
                },
            );
        }

        Ok(HttpReactionConfig {
            base_url: resolver.resolve_string(&dto.base_url)?,
            token: resolver.resolve_optional(&dto.token)?,
            timeout_ms: resolver.resolve_typed(&dto.timeout_ms)?,
            routes,
        })
    }
}

// Helper function to resolve HashMap<String, ConfigValue<String>>
fn resolve_hashmap(
    map: &HashMap<String, ConfigValue<String>>,
    resolver: &DtoMapper,
) -> Result<HashMap<String, String>, MappingError> {
    let mut result = HashMap::new();
    for (key, value) in map {
        result.insert(key.clone(), resolver.resolve_string(value)?);
    }
    Ok(result)
}
