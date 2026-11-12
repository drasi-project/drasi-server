// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Startup-only telemetry configuration. Resolution creates no providers or listeners.

use std::{collections::BTreeMap, fmt, str::FromStr, time::Duration};

use anyhow::{bail, ensure, Result};
use drasi_lib::observability::{
    ResolvedTelemetry, RootSampling, SamplingRatio, TelemetryProfile, TelemetrySettings,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::api::{mappings::DtoMapper, models::ConfigValue};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelemetryConfig {
    #[serde(default = "default_profile")]
    #[schema(schema_with = string_schema)]
    pub profile: ConfigValue<String>,
    #[serde(default)]
    pub inspection: InspectionConfig,
    #[serde(default)]
    pub filters: TelemetryFilters,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub metrics: TelemetryMetricsConfig,
    #[serde(default = "default_shutdown_timeout")]
    #[schema(schema_with = integer_schema)]
    pub shutdown_timeout_seconds: ConfigValue<u64>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            profile: default_profile(),
            inspection: Default::default(),
            filters: Default::default(),
            tracing: Default::default(),
            metrics: Default::default(),
            shutdown_timeout_seconds: default_shutdown_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectionConfig {
    #[serde(default = "default_false")]
    #[schema(schema_with = bool_schema)]
    pub enabled: ConfigValue<bool>,
}

impl Default for InspectionConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelemetryFilters {
    /// Literal metric-name substrings, not glob patterns.
    #[serde(default)]
    #[schema(schema_with = exclusions_schema)]
    pub exclude: Vec<ConfigValue<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TracingConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(schema_with = string_schema)]
    pub endpoint: Option<ConfigValue<String>>,
    /// Must explicitly select `http/protobuf` when an endpoint is configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(schema_with = string_schema)]
    pub protocol: Option<ConfigValue<String>>,
    #[serde(default = "default_service_name")]
    #[schema(schema_with = string_schema)]
    pub service_name: ConfigValue<String>,
    #[serde(default = "default_sample_ratio")]
    #[schema(schema_with = number_schema)]
    pub sample_ratio: ConfigValue<f64>,
    /// Header values must be environment references; authenticated export requires HTTPS.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(schema_with = headers_schema)]
    pub headers: BTreeMap<String, ConfigValue<String>>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            protocol: None,
            service_name: default_service_name(),
            sample_ratio: default_sample_ratio(),
            headers: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelemetryMetricsConfig {
    #[serde(default = "default_backend")]
    #[schema(schema_with = string_schema)]
    pub backend: ConfigValue<String>,
    #[serde(default = "default_false")]
    #[schema(schema_with = bool_schema)]
    pub process_metrics: ConfigValue<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prometheus: Option<PrometheusConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otlp: Option<OtlpMetricsConfig>,
}

impl Default for TelemetryMetricsConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            process_metrics: default_false(),
            prometheus: None,
            otlp: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrometheusConfig {
    #[serde(default = "default_metrics_path")]
    #[schema(schema_with = string_schema)]
    pub path: ConfigValue<String>,
    /// Omitted uses the management listener; a separate listener inherits its bind address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(schema_with = integer_schema)]
    pub port: Option<ConfigValue<u16>>,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            path: default_metrics_path(),
            port: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OtlpMetricsConfig {
    #[schema(schema_with = string_schema)]
    pub endpoint: ConfigValue<String>,
    #[schema(schema_with = string_schema)]
    pub protocol: ConfigValue<String>,
    #[serde(default = "default_export_interval")]
    #[schema(schema_with = integer_schema)]
    pub export_interval: ConfigValue<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(schema_with = headers_schema)]
    pub headers: BTreeMap<String, ConfigValue<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricsBackend {
    None,
    Prometheus,
    Otlp,
}

/// Validated HTTP/protobuf destination. Debug output deliberately omits endpoint and credentials.
#[derive(Clone)]
pub struct OtlpDestination {
    endpoint: reqwest::Url,
    headers: axum::http::HeaderMap,
}

impl fmt::Debug for OtlpDestination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OtlpDestination { protocol: http/protobuf, .. }")
    }
}

impl OtlpDestination {
    pub fn endpoint(&self) -> &reqwest::Url {
        &self.endpoint
    }

    pub fn headers(&self) -> &axum::http::HeaderMap {
        &self.headers
    }
}

#[derive(Debug, Clone)]
pub struct EffectivePrometheus {
    pub path: String,
    pub port: Option<u16>,
}

/// Runtime-only policy. Never serialize this in place of the original configuration.
#[derive(Debug, Clone)]
pub struct EffectiveTelemetry {
    settings: TelemetrySettings,
    policy: ResolvedTelemetry,
    pub inspection_enabled: bool,
    pub service_name: String,
    pub tracing: Option<OtlpDestination>,
    pub metrics_backend: MetricsBackend,
    pub prometheus: Option<EffectivePrometheus>,
    pub metrics_otlp: Option<OtlpDestination>,
    pub export_interval: Duration,
    pub process_metrics: bool,
    pub shutdown_timeout: Duration,
}

impl EffectiveTelemetry {
    pub fn settings(&self) -> &TelemetrySettings {
        &self.settings
    }

    pub fn policy(&self) -> &ResolvedTelemetry {
        &self.policy
    }

    /// Updated as production capabilities land in OBS-03 through OBS-06.
    pub fn ensure_runtime_available(&self) -> Result<()> {
        let mut unavailable = Vec::new();
        if self.tracing.is_some() {
            unavailable.push("OTLP tracing (OBS-03/OBS-05)");
        }
        match self.metrics_backend {
            MetricsBackend::None => {}
            MetricsBackend::Prometheus => unavailable.push("Prometheus metrics (OBS-03/OBS-04)"),
            MetricsBackend::Otlp => unavailable.push("OTLP metrics (OBS-03/OBS-04)"),
        }
        if self.process_metrics {
            unavailable.push("process metrics (OBS-06)");
        }
        if self.inspection_enabled {
            unavailable.push("inspection API (OBS-06)");
        }
        ensure!(
            unavailable.is_empty(),
            "Telemetry configuration is valid, but this build cannot activate: {}. Disable these capabilities until their runtime support is available.",
            unavailable.join(", ")
        );
        Ok(())
    }
}

impl TelemetryConfig {
    pub fn resolve(
        &self,
        mapper: &DtoMapper,
        management_port: u16,
        enable_ui: bool,
    ) -> Result<EffectiveTelemetry> {
        let profile: String = resolve(mapper, &self.profile, "telemetry.profile")?;
        let profile = TelemetryProfile::from_str(&profile).map_err(|_| {
            anyhow::anyhow!("telemetry.profile must be off, basic, debug or persistence")
        })?;
        let backend: String = resolve(mapper, &self.metrics.backend, "telemetry.metrics.backend")?;
        let backend = match backend.as_str() {
            "none" => MetricsBackend::None,
            "prometheus" => MetricsBackend::Prometheus,
            "otlp" => MetricsBackend::Otlp,
            _ => bail!("telemetry.metrics.backend must be none, prometheus or otlp"),
        };
        let inspection_enabled = resolve(
            mapper,
            &self.inspection.enabled,
            "telemetry.inspection.enabled",
        )?;
        let process_metrics = resolve(
            mapper,
            &self.metrics.process_metrics,
            "telemetry.metrics.processMetrics",
        )?;
        ensure!(
            !process_metrics || (profile != TelemetryProfile::Off && backend != MetricsBackend::None),
            "telemetry.metrics.processMetrics requires a non-off profile and an enabled metrics backend"
        );
        let service_name: String = resolve(
            mapper,
            &self.tracing.service_name,
            "telemetry.tracing.serviceName",
        )?;
        ensure!(
            !service_name.trim().is_empty() && !service_name.chars().any(char::is_control),
            "telemetry.tracing.serviceName must be nonempty and contain no control characters"
        );
        let ratio = resolve(
            mapper,
            &self.tracing.sample_ratio,
            "telemetry.tracing.sampleRatio",
        )?;
        let sampling = SamplingRatio::new(ratio)
            .map_err(|error| anyhow::anyhow!("telemetry.tracing.sampleRatio: {error}"))?;
        let shutdown = bounded_seconds(
            mapper,
            &self.shutdown_timeout_seconds,
            "telemetry.shutdownTimeoutSeconds",
            300,
        )?;
        let tracing = destination(
            mapper,
            self.tracing.endpoint.as_ref(),
            self.tracing.protocol.as_ref(),
            &self.tracing.headers,
            "telemetry.tracing",
        )?;
        let prometheus = self.metrics.prometheus.clone().unwrap_or_default();
        let path: String = resolve(
            mapper,
            &prometheus.path,
            "telemetry.metrics.prometheus.path",
        )?;
        validate_path(&path)?;
        let port = prometheus
            .port
            .as_ref()
            .map(|value| resolve(mapper, value, "telemetry.metrics.prometheus.port"))
            .transpose()?;
        ensure!(
            port != Some(0),
            "telemetry.metrics.prometheus.port must be between 1 and 65535"
        );
        let active = profile != TelemetryProfile::Off;
        if active && backend == MetricsBackend::Prometheus {
            if let Some(port) = port {
                ensure!(port != management_port, "telemetry.metrics.prometheus.port conflicts with the management listener; omit port to share it");
            } else {
                ensure!(
                    path != "/health"
                        && path != "/api"
                        && !path.starts_with("/api/")
                        && !(enable_ui && (path == "/ui" || path.starts_with("/ui/"))),
                    "telemetry.metrics.prometheus.path conflicts with a reserved management route"
                );
            }
        }
        let (metrics_otlp, export_interval) = if let Some(otlp) = &self.metrics.otlp {
            (
                destination(
                    mapper,
                    Some(&otlp.endpoint),
                    Some(&otlp.protocol),
                    &otlp.headers,
                    "telemetry.metrics.otlp",
                )?,
                bounded_seconds(
                    mapper,
                    &otlp.export_interval,
                    "telemetry.metrics.otlp.exportInterval",
                    3600,
                )?,
            )
        } else {
            (None, Duration::from_secs(30))
        };
        ensure!(
            !active || backend != MetricsBackend::Otlp || metrics_otlp.is_some(),
            "telemetry.metrics.backend otlp requires a nonempty telemetry.metrics.otlp.endpoint and explicit protocol"
        );
        let mut settings = TelemetrySettings::new(profile)
            .with_tracing(tracing.is_some())
            .with_metrics(backend != MetricsBackend::None)
            .with_sampling(RootSampling::Ratio(sampling));
        for exclusion in &self.filters.exclude {
            settings = settings.exclude_metrics(resolve::<String>(
                mapper,
                exclusion,
                "telemetry.filters.exclude",
            )?);
        }
        let policy = settings.resolve()?;
        Ok(EffectiveTelemetry {
            settings,
            policy,
            inspection_enabled,
            service_name,
            tracing: tracing.filter(|_| active),
            metrics_backend: if active {
                backend
            } else {
                MetricsBackend::None
            },
            prometheus: (active && backend == MetricsBackend::Prometheus)
                .then_some(EffectivePrometheus { path, port }),
            metrics_otlp: metrics_otlp.filter(|_| active && backend == MetricsBackend::Otlp),
            export_interval,
            process_metrics,
            shutdown_timeout: shutdown,
        })
    }
}

fn resolve<T>(mapper: &DtoMapper, value: &ConfigValue<T>, field: &str) -> Result<T>
where
    T: FromStr + Clone + Serialize + DeserializeOwned,
    T::Err: fmt::Display,
{
    ensure!(
        !matches!(value, ConfigValue::Secret { .. }),
        "{field}: plugin-backed Secret references are unavailable during early telemetry initialization; use an environment reference"
    );
    // Parsing errors may include the resolved value; never expose it in diagnostics.
    mapper.resolve_typed(value).map_err(|_| anyhow::anyhow!(
        "{field}: cannot resolve or parse configuration value; check its type and environment reference/default"
    ))
}

fn bounded_seconds(
    mapper: &DtoMapper,
    value: &ConfigValue<u64>,
    field: &str,
    max: u64,
) -> Result<Duration> {
    let seconds = resolve(mapper, value, field)?;
    ensure!(
        (1..=max).contains(&seconds),
        "{field} must be between 1 and {max} seconds"
    );
    Ok(Duration::from_secs(seconds))
}

fn destination(
    mapper: &DtoMapper,
    endpoint: Option<&ConfigValue<String>>,
    protocol: Option<&ConfigValue<String>>,
    headers: &BTreeMap<String, ConfigValue<String>>,
    field: &str,
) -> Result<Option<OtlpDestination>> {
    if let Some(protocol) = protocol {
        let protocol: String = resolve(mapper, protocol, &format!("{field}.protocol"))?;
        ensure!(protocol == "http/protobuf", "{field}.protocol supports only http/protobuf; gRPC and HTTP/JSON are not supported for new telemetry");
    }
    let endpoint: Option<String> = endpoint
        .map(|value| resolve(mapper, value, &format!("{field}.endpoint")))
        .transpose()?;
    let Some(endpoint) = endpoint.filter(|value| !value.is_empty()) else {
        ensure!(
            headers.is_empty(),
            "{field}.headers requires a nonempty endpoint"
        );
        return Ok(None);
    };
    ensure!(
        protocol.is_some(),
        "{field}.protocol must explicitly select http/protobuf"
    );
    let url = reqwest::Url::parse(&endpoint).map_err(|_| {
        anyhow::anyhow!("{field}.endpoint must be a full HTTP(S) signal-specific URL")
    })?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
            && url.username().is_empty() && url.password().is_none()
            && url.query().is_none() && url.fragment().is_none()
            && url.path() != "/" && !endpoint.chars().any(char::is_whitespace),
        "{field}.endpoint requires HTTP(S), a host and an explicit signal path, without embedded credentials, query, fragment or whitespace"
    );
    ensure!(
        headers.is_empty() || url.scheme() == "https",
        "{field}.headers requires HTTPS with certificate verification"
    );
    let mut resolved = axum::http::HeaderMap::new();
    for (name, value) in headers {
        ensure!(
            matches!(
                value,
                ConfigValue::EnvironmentVariable { default: None, .. }
            ),
            "{field}.headers values must be environment references without plaintext defaults"
        );
        let name = axum::http::HeaderName::from_str(name)
            .map_err(|_| anyhow::anyhow!("{field}.headers contains an invalid header name"))?;
        ensure!(
            !matches!(
                name.as_str(),
                "host"
                    | "content-type"
                    | "content-length"
                    | "content-encoding"
                    | "transfer-encoding"
                    | "connection"
                    | "te"
                    | "trailer"
                    | "upgrade"
                    | "proxy-authorization"
                    | "proxy-authenticate"
            ),
            "{field}.headers cannot override HTTP routing or payload headers"
        );
        let value: String = resolve(mapper, value, &format!("{field}.headers"))?;
        ensure!(
            !value.is_empty(),
            "{field}.headers contains an empty credential value"
        );
        let mut value = axum::http::HeaderValue::from_str(&value)
            .map_err(|_| anyhow::anyhow!("{field}.headers contains an invalid header value"))?;
        value.set_sensitive(true);
        ensure!(
            resolved.insert(name, value).is_none(),
            "{field}.headers contains duplicate case-insensitive names"
        );
    }
    Ok(Some(OtlpDestination {
        endpoint: url,
        headers: resolved,
    }))
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(
        path.starts_with('/') && path != "/" && !path.ends_with('/')
            && !path.contains("//")
            && path.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/-_.".contains(&byte))
            && !path.split('/').any(|part| matches!(part, "." | "..")),
        "telemetry.metrics.prometheus.path must be an absolute literal route without parameters, wildcards, escaping, query, fragment or dot segments"
    );
    Ok(())
}

fn value_schema(kind: utoipa::openapi::SchemaType) -> utoipa::openapi::schema::Schema {
    use utoipa::openapi::{
        schema::{AnyOfBuilder, ObjectBuilder},
        SchemaType,
    };
    let reference = ObjectBuilder::new()
        .property(
            "kind",
            ObjectBuilder::new()
                .schema_type(SchemaType::String)
                .enum_values(Some(["EnvironmentVariable", "Secret"])),
        )
        .property("name", ObjectBuilder::new().schema_type(SchemaType::String))
        .property(
            "default",
            ObjectBuilder::new().schema_type(SchemaType::String),
        )
        .required("kind")
        .required("name");
    let mut schema = AnyOfBuilder::new()
        .item(ObjectBuilder::new().schema_type(kind.clone()))
        .item(reference);
    if kind != SchemaType::String {
        schema = schema.item(
            ObjectBuilder::new()
                .schema_type(SchemaType::String)
                .pattern(Some(r"^\$\{[^}]+\}$")),
        );
    }
    schema.build().into()
}

fn string_schema() -> utoipa::openapi::schema::Schema {
    value_schema(utoipa::openapi::SchemaType::String)
}
fn integer_schema() -> utoipa::openapi::schema::Schema {
    value_schema(utoipa::openapi::SchemaType::Integer)
}
fn number_schema() -> utoipa::openapi::schema::Schema {
    value_schema(utoipa::openapi::SchemaType::Number)
}
fn bool_schema() -> utoipa::openapi::schema::Schema {
    value_schema(utoipa::openapi::SchemaType::Boolean)
}
fn exclusions_schema() -> utoipa::openapi::schema::Schema {
    utoipa::openapi::schema::ArrayBuilder::new()
        .items(string_schema())
        .build()
        .into()
}
fn headers_schema() -> utoipa::openapi::schema::Schema {
    use utoipa::openapi::{
        schema::{AnyOfBuilder, ObjectBuilder},
        SchemaType,
    };
    let environment = ObjectBuilder::new()
        .property(
            "kind",
            ObjectBuilder::new()
                .schema_type(SchemaType::String)
                .enum_values(Some(["EnvironmentVariable"])),
        )
        .property("name", ObjectBuilder::new().schema_type(SchemaType::String))
        .required("kind")
        .required("name")
        .additional_properties(Some(
            utoipa::openapi::schema::AdditionalProperties::FreeForm(false),
        ));
    let value = AnyOfBuilder::new()
        .item(
            ObjectBuilder::new()
                .schema_type(SchemaType::String)
                .pattern(Some(r"^\$\{[^}:]+\}$")),
        )
        .item(environment);
    ObjectBuilder::new()
        .additional_properties(Some(utoipa::openapi::schema::Schema::from(value.build())))
        .build()
        .into()
}

fn default_profile() -> ConfigValue<String> {
    ConfigValue::Static("basic".into())
}
fn default_backend() -> ConfigValue<String> {
    ConfigValue::Static("none".into())
}
fn default_false() -> ConfigValue<bool> {
    ConfigValue::Static(false)
}
fn default_service_name() -> ConfigValue<String> {
    ConfigValue::Static("drasi-server".into())
}
fn default_sample_ratio() -> ConfigValue<f64> {
    ConfigValue::Static(1.0)
}
fn default_metrics_path() -> ConfigValue<String> {
    ConfigValue::Static("/metrics".into())
}
fn default_export_interval() -> ConfigValue<u64> {
    ConfigValue::Static(30)
}
fn default_shutdown_timeout() -> ConfigValue<u64> {
    ConfigValue::Static(5)
}

#[cfg(test)]
mod tests;
