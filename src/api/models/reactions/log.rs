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

//! Log reaction configuration DTOs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Template specification for log output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateSpecDto {
    /// Output template as a Handlebars template
    #[serde(default)]
    pub template: String,
}

/// Configuration for query-specific log output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryConfigDto {
    /// Template for ADD operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<TemplateSpecDto>,
    /// Template for UPDATE operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<TemplateSpecDto>,
    /// Template for DELETE operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<TemplateSpecDto>,
}

/// Local copy of log reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogReactionConfigDto {
    /// Query-specific template configurations
    #[serde(default)]
    pub routes: HashMap<String, QueryConfigDto>,
    /// Default template configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_template: Option<QueryConfigDto>,
}
