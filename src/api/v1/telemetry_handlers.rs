// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Read-only telemetry inspection, mounted only when
//! `telemetry.inspection.enabled` is true.
//!
//! Settings are projected once at startup and never include destinations,
//! headers or credentials. The metrics route delegates to the shared
//! Prometheus scrape runtime and never installs a backend of its own.

use std::sync::Arc;

use axum::{extract::State, response::Response, routing::get, Json, Router};
use drasi_lib::observability::{RootSampling, TelemetryProfile};
use serde::Serialize;

use crate::api::shared::error::{error_codes, ErrorResponse};
use crate::config::telemetry::{EffectiveTelemetry, MetricsBackend};
use crate::telemetry::prometheus::PrometheusRuntime;

const TELEMETRY_PATH: &str = "/api/v1/telemetry";
pub const TELEMETRY_METRICS_PATH: &str = "/api/v1/telemetry/metrics";
pub const LOG_PATH_TEMPLATE: &str =
    "/api/v1/instances/{instanceId}/{collection}/{componentId}/logs";
pub const LOG_STREAM_PATH_TEMPLATE: &str =
    "/api/v1/instances/{instanceId}/{collection}/{componentId}/logs/stream";

/// Effective telemetry profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum InspectionProfile {
    Off,
    Basic,
    Debug,
    Persistence,
}

impl From<TelemetryProfile> for InspectionProfile {
    fn from(profile: TelemetryProfile) -> Self {
        match profile {
            TelemetryProfile::Off => Self::Off,
            TelemetryProfile::Basic => Self::Basic,
            TelemetryProfile::Debug => Self::Debug,
            TelemetryProfile::Persistence => Self::Persistence,
        }
    }
}

/// Effective metrics backend; `none` whenever the profile is `off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum InspectionMetricsBackend {
    None,
    Prometheus,
    Otlp,
}

impl From<MetricsBackend> for InspectionMetricsBackend {
    fn from(backend: MetricsBackend) -> Self {
        match backend {
            MetricsBackend::None => Self::None,
            MetricsBackend::Prometheus => Self::Prometheus,
            MetricsBackend::Otlp => Self::Otlp,
        }
    }
}

/// Effective telemetry settings, computed once at startup.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryInspectionResponse {
    pub profile: InspectionProfile,
    pub tracing: TracingInspection,
    pub metrics: MetricsInspection,
    pub logs: LogsInspection,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TracingInspection {
    /// A trace destination is configured and the profile is not `off`; not collector health.
    pub export_enabled: bool,
    /// Configured root sampling ratio in [0, 1].
    pub sample_ratio: f64,
    /// Always `false`; no trace history is stored.
    pub local_inspection: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MetricsInspection {
    pub backend: InspectionMetricsBackend,
    /// Effective process metrics; per-OS availability is documented, not returned.
    pub process_metrics: bool,
    /// Local metrics route when the Prometheus backend is active, otherwise `null`.
    #[schema(required = true)]
    pub path: Option<String>,
}

/// Relative templates for the existing source, query and reaction log routes.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogsInspection {
    pub path_template: String,
    pub stream_path_template: String,
}

impl TelemetryInspectionResponse {
    pub(crate) fn from_effective(effective: &EffectiveTelemetry) -> Self {
        let policy = effective.policy();
        let sample_ratio = match policy.sampling() {
            RootSampling::Always => 1.0,
            RootSampling::Never => 0.0,
            RootSampling::Ratio(ratio) => ratio.value(),
        };
        Self {
            profile: policy.profile().into(),
            tracing: TracingInspection {
                export_enabled: effective.tracing.is_some(),
                sample_ratio,
                local_inspection: false,
            },
            metrics: MetricsInspection {
                backend: effective.metrics_backend.into(),
                process_metrics: effective.process_metrics
                    && effective.metrics_backend != MetricsBackend::None,
                path: effective
                    .prometheus
                    .as_ref()
                    .map(|_| TELEMETRY_METRICS_PATH.to_string()),
            },
            logs: LogsInspection {
                path_template: LOG_PATH_TEMPLATE.to_string(),
                stream_path_template: LOG_STREAM_PATH_TEMPLATE.to_string(),
            },
        }
    }
}

/// Immutable inspection state shared by both routes.
pub(crate) struct TelemetryInspection {
    settings: TelemetryInspectionResponse,
    prometheus: Option<Arc<PrometheusRuntime>>,
}

impl TelemetryInspection {
    pub(crate) fn new(
        effective: &EffectiveTelemetry,
        prometheus: Option<Arc<PrometheusRuntime>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            settings: TelemetryInspectionResponse::from_effective(effective),
            prometheus,
        })
    }
}

/// GET-only inspection routes; other methods receive axum's 405.
pub(crate) fn telemetry_router(inspection: Arc<TelemetryInspection>) -> Router {
    Router::new()
        .route(TELEMETRY_PATH, get(get_telemetry))
        .route(TELEMETRY_METRICS_PATH, get(get_telemetry_metrics))
        .with_state(inspection)
}

/// Read the effective telemetry settings.
///
/// This route exists only when `telemetry.inspection.enabled` is true. It
/// returns a startup projection without destinations or credentials.
#[utoipa::path(
    get,
    path = "/api/v1/telemetry",
    tag = "Telemetry",
    responses(
        (status = 200, description = "Effective telemetry settings", body = TelemetryInspectionResponse)
    )
)]
pub async fn get_telemetry(
    State(inspection): State<Arc<TelemetryInspection>>,
) -> Json<TelemetryInspectionResponse> {
    Json(inspection.settings.clone())
}

/// Read local Prometheus metrics.
///
/// This route exists only when `telemetry.inspection.enabled` is true. It
/// shares the `/metrics` scrape admission, wait and render job.
#[utoipa::path(
    get,
    path = "/api/v1/telemetry/metrics",
    tag = "Telemetry",
    responses(
        (status = 200, description = "Prometheus text exposition", body = String, content_type = "text/plain; version=0.0.4; charset=utf-8"),
        (status = 503, description = "No active Prometheus backend, or the scrape is overloaded, timed out or shutting down", body = ErrorResponse),
        (status = 500, description = "Local metrics could not be read", body = ErrorResponse)
    )
)]
pub async fn get_telemetry_metrics(
    State(inspection): State<Arc<TelemetryInspection>>,
) -> Result<Response, ErrorResponse> {
    let Some(runtime) = &inspection.prometheus else {
        return Err(ErrorResponse::new(
            error_codes::TELEMETRY_METRICS_UNAVAILABLE,
            "Local metrics are not available for this telemetry configuration",
        ));
    };
    runtime.scrape().await.map_err(|error| {
        if error.code == error_codes::TELEMETRY_SCRAPE_UNAVAILABLE {
            ErrorResponse::new(
                error_codes::TELEMETRY_METRICS_UNAVAILABLE,
                "Local metrics are temporarily unavailable",
            )
        } else {
            ErrorResponse::new(
                error_codes::TELEMETRY_INSPECTION_FAILED,
                "Failed to read local metrics",
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::mappings::DtoMapper, config::telemetry::TelemetryConfig};
    use axum::{
        body::{to_bytes, Body},
        http::{header::CONTENT_TYPE, Method, Request, StatusCode},
    };
    use serde_json::json;
    use tower::ServiceExt;

    fn effective(value: serde_json::Value) -> EffectiveTelemetry {
        serde_json::from_value::<TelemetryConfig>(value)
            .unwrap()
            .resolve(&DtoMapper::new(), 8080, true)
            .unwrap()
    }

    fn settings(value: serde_json::Value) -> serde_json::Value {
        serde_json::to_value(TelemetryInspectionResponse::from_effective(&effective(
            value,
        )))
        .unwrap()
    }

    #[test]
    fn settings_project_effective_policy_without_destinations_or_credentials() {
        std::env::set_var("OBS06_INSPECTION_SECRET", "s3cr3t-value");
        let response = TelemetryInspectionResponse::from_effective(&effective(json!({
            "profile": "basic",
            "inspection": {"enabled": true},
            "tracing": {
                "endpoint": "https://collector.example.test/v1/traces",
                "protocol": "http/protobuf",
                "sampleRatio": 0.25,
                "headers": {"x-obs06-key": "${OBS06_INSPECTION_SECRET}"}
            },
            "metrics": {"backend": "prometheus", "processMetrics": true}
        })));
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            json!({
                "profile": "basic",
                "tracing": {"exportEnabled": true, "sampleRatio": 0.25, "localInspection": false},
                "metrics": {
                    "backend": "prometheus",
                    "processMetrics": true,
                    "path": "/api/v1/telemetry/metrics"
                },
                "logs": {
                    "pathTemplate": LOG_PATH_TEMPLATE,
                    "streamPathTemplate": LOG_STREAM_PATH_TEMPLATE
                }
            })
        );
        let serialized = serde_json::to_string(&response).unwrap();
        for hidden in [
            "collector.example.test",
            "https",
            "x-obs06-key",
            "s3cr3t-value",
            "OBS06_INSPECTION_SECRET",
        ] {
            assert!(
                !serialized.contains(hidden),
                "{hidden} leaked: {serialized}"
            );
        }
    }

    #[test]
    fn settings_under_off_or_otlp_report_no_local_metrics() {
        let off = settings(json!({
            "profile": "off",
            "inspection": {"enabled": true},
            "tracing": {
                "endpoint": "http://collector.example.test/v1/traces",
                "protocol": "http/protobuf",
                "sampleRatio": 0.5
            },
            "metrics": {"backend": "prometheus", "prometheus": {"port": 9464}}
        }));
        assert_eq!(off["profile"], "off");
        assert_eq!(
            off["tracing"],
            json!({"exportEnabled": false, "sampleRatio": 0.5, "localInspection": false})
        );
        assert_eq!(
            off["metrics"],
            json!({"backend": "none", "processMetrics": false, "path": null})
        );

        let otlp = settings(json!({
            "profile": "basic",
            "inspection": {"enabled": true},
            "metrics": {
                "backend": "otlp",
                "processMetrics": true,
                "otlp": {
                    "endpoint": "http://collector.example.test/v1/metrics",
                    "protocol": "http/protobuf"
                }
            }
        }));
        assert_eq!(
            otlp["metrics"],
            json!({"backend": "otlp", "processMetrics": true, "path": null})
        );
    }

    #[tokio::test]
    async fn mutations_return_405_and_settings_reads_are_stable() {
        let router = telemetry_router(TelemetryInspection::new(
            &effective(json!({
                "profile": "basic",
                "inspection": {"enabled": true},
                "metrics": {"backend": "none"}
            })),
            None,
        ));
        for path in [TELEMETRY_PATH, TELEMETRY_METRICS_PATH] {
            for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method.clone())
                            .uri(path)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::METHOD_NOT_ALLOWED,
                    "{method} {path}"
                );
            }
        }
        let mut bodies = Vec::new();
        for _ in 0..2 {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(TELEMETRY_PATH)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
            bodies.push(to_bytes(response.into_body(), 64 * 1024).await.unwrap());
        }
        assert_eq!(bodies[0], bodies[1]);
    }
}
