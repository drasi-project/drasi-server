use super::*;
use crate::config::{loader::load_config_file_with_overrides, DrasiServerConfig};
use serde_json::json;

fn config(value: serde_json::Value) -> DrasiServerConfig {
    serde_json::from_value(json!({"telemetry": value})).unwrap()
}

fn effective(config: &DrasiServerConfig) -> Result<EffectiveTelemetry> {
    config
        .resolved_telemetry(&DtoMapper::new(), 8080, true)?
        .ok_or_else(|| anyhow::anyhow!("missing telemetry"))
}

#[test]
fn profile_backend_signal_matrix_preserves_independence() {
    for profile in ["off", "basic", "debug", "persistence"] {
        for backend in ["none", "prometheus", "otlp"] {
            for tracing in [false, true] {
                for inspection in [false, true] {
                    let config = config(json!({
                        "profile": profile,
                        "inspection": {"enabled": inspection},
                        "tracing": {
                            "endpoint": if tracing { "http://localhost:4318/v1/traces" } else { "" },
                            "protocol": "http/protobuf"
                        },
                        "metrics": {
                            "backend": backend,
                            "otlp": {
                                "endpoint": "http://localhost:4318/v1/metrics",
                                "protocol": "http/protobuf"
                            }
                        }
                    }));
                    config.validate().unwrap();
                    let policy = effective(&config).unwrap();
                    assert_eq!(
                        policy.policy().tracing_enabled(),
                        profile != "off" && tracing
                    );
                    assert_eq!(
                        policy.policy().metrics_enabled(),
                        profile != "off" && backend != "none"
                    );
                    assert_eq!(policy.inspection_enabled, inspection);
                    assert_eq!(policy.tracing.is_some(), profile != "off" && tracing);
                    assert_eq!(
                        policy.prometheus.is_some(),
                        profile != "off" && backend == "prometheus"
                    );
                    assert_eq!(
                        policy.metrics_otlp.is_some(),
                        profile != "off" && backend == "otlp"
                    );
                    assert_eq!(
                        policy.ensure_runtime_available().is_ok(),
                        !inspection && (profile == "off" || (!tracing && backend == "none"))
                    );
                }
            }
        }
    }
}

#[test]
fn omission_empty_section_and_legacy_migration_are_distinct() {
    let omitted = DrasiServerConfig::default();
    assert!(omitted
        .resolved_telemetry(&DtoMapper::new(), 8080, true)
        .unwrap()
        .is_none());
    assert!(!serde_yaml::to_string(&omitted)
        .unwrap()
        .contains("telemetry:"));
    let empty = effective(&config(json!({}))).unwrap();
    assert_eq!(empty.policy().profile(), TelemetryProfile::Basic);
    assert!(!empty.policy().enabled());
    assert_eq!(empty.export_interval, Duration::from_secs(30));
    assert_eq!(empty.shutdown_timeout, Duration::from_secs(5));
    assert_eq!(empty.service_name, "drasi-server");
    let legacy: crate::telemetry::MetricsConfig = serde_json::from_value(json!({
        "endpoint": "http://localhost:4318/v1/metrics"
    }))
    .unwrap();
    assert_eq!(legacy.export_interval_ms, 10_000);
    assert_eq!(legacy.export_timeout_ms, 5_000);
    let mut legacy_only = omitted;
    legacy_only.metrics = Some(legacy.clone());
    legacy_only.validate().unwrap();
    for telemetry in [
        json!({}),
        json!({"profile":"off"}),
        json!({"inspection":{"enabled":true}}),
        json!({"tracing":{"endpoint":"https://collector/v1/traces","protocol":"http/protobuf"}}),
    ] {
        let mut both = config(telemetry);
        both.metrics = Some(legacy.clone());
        assert!(both
            .validate()
            .unwrap_err()
            .to_string()
            .contains("cannot be configured together"));
    }
}

#[test]
fn invalid_policy_inputs_have_field_context() {
    for (value, field) in [
        (json!({"profile":"verbose"}), "telemetry.profile"),
        (
            json!({"metrics":{"backend":"pushgateway"}}),
            "telemetry.metrics.backend",
        ),
        (json!({"tracing":{"sampleRatio":-0.1}}), "sampling ratio"),
        (json!({"tracing":{"sampleRatio":1.1}}), "sampling ratio"),
        (
            json!({"tracing":{"sampleRatio":"${UNSET_OBS02_RATIO:-NaN}"}}),
            "sampling ratio",
        ),
        (
            json!({"tracing":{"sampleRatio":"${UNSET_OBS02_RATIO:-inf}"}}),
            "sampling ratio",
        ),
        (
            json!({"shutdownTimeoutSeconds":0}),
            "telemetry.shutdownTimeoutSeconds",
        ),
        (
            json!({"shutdownTimeoutSeconds":301}),
            "telemetry.shutdownTimeoutSeconds",
        ),
        (
            json!({"shutdownTimeoutSeconds":u64::MAX}),
            "telemetry.shutdownTimeoutSeconds",
        ),
        (
            json!({"shutdownTimeoutSeconds":"${UNSET_OBS02_TIMEOUT:-wrong}"}),
            "telemetry.shutdownTimeoutSeconds",
        ),
        (json!({"metrics":{"backend":"otlp"}}), "telemetry.metrics"),
        (
            json!({"metrics":{"processMetrics":true}}),
            "telemetry.metrics.processMetrics",
        ),
        (
            json!({"profile":"off","metrics":{"backend":"prometheus","processMetrics":true}}),
            "telemetry.metrics.processMetrics",
        ),
        (
            json!({"inspection":{"enabled":"${UNSET_OBS02_BOOL:-wrong}"}}),
            "telemetry.inspection.enabled",
        ),
        (
            json!({"tracing":{"serviceName":""}}),
            "telemetry.tracing.serviceName",
        ),
    ] {
        let error = config(value.clone()).validate().unwrap_err().to_string();
        assert!(error.contains(field), "{value}: {error}");
    }
    for interval in [0, 3601, u64::MAX] {
        let value = json!({"metrics":{"backend":"otlp","otlp":{
            "endpoint":"https://collector/v1/metrics","protocol":"http/protobuf","exportInterval":interval
        }}});
        assert!(config(value)
            .validate()
            .unwrap_err()
            .to_string()
            .contains("exportInterval"));
    }
    for (interval, shutdown, ratio) in [(1, 1, 0.0), (3600, 300, 1.0)] {
        let policy = effective(&config(json!({
            "shutdownTimeoutSeconds":shutdown,
            "tracing":{"sampleRatio":ratio},
            "metrics":{"backend":"otlp","otlp":{
                "endpoint":"https://collector/v1/metrics","protocol":"http/protobuf","exportInterval":interval
            }}
        }))).unwrap();
        assert!(policy.policy().metrics_enabled());
        assert_eq!(policy.export_interval.as_secs(), interval);
    }
}

#[test]
fn endpoints_protocols_and_credentials_are_validated_without_disclosure() {
    for endpoint in [
        "not-a-url",
        "ftp://collector/v1/traces",
        "http://collector",
        "https://user:password@collector/v1/traces",
        "https://collector/v1/traces?token=secret",
        "https://collector/v1/traces#secret",
        " https://collector/v1/traces",
    ] {
        let error = config(json!({"tracing":{"endpoint":endpoint,"protocol":"http/protobuf"}}))
            .validate()
            .unwrap_err()
            .to_string();
        assert!(error.contains("telemetry.tracing.endpoint"));
        assert!(!error.contains(endpoint));
    }
    for protocol in [None, Some("grpc"), Some("http/json")] {
        let mut tracing = json!({"endpoint":"https://collector/v1/traces"});
        if let Some(protocol) = protocol {
            tracing["protocol"] = json!(protocol);
        }
        assert!(config(json!({"tracing":tracing}))
            .validate()
            .unwrap_err()
            .to_string()
            .contains("protocol"));
    }
    for header in [
        json!("plaintext-secret"),
        json!("${HEADER:-plaintext-secret}"),
        json!({"kind":"Secret","name":"token"}),
    ] {
        let error = config(json!({"tracing":{
            "endpoint":"https://collector/v1/traces","protocol":"http/protobuf",
            "headers":{"authorization":header}
        }}))
        .validate()
        .unwrap_err()
        .to_string();
        assert!(error.contains("environment references"));
        assert!(!error.contains("plaintext-secret"));
    }
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("OBS02_AUTH".into(), "Bearer sensitive-value".into());
    let mapper = DtoMapper::with_overrides(overrides);
    let config = config(json!({"tracing":{
        "endpoint":"https://collector/custom/traces","protocol":"http/protobuf",
        "headers":{"authorization":"${OBS02_AUTH}"}
    }}));
    let policy = config
        .resolved_telemetry(&mapper, 8080, true)
        .unwrap()
        .unwrap();
    let destination = policy.tracing.as_ref().unwrap();
    assert_eq!(destination.endpoint().path(), "/custom/traces");
    assert_eq!(
        destination.headers()["authorization"],
        "Bearer sensitive-value"
    );
    assert!(destination.headers()["authorization"].is_sensitive());
    let debug = format!("{policy:?}");
    assert!(!debug.contains("sensitive-value") && !debug.contains("collector"));
    let mut insecure = config.clone();
    insecure.telemetry.as_mut().unwrap().tracing.endpoint =
        Some(ConfigValue::Static("http://collector/v1/traces".into()));
    assert!(insecure
        .resolved_telemetry(&mapper, 8080, true)
        .unwrap_err()
        .to_string()
        .contains("HTTPS"));
    for (name, value) in [
        ("bad header", "Bearer value"),
        ("host", "collector.example"),
        ("authorization", ""),
        ("authorization", "Bearer value\r\ninjected: value"),
    ] {
        let mut invalid = config.clone();
        invalid.telemetry.as_mut().unwrap().tracing.headers = BTreeMap::from([(
            name.into(),
            ConfigValue::EnvironmentVariable {
                name: "OBS02_INVALID_AUTH".into(),
                default: None,
            },
        )]);
        let mapper = DtoMapper::with_overrides(std::collections::HashMap::from([(
            "OBS02_INVALID_AUTH".into(),
            value.into(),
        )]));
        let error = invalid
            .resolved_telemetry(&mapper, 8080, true)
            .unwrap_err()
            .to_string();
        assert!(error.contains("headers"));
        assert!(!error.contains("Bearer value"));
    }
}

#[test]
fn interpolation_precedence_and_literal_exclusions_reuse_core_policy() {
    let config = config(json!({
        "profile":"${OBS02_PROFILE:-basic}",
        "tracing":{"endpoint":"${UNSET_OBS02_ENDPOINT:-}","sampleRatio":"${OBS02_RATIO:-1}"},
        "metrics":{"backend":"${OBS02_BACKEND:-none}"},
        "filters":{"exclude":["drasi.plugin.*","drasi.query."]}
    }));
    let mapper = DtoMapper::with_overrides(std::collections::HashMap::from([
        ("OBS02_PROFILE".into(), "persistence".into()),
        ("OBS02_RATIO".into(), "0".into()),
        ("OBS02_BACKEND".into(), "prometheus".into()),
    ]));
    let policy = config
        .resolved_telemetry(&mapper, 8080, true)
        .unwrap()
        .unwrap();
    assert_eq!(policy.policy().profile(), TelemetryProfile::Persistence);
    assert!(!policy.policy().allows_metric("drasi.query.custom"));
    assert!(policy.policy().allows_metric("drasi.plugin.custom"));
    assert!(!policy.policy().allows_metric("drasi.plugin.*custom"));
    assert!(policy.policy().metrics_enabled());
    assert!(policy.policy().handled_branch_limits().is_none());
    assert_eq!(config.log_level, ConfigValue::Static("info".into()));
    let mut too_many = config.clone();
    too_many.telemetry.as_mut().unwrap().filters.exclude =
        vec![ConfigValue::Static("x".into()); 65];
    assert!(effective(&too_many).is_err());
}

#[test]
fn route_and_listener_collisions_use_final_overrides() {
    for path in [
        "/",
        "metrics",
        "/metrics/",
        "/metrics?x=1",
        "/:id",
        "/{id}",
        "/*rest",
        "/m%65trics",
        "/a/../b",
        "/a//b",
        "/health",
        "/api",
        "/api/v1/telemetry",
        "/ui/assets",
    ] {
        assert!(
            effective(&config(
                json!({"metrics":{"backend":"prometheus","prometheus":{"path":path}}})
            ))
            .is_err(),
            "{path}"
        );
    }
    let config = config(json!({"metrics":{"backend":"prometheus","prometheus":{"port":9090}}}));
    assert!(config.validate().is_ok());
    // Existing programmatic startup permits port zero for an ephemeral listener.
    assert!(config.validate_with_overrides(Some(0), None).is_ok());
    assert!(config.validate_with_overrides(Some(9090), None).is_err());
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), serde_yaml::to_string(&config).unwrap()).unwrap();
    assert!(load_config_file_with_overrides(file.path(), Some(9090), None).is_err());
    let mut changed = config.clone();
    changed.port = ConfigValue::Static(9090);
    std::fs::write(file.path(), serde_yaml::to_string(&changed).unwrap()).unwrap();
    let loaded = load_config_file_with_overrides(file.path(), Some(8080), None).unwrap();
    assert_eq!(loaded.port, ConfigValue::Static(9090));
    let ui = serde_json::from_value::<DrasiServerConfig>(json!({"telemetry":{"metrics":{"backend":"prometheus","prometheus":{"path":"/ui/metrics"}}}})).unwrap();
    assert!(ui.validate_with_overrides(None, Some(false)).is_ok());
}

#[test]
fn openapi_telemetry_value_schemas_include_numbers_booleans_and_references() {
    use utoipa::OpenApi;
    let document = serde_json::to_value(crate::api::ApiDocV1::openapi()).unwrap();
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["TracingConfig"]["properties"]["sampleRatio"]["anyOf"][0]["type"],
        "number"
    );
    assert_eq!(
        schemas["InspectionConfig"]["properties"]["enabled"]["anyOf"][0]["type"],
        "boolean"
    );
    assert_eq!(
        schemas["OtlpMetricsConfig"]["properties"]["exportInterval"]["anyOf"][0]["type"],
        "integer"
    );
    assert_eq!(
        schemas["TelemetryConfig"]["properties"]["profile"]["anyOf"][1]["required"],
        json!(["kind", "name"])
    );
}

#[test]
fn yaml_json_round_trips_keep_references_and_reject_unknown_fields() {
    let input = r#"
telemetry:
  profile: "${OBS02_PROFILE:-basic}"
  tracing:
    endpoint: "${OBS02_ENDPOINT:-}"
    sampleRatio: "${OBS02_RATIO:-0.5}"
  metrics:
    backend: "${OBS02_BACKEND:-none}"
"#;
    let original: DrasiServerConfig = serde_yaml::from_str(input).unwrap();
    original.validate().unwrap();
    for saved in [
        serde_yaml::to_string(&original).unwrap(),
        serde_json::to_string(&original).unwrap(),
    ] {
        let restored: DrasiServerConfig = serde_yaml::from_str(&saved).unwrap();
        restored.validate().unwrap();
        assert_eq!(
            serde_json::to_value(restored.telemetry).unwrap(),
            serde_json::to_value(&original.telemetry).unwrap()
        );
        assert!(saved.contains("EnvironmentVariable"));
    }
    let mut secret = original.clone();
    secret.telemetry.as_mut().unwrap().tracing.endpoint = Some(ConfigValue::Secret {
        name: "trace-endpoint".into(),
    });
    let saved = serde_yaml::to_string(&secret).unwrap();
    let restored: DrasiServerConfig = serde_yaml::from_str(&saved).unwrap();
    assert!(matches!(
        restored.telemetry.as_ref().unwrap().tracing.endpoint,
        Some(ConfigValue::Secret { .. })
    ));
    assert!(restored
        .validate()
        .unwrap_err()
        .to_string()
        .contains("early telemetry"));
    for value in [
        json!({"telemetry":{"unknown":true}}),
        json!({"telemetry":{"tracing":{"sample_ratio":0.5}}}),
        json!({"telemetry":{"metrics":{"backend":"none","typo":1}}}),
        json!({"instances":[{"telemetry":{}}]}),
    ] {
        assert!(serde_json::from_value::<DrasiServerConfig>(value).is_err());
    }
}

#[tokio::test]
async fn startup_rejects_unavailable_capabilities_before_loading_plugins() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("server.yaml");
    std::fs::write(&path, "telemetry:\n  inspection:\n    enabled: true\n").unwrap();
    let error = match crate::DrasiServer::new(
        path,
        8080,
        directory.path().join("missing-plugins"),
        false,
        false,
    )
    .await
    {
        Ok(_) => panic!("unsupported inspection must not start"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("inspection API (OBS-06)"));
}
