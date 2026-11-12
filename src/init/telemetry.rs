// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_server::{
    api::{mappings::DtoMapper, models::ConfigValue},
    config::telemetry::{OtlpMetricsConfig, PrometheusConfig, TelemetryConfig},
};
use inquire::{Confirm, Select, Text};

trait Prompts {
    fn confirm(&mut self, message: &'static str, default: bool) -> Result<bool>;
    fn select(&mut self, message: &'static str, options: &[&'static str]) -> Result<String>;
    fn text(&mut self, message: &'static str, default: &'static str) -> Result<String>;
}

struct Interactive;

impl Prompts for Interactive {
    fn confirm(&mut self, message: &'static str, default: bool) -> Result<bool> {
        Ok(Confirm::new(message).with_default(default).prompt()?)
    }

    fn select(&mut self, message: &'static str, options: &[&'static str]) -> Result<String> {
        Ok(Select::new(message, options.to_vec()).prompt()?.to_string())
    }

    fn text(&mut self, message: &'static str, default: &'static str) -> Result<String> {
        Ok(Text::new(message).with_default(default).prompt()?)
    }
}

pub fn prompt_telemetry(port: u16) -> Result<Option<TelemetryConfig>> {
    collect(&mut Interactive, port)
}

fn reference<T>(value: String) -> Result<ConfigValue<T>>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Clone + std::str::FromStr + 'static,
{
    match serde_json::from_value::<ConfigValue<String>>(serde_json::Value::String(value))? {
        ConfigValue::Static(value) => value
            .parse()
            .map(ConfigValue::Static)
            .map_err(|_| anyhow::anyhow!("Invalid telemetry value; check its type")),
        ConfigValue::EnvironmentVariable { name, default } => {
            Ok(ConfigValue::EnvironmentVariable { name, default })
        }
        ConfigValue::Secret { name } => Ok(ConfigValue::Secret { name }),
    }
}

fn collect(prompts: &mut impl Prompts, management_port: u16) -> Result<Option<TelemetryConfig>> {
    if !prompts.confirm("Configure telemetry (runtime support is staged)?", false)? {
        return Ok(None);
    }
    let profile = prompts.select(
        "Telemetry profile:",
        &["basic", "off", "debug", "persistence"],
    )?;
    let mut config = TelemetryConfig {
        profile: ConfigValue::Static(profile.clone()),
        ..Default::default()
    };
    config.inspection.enabled = ConfigValue::Static(prompts.confirm(
        "Enable inspection API (requires trusted proxy/restricted network)?",
        false,
    )?);
    if profile != "off" {
        let endpoint = prompts.text(
            "OTLP HTTP/protobuf tracing URL (${VAR} allowed; blank to skip):",
            "",
        )?;
        if !endpoint.is_empty() {
            config.tracing.endpoint = Some(reference(endpoint)?);
            config.tracing.protocol = Some(ConfigValue::Static("http/protobuf".into()));
        }
        config.tracing.service_name =
            reference(prompts.text("OTLP service name:", "drasi-server")?)?;
        let backend = prompts.select("Metrics backend:", &["none", "prometheus", "otlp"])?;
        config.metrics.backend = ConfigValue::Static(backend.clone());
        match backend.as_str() {
            "none" => {}
            "prometheus" => {
                let shared =
                    prompts.confirm("Serve metrics on the management API listener?", true)?;
                let port = if shared {
                    None
                } else {
                    Some(reference(prompts.text(
                        "Separate metrics port (inherits server bind address; protect it separately):", "9090",
                    )?)?)
                };
                config.metrics.prometheus = Some(PrometheusConfig {
                    path: reference(prompts.text("Prometheus path:", "/metrics")?)?,
                    port,
                });
            }
            "otlp" => {
                config.metrics.otlp = Some(OtlpMetricsConfig {
                    endpoint: reference(prompts.text(
                        "OTLP HTTP/protobuf metrics URL:",
                        "http://localhost:4318/v1/metrics",
                    )?)?,
                    protocol: ConfigValue::Static("http/protobuf".into()),
                    export_interval: reference(
                        prompts.text("Metrics export interval in seconds (1-3600):", "30")?,
                    )?,
                    headers: Default::default(),
                });
            }
            _ => anyhow::bail!("Invalid telemetry metrics backend selection"),
        }
        if backend != "none" {
            config.metrics.process_metrics =
                ConfigValue::Static(prompts.confirm("Collect process memory/CPU metrics?", false)?);
        }
    }
    config
        .resolve(&DtoMapper::new(), management_port, true)
        .context("Invalid telemetry choices")?;
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    enum Answer {
        Confirm(bool),
        Select(&'static str),
        Text(&'static str),
    }

    struct Script(VecDeque<Answer>);

    impl Prompts for Script {
        fn confirm(&mut self, _: &'static str, _: bool) -> Result<bool> {
            match self.0.pop_front().expect("unexpected confirm prompt") {
                Answer::Confirm(value) => Ok(value),
                _ => panic!("expected confirm"),
            }
        }
        fn select(&mut self, _: &'static str, _: &[&'static str]) -> Result<String> {
            match self.0.pop_front().expect("unexpected selection prompt") {
                Answer::Select(value) => Ok(value.into()),
                _ => panic!("expected selection"),
            }
        }
        fn text(&mut self, _: &'static str, _: &'static str) -> Result<String> {
            match self.0.pop_front().expect("unexpected text prompt") {
                Answer::Text(value) => Ok(value.into()),
                _ => panic!("expected text"),
            }
        }
    }

    fn run(answers: Vec<Answer>) -> Result<Option<TelemetryConfig>> {
        let mut script = Script(answers.into());
        let result = collect(&mut script, 8080);
        assert!(script.0.is_empty(), "unconsumed prompts");
        result
    }

    #[test]
    fn skip_omits_telemetry_and_off_only_prompts_for_inspection() {
        use Answer::*;
        let skipped = drasi_server::DrasiServerConfig {
            telemetry: run(vec![Confirm(false)]).unwrap(),
            ..Default::default()
        };
        assert!(!super::super::builder::generate_yaml(&skipped)
            .unwrap()
            .contains("telemetry:"));
        let config = run(vec![Confirm(true), Select("off"), Confirm(true)])
            .unwrap()
            .unwrap();
        let policy = config.resolve(&DtoMapper::new(), 8080, true).unwrap();
        assert!(policy.inspection_enabled);
        assert!(!policy.policy().enabled());
    }

    #[test]
    fn prometheus_and_otlp_branches_generate_startup_valid_configurations() {
        use Answer::*;
        for answers in [
            vec![
                Confirm(true),
                Select("basic"),
                Confirm(false),
                Text(""),
                Text("drasi-server"),
                Select("prometheus"),
                Confirm(true),
                Text("/metrics"),
                Confirm(true),
            ],
            vec![
                Confirm(true),
                Select("debug"),
                Confirm(true),
                Text("https://collector/v1/traces"),
                Text("demo"),
                Select("prometheus"),
                Confirm(false),
                Text("9090"),
                Text("/custom-metrics"),
                Confirm(false),
            ],
            vec![
                Confirm(true),
                Select("persistence"),
                Confirm(false),
                Text(""),
                Text("metrics-only"),
                Select("otlp"),
                Text("http://localhost:4318/v1/metrics"),
                Text("30"),
                Confirm(false),
            ],
        ] {
            let config = drasi_server::DrasiServerConfig {
                telemetry: run(answers).unwrap(),
                ..Default::default()
            };
            let yaml = super::super::builder::generate_yaml(&config).unwrap();
            let restored: drasi_server::DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();
            restored.validate().unwrap();
            assert!(restored
                .resolved_telemetry(&DtoMapper::new(), 8080, true)
                .unwrap()
                .unwrap()
                .ensure_runtime_available()
                .is_err());
        }
    }

    #[test]
    fn invalid_selections_intervals_and_listener_conflicts_fail() {
        use Answer::*;
        for answers in [
            vec![
                Confirm(true),
                Select("basic"),
                Confirm(false),
                Text(""),
                Text("demo"),
                Select("invalid"),
            ],
            vec![
                Confirm(true),
                Select("basic"),
                Confirm(false),
                Text(""),
                Text("demo"),
                Select("otlp"),
                Text("http://collector/v1/metrics"),
                Text("0"),
                Confirm(false),
            ],
            vec![
                Confirm(true),
                Select("basic"),
                Confirm(false),
                Text(""),
                Text("demo"),
                Select("prometheus"),
                Confirm(false),
                Text("8080"),
                Text("/metrics"),
                Confirm(false),
            ],
        ] {
            assert!(run(answers).is_err());
        }
    }
}
