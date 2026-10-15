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
use drasi_lib::{
    ComponentEvent, ComponentLogType, ComponentStatus, ComponentType, LogLevel, LogMessage,
};
use serde::{Deserialize, Serialize};

/// Component type for observability payloads.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = ComponentType)]
pub enum ComponentTypeDto {
    Source,
    Query,
    Reaction,
    BootstrapProvider,
    IdentityProvider,
}

impl From<ComponentType> for ComponentTypeDto {
    fn from(component_type: ComponentType) -> Self {
        match component_type {
            ComponentType::Source => Self::Source,
            ComponentType::Query => Self::Query,
            ComponentType::Reaction => Self::Reaction,
            ComponentType::BootstrapProvider => Self::BootstrapProvider,
            ComponentType::IdentityProvider => Self::IdentityProvider,
        }
    }
}

/// Log categories, including native roles without extending lifecycle events.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = ComponentLogType)]
pub enum ComponentLogTypeDto {
    Source,
    Query,
    Reaction,
    BootstrapProvider,
    IdentityProvider,
    Transformer,
    Sink,
    Service,
}

impl From<ComponentLogType> for ComponentLogTypeDto {
    fn from(component_type: ComponentLogType) -> Self {
        match component_type {
            ComponentLogType::Source => Self::Source,
            ComponentLogType::Query => Self::Query,
            ComponentLogType::Reaction => Self::Reaction,
            ComponentLogType::BootstrapProvider => Self::BootstrapProvider,
            ComponentLogType::IdentityProvider => Self::IdentityProvider,
            ComponentLogType::Transformer => Self::Transformer,
            ComponentLogType::Sink => Self::Sink,
            ComponentLogType::Service => Self::Service,
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
    Reconfiguring,
    /// Structural event: component was registered in the graph.
    Added,
    /// Structural event: component was deregistered from the graph.
    Removed,
}

impl From<ComponentStatus> for ComponentStatusDto {
    fn from(status: ComponentStatus) -> Self {
        match status {
            ComponentStatus::Starting => Self::Starting,
            ComponentStatus::Running => Self::Running,
            ComponentStatus::Stopping => Self::Stopping,
            ComponentStatus::Stopped => Self::Stopped,
            ComponentStatus::Error => Self::Error,
            ComponentStatus::Reconfiguring => Self::Reconfiguring,
            ComponentStatus::Added => Self::Added,
            ComponentStatus::Removed => Self::Removed,
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
    #[schema(value_type = ComponentType)]
    pub component_type: ComponentTypeDto,
    #[schema(value_type = ComponentStatus)]
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
    #[schema(value_type = LogLevel)]
    pub level: LogLevelDto,
    pub message: String,
    pub component_id: String,
    #[schema(value_type = ComponentLogType)]
    pub component_type: ComponentLogTypeDto,
}

impl From<LogMessage> for LogMessageDto {
    fn from(message: LogMessage) -> Self {
        Self {
            timestamp: message.timestamp,
            level: LogLevelDto::from(message.level),
            message: message.message,
            component_id: message.component_id,
            component_type: ComponentLogTypeDto::from(message.component_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    #[test]
    fn log_payloads_preserve_native_roles_and_existing_fields() {
        for (category, name) in [
            (ComponentLogType::Source, "Source"),
            (ComponentLogType::Query, "Query"),
            (ComponentLogType::Reaction, "Reaction"),
            (ComponentLogType::BootstrapProvider, "BootstrapProvider"),
            (ComponentLogType::IdentityProvider, "IdentityProvider"),
            (ComponentLogType::Transformer, "Transformer"),
            (ComponentLogType::Sink, "Sink"),
            (ComponentLogType::Service, "Service"),
        ] {
            let message = LogMessage::with_instance(
                LogLevel::Info,
                "started",
                "instance",
                "component",
                category,
            );
            let timestamp = message.timestamp;
            let payload = serde_json::to_value(LogMessageDto::from(message)).unwrap();
            assert_eq!(
                payload,
                serde_json::json!({
                    "timestamp": timestamp,
                    "level": "Info",
                    "message": "started",
                    "componentId": "component",
                    "componentType": name
                })
            );
            let decoded: LogMessageDto = serde_json::from_value(payload.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), payload);
        }
    }

    #[test]
    fn log_schema_adds_native_roles_without_changing_event_categories() {
        let document = serde_json::to_value(crate::api::v1::openapi::ApiDocV1::openapi()).unwrap();
        let schemas = &document["components"]["schemas"];
        assert_eq!(
            schemas["ComponentType"]["enum"],
            serde_json::json!([
                "Source",
                "Query",
                "Reaction",
                "BootstrapProvider",
                "IdentityProvider"
            ])
        );
        assert_eq!(
            schemas["ComponentLogType"]["enum"],
            serde_json::json!([
                "Source",
                "Query",
                "Reaction",
                "BootstrapProvider",
                "IdentityProvider",
                "Transformer",
                "Sink",
                "Service"
            ])
        );
        assert_eq!(
            schemas["LogMessage"]["properties"]["componentType"]["$ref"],
            "#/components/schemas/ComponentLogType"
        );
        assert_eq!(
            schemas["ComponentEvent"]["properties"]["componentType"]["$ref"],
            "#/components/schemas/ComponentType"
        );
    }
}
