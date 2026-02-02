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

//! DTOs for component events and logs.

use chrono::{DateTime, Utc};
use drasi_lib::{ComponentEvent, ComponentStatus, ComponentType, LogLevel, LogMessage};
use serde::{Deserialize, Serialize};

/// Component type for observability payloads.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = ComponentType)]
pub enum ComponentTypeDto {
    Source,
    Query,
    Reaction,
}

impl From<ComponentType> for ComponentTypeDto {
    fn from(component_type: ComponentType) -> Self {
        match component_type {
            ComponentType::Source => Self::Source,
            ComponentType::Query => Self::Query,
            ComponentType::Reaction => Self::Reaction,
        }
    }
}

/// Component status for observability payloads.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = ComponentStatus)]
pub enum ComponentStatusDto {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

impl From<ComponentStatus> for ComponentStatusDto {
    fn from(status: ComponentStatus) -> Self {
        match status {
            ComponentStatus::Starting => Self::Starting,
            ComponentStatus::Running => Self::Running,
            ComponentStatus::Stopping => Self::Stopping,
            ComponentStatus::Stopped => Self::Stopped,
            ComponentStatus::Error => Self::Error,
        }
    }
}

/// Log level for observability payloads.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = LogLevel)]
pub enum LogLevelDto {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LogLevelDto {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => Self::Trace,
            LogLevel::Debug => Self::Debug,
            LogLevel::Info => Self::Info,
            LogLevel::Warn => Self::Warn,
            LogLevel::Error => Self::Error,
        }
    }
}

/// Component lifecycle event payload.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = ComponentEvent)]
#[serde(rename_all = "camelCase")]
pub struct ComponentEventDto {
    pub component_id: String,
    pub component_type: ComponentTypeDto,
    pub status: ComponentStatusDto,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl From<ComponentEvent> for ComponentEventDto {
    fn from(event: ComponentEvent) -> Self {
        Self {
            component_id: event.component_id,
            component_type: ComponentTypeDto::from(event.component_type),
            status: ComponentStatusDto::from(event.status),
            timestamp: event.timestamp,
            message: event.message,
        }
    }
}

/// Component log message payload.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = LogMessage)]
#[serde(rename_all = "camelCase")]
pub struct LogMessageDto {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevelDto,
    pub message: String,
    pub component_id: String,
    pub component_type: ComponentTypeDto,
}

impl From<LogMessage> for LogMessageDto {
    fn from(message: LogMessage) -> Self {
        Self {
            timestamp: message.timestamp,
            level: LogLevelDto::from(message.level),
            message: message.message,
            component_id: message.component_id,
            component_type: ComponentTypeDto::from(message.component_type),
        }
    }
}
