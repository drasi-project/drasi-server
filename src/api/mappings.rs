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

//! Conversion implementations between DTO and external configuration types.
//!
//! This module provides bidirectional conversions (Into trait implementations)
//! between our DTO configuration types and the external plugin library types.

use super::models::{
    grpc_reaction::*, grpc_source::*, http_reaction::*, http_source::*, log::*, mock::*,
    platform_reaction::*, platform_source::*, postgres::*, profiler::*, sse::*,
};

// External library imports
use drasi_reaction_grpc::GrpcReactionConfig;
use drasi_reaction_grpc_adaptive::GrpcAdaptiveReactionConfig;
use drasi_reaction_http::{CallSpec, HttpReactionConfig, QueryConfig};
use drasi_reaction_http_adaptive::HttpAdaptiveReactionConfig;
use drasi_reaction_log::LogReactionConfig;
use drasi_reaction_platform::PlatformReactionConfig;
use drasi_reaction_profiler::ProfilerReactionConfig;
use drasi_reaction_sse::SseReactionConfig;
use drasi_source_grpc::GrpcSourceConfig;
use drasi_source_http::HttpSourceConfig;
use drasi_source_mock::MockSourceConfig;
use drasi_source_platform::PlatformSourceConfig;
use drasi_source_postgres::{PostgresSourceConfig, SslMode, TableKeyConfig};

// =============================================================================
// Source Configuration Conversions
// =============================================================================

// PostgreSQL Source
impl From<PostgresSourceConfigDto> for PostgresSourceConfig {
    fn from(local: PostgresSourceConfigDto) -> Self {
        Self {
            host: local.host,
            port: local.port,
            database: local.database,
            user: local.user,
            password: local.password,
            tables: local.tables,
            slot_name: local.slot_name,
            publication_name: local.publication_name,
            ssl_mode: local.ssl_mode.into(),
            table_keys: local.table_keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<PostgresSourceConfig> for PostgresSourceConfigDto {
    fn from(external: PostgresSourceConfig) -> Self {
        Self {
            host: external.host,
            port: external.port,
            database: external.database,
            user: external.user,
            password: external.password,
            tables: external.tables,
            slot_name: external.slot_name,
            publication_name: external.publication_name,
            ssl_mode: external.ssl_mode.into(),
            table_keys: external.table_keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<SslModeDto> for SslMode {
    fn from(local: SslModeDto) -> Self {
        match local {
            SslModeDto::Disable => SslMode::Disable,
            SslModeDto::Prefer => SslMode::Prefer,
            SslModeDto::Require => SslMode::Require,
        }
    }
}

impl From<SslMode> for SslModeDto {
    fn from(external: SslMode) -> Self {
        match external {
            SslMode::Disable => SslModeDto::Disable,
            SslMode::Prefer => SslModeDto::Prefer,
            SslMode::Require => SslModeDto::Require,
        }
    }
}

impl From<TableKeyConfigDto> for TableKeyConfig {
    fn from(local: TableKeyConfigDto) -> Self {
        Self {
            table: local.table,
            key_columns: local.key_columns,
        }
    }
}

impl From<TableKeyConfig> for TableKeyConfigDto {
    fn from(external: TableKeyConfig) -> Self {
        Self {
            table: external.table,
            key_columns: external.key_columns,
        }
    }
}

// HTTP Source
impl From<HttpSourceConfigDto> for HttpSourceConfig {
    fn from(local: HttpSourceConfigDto) -> Self {
        Self {
            host: local.host,
            port: local.port,
            endpoint: local.endpoint,
            timeout_ms: local.timeout_ms,
            adaptive_max_batch_size: local.adaptive_max_batch_size,
            adaptive_min_batch_size: local.adaptive_min_batch_size,
            adaptive_max_wait_ms: local.adaptive_max_wait_ms,
            adaptive_min_wait_ms: local.adaptive_min_wait_ms,
            adaptive_window_secs: local.adaptive_window_secs,
            adaptive_enabled: local.adaptive_enabled,
        }
    }
}

impl From<HttpSourceConfig> for HttpSourceConfigDto {
    fn from(external: HttpSourceConfig) -> Self {
        Self {
            host: external.host,
            port: external.port,
            endpoint: external.endpoint,
            timeout_ms: external.timeout_ms,
            adaptive_max_batch_size: external.adaptive_max_batch_size,
            adaptive_min_batch_size: external.adaptive_min_batch_size,
            adaptive_max_wait_ms: external.adaptive_max_wait_ms,
            adaptive_min_wait_ms: external.adaptive_min_wait_ms,
            adaptive_window_secs: external.adaptive_window_secs,
            adaptive_enabled: external.adaptive_enabled,
        }
    }
}

// gRPC Source
impl From<GrpcSourceConfigDto> for GrpcSourceConfig {
    fn from(local: GrpcSourceConfigDto) -> Self {
        Self {
            host: local.host,
            port: local.port,
            endpoint: local.endpoint,
            timeout_ms: local.timeout_ms,
        }
    }
}

impl From<GrpcSourceConfig> for GrpcSourceConfigDto {
    fn from(external: GrpcSourceConfig) -> Self {
        Self {
            host: external.host,
            port: external.port,
            endpoint: external.endpoint,
            timeout_ms: external.timeout_ms,
        }
    }
}

// Mock Source
impl From<MockSourceConfigDto> for MockSourceConfig {
    fn from(local: MockSourceConfigDto) -> Self {
        Self {
            data_type: local.data_type,
            interval_ms: local.interval_ms,
        }
    }
}

impl From<MockSourceConfig> for MockSourceConfigDto {
    fn from(external: MockSourceConfig) -> Self {
        Self {
            data_type: external.data_type,
            interval_ms: external.interval_ms,
        }
    }
}

// Platform Source
impl From<PlatformSourceConfigDto> for PlatformSourceConfig {
    fn from(local: PlatformSourceConfigDto) -> Self {
        Self {
            redis_url: local.redis_url,
            stream_key: local.stream_key,
            consumer_group: local.consumer_group,
            consumer_name: local.consumer_name,
            batch_size: local.batch_size,
            block_ms: local.block_ms,
        }
    }
}

impl From<PlatformSourceConfig> for PlatformSourceConfigDto {
    fn from(external: PlatformSourceConfig) -> Self {
        Self {
            redis_url: external.redis_url,
            stream_key: external.stream_key,
            consumer_group: external.consumer_group,
            consumer_name: external.consumer_name,
            batch_size: external.batch_size,
            block_ms: external.block_ms,
        }
    }
}

// =============================================================================
// Reaction Configuration Conversions
// =============================================================================

// HTTP Reaction
impl From<HttpReactionConfigDto> for HttpReactionConfig {
    fn from(local: HttpReactionConfigDto) -> Self {
        Self {
            base_url: local.base_url,
            token: local.token,
            timeout_ms: local.timeout_ms,
            routes: local
                .routes
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl From<HttpReactionConfig> for HttpReactionConfigDto {
    fn from(external: HttpReactionConfig) -> Self {
        Self {
            base_url: external.base_url,
            token: external.token,
            timeout_ms: external.timeout_ms,
            routes: external
                .routes
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl From<QueryConfigDto> for QueryConfig {
    fn from(local: QueryConfigDto) -> Self {
        Self {
            added: local.added.map(Into::into),
            updated: local.updated.map(Into::into),
            deleted: local.deleted.map(Into::into),
        }
    }
}

impl From<QueryConfig> for QueryConfigDto {
    fn from(external: QueryConfig) -> Self {
        Self {
            added: external.added.map(Into::into),
            updated: external.updated.map(Into::into),
            deleted: external.deleted.map(Into::into),
        }
    }
}

impl From<CallSpecDto> for CallSpec {
    fn from(local: CallSpecDto) -> Self {
        Self {
            url: local.url,
            method: local.method,
            body: local.body,
            headers: local.headers,
        }
    }
}

impl From<CallSpec> for CallSpecDto {
    fn from(external: CallSpec) -> Self {
        Self {
            url: external.url,
            method: external.method,
            body: external.body,
            headers: external.headers,
        }
    }
}

// HTTP Adaptive Reaction
impl From<HttpAdaptiveReactionConfigDto> for HttpAdaptiveReactionConfig {
    fn from(local: HttpAdaptiveReactionConfigDto) -> Self {
        Self {
            base_url: local.base_url,
            token: local.token,
            timeout_ms: local.timeout_ms,
            routes: local
                .routes
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            adaptive: local.adaptive.into(),
        }
    }
}

impl From<HttpAdaptiveReactionConfig> for HttpAdaptiveReactionConfigDto {
    fn from(external: HttpAdaptiveReactionConfig) -> Self {
        Self {
            base_url: external.base_url,
            token: external.token,
            timeout_ms: external.timeout_ms,
            routes: external
                .routes
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            adaptive: external.adaptive.into(),
        }
    }
}

impl From<AdaptiveBatchConfigDto> for drasi_lib::reactions::common::AdaptiveBatchConfig {
    fn from(local: AdaptiveBatchConfigDto) -> Self {
        Self {
            adaptive_min_batch_size: local.adaptive_min_batch_size,
            adaptive_max_batch_size: local.adaptive_max_batch_size,
            adaptive_window_size: local.adaptive_window_size,
            adaptive_batch_timeout_ms: local.adaptive_batch_timeout_ms,
        }
    }
}

impl From<drasi_lib::reactions::common::AdaptiveBatchConfig> for AdaptiveBatchConfigDto {
    fn from(external: drasi_lib::reactions::common::AdaptiveBatchConfig) -> Self {
        Self {
            adaptive_min_batch_size: external.adaptive_min_batch_size,
            adaptive_max_batch_size: external.adaptive_max_batch_size,
            adaptive_window_size: external.adaptive_window_size,
            adaptive_batch_timeout_ms: external.adaptive_batch_timeout_ms,
        }
    }
}

// gRPC Reaction
impl From<GrpcReactionConfigDto> for GrpcReactionConfig {
    fn from(local: GrpcReactionConfigDto) -> Self {
        Self {
            endpoint: local.endpoint,
            timeout_ms: local.timeout_ms,
            batch_size: local.batch_size,
            batch_flush_timeout_ms: local.batch_flush_timeout_ms,
            max_retries: local.max_retries,
            connection_retry_attempts: local.connection_retry_attempts,
            initial_connection_timeout_ms: local.initial_connection_timeout_ms,
            metadata: local.metadata,
        }
    }
}

impl From<GrpcReactionConfig> for GrpcReactionConfigDto {
    fn from(external: GrpcReactionConfig) -> Self {
        Self {
            endpoint: external.endpoint,
            timeout_ms: external.timeout_ms,
            batch_size: external.batch_size,
            batch_flush_timeout_ms: external.batch_flush_timeout_ms,
            max_retries: external.max_retries,
            connection_retry_attempts: external.connection_retry_attempts,
            initial_connection_timeout_ms: external.initial_connection_timeout_ms,
            metadata: external.metadata,
        }
    }
}

// gRPC Adaptive Reaction
impl From<GrpcAdaptiveReactionConfigDto> for GrpcAdaptiveReactionConfig {
    fn from(local: GrpcAdaptiveReactionConfigDto) -> Self {
        Self {
            endpoint: local.endpoint,
            timeout_ms: local.timeout_ms,
            max_retries: local.max_retries,
            connection_retry_attempts: local.connection_retry_attempts,
            initial_connection_timeout_ms: local.initial_connection_timeout_ms,
            metadata: local.metadata,
            adaptive: local.adaptive.into(),
        }
    }
}

impl From<GrpcAdaptiveReactionConfig> for GrpcAdaptiveReactionConfigDto {
    fn from(external: GrpcAdaptiveReactionConfig) -> Self {
        Self {
            endpoint: external.endpoint,
            timeout_ms: external.timeout_ms,
            max_retries: external.max_retries,
            connection_retry_attempts: external.connection_retry_attempts,
            initial_connection_timeout_ms: external.initial_connection_timeout_ms,
            metadata: external.metadata,
            adaptive: external.adaptive.into(),
        }
    }
}

// SSE Reaction
impl From<SseReactionConfigDto> for SseReactionConfig {
    fn from(local: SseReactionConfigDto) -> Self {
        Self {
            host: local.host,
            port: local.port,
            sse_path: local.sse_path,
            heartbeat_interval_ms: local.heartbeat_interval_ms,
        }
    }
}

impl From<SseReactionConfig> for SseReactionConfigDto {
    fn from(external: SseReactionConfig) -> Self {
        Self {
            host: external.host,
            port: external.port,
            sse_path: external.sse_path,
            heartbeat_interval_ms: external.heartbeat_interval_ms,
        }
    }
}

// Log Reaction
impl From<LogReactionConfigDto> for LogReactionConfig {
    fn from(local: LogReactionConfigDto) -> Self {
        Self {
            added_template: local.added_template,
            updated_template: local.updated_template,
            deleted_template: local.deleted_template,
        }
    }
}

impl From<LogReactionConfig> for LogReactionConfigDto {
    fn from(external: LogReactionConfig) -> Self {
        Self {
            added_template: external.added_template,
            updated_template: external.updated_template,
            deleted_template: external.deleted_template,
        }
    }
}

// Platform Reaction
impl From<PlatformReactionConfigDto> for PlatformReactionConfig {
    fn from(local: PlatformReactionConfigDto) -> Self {
        Self {
            redis_url: local.redis_url,
            pubsub_name: local.pubsub_name,
            source_name: local.source_name,
            max_stream_length: local.max_stream_length,
            emit_control_events: local.emit_control_events,
            batch_enabled: local.batch_enabled,
            batch_max_size: local.batch_max_size,
            batch_max_wait_ms: local.batch_max_wait_ms,
        }
    }
}

impl From<PlatformReactionConfig> for PlatformReactionConfigDto {
    fn from(external: PlatformReactionConfig) -> Self {
        Self {
            redis_url: external.redis_url,
            pubsub_name: external.pubsub_name,
            source_name: external.source_name,
            max_stream_length: external.max_stream_length,
            emit_control_events: external.emit_control_events,
            batch_enabled: external.batch_enabled,
            batch_max_size: external.batch_max_size,
            batch_max_wait_ms: external.batch_max_wait_ms,
        }
    }
}

// Profiler Reaction
impl From<ProfilerReactionConfigDto> for ProfilerReactionConfig {
    fn from(local: ProfilerReactionConfigDto) -> Self {
        Self {
            window_size: local.window_size,
            report_interval_secs: local.report_interval_secs,
        }
    }
}

impl From<ProfilerReactionConfig> for ProfilerReactionConfigDto {
    fn from(external: ProfilerReactionConfig) -> Self {
        Self {
            window_size: external.window_size,
            report_interval_secs: external.report_interval_secs,
        }
    }
}
