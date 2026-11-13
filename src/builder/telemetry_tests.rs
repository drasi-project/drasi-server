// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::DrasiServerBuilder;
use crate::{
    api::models::ConfigValue,
    config::telemetry::{MetricsBackend, TelemetryConfig},
    telemetry::MetricsConfig,
    TelemetryBindings,
};
use anyhow::{Context, Result};
use drasi_lib::observability::{
    default_log_level, init_default_subscriber, ComponentLogRuntime, LogPolicy, RootSampling,
    SamplingRatio, TelemetryProfile, TelemetrySettings,
};
use drasi_source_application::{ApplicationSource, ApplicationSourceConfig};
use std::{
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::time::{timeout, Instant};

const WAIT: Duration = Duration::from_secs(5);
const CASE: &str = "DRASI_TASK4_BUILDER_CASE";

fn config(yaml: &str) -> Result<TelemetryConfig> {
    Ok(serde_yaml::from_str(yaml)?)
}

fn source_builder(calls: Arc<AtomicUsize>) -> DrasiServerBuilder {
    DrasiServerBuilder::new()
        .with_id("builder-policy")
        .with_source_factory("input", true, move |id| {
            calls.fetch_add(1, Ordering::SeqCst);
            let (source, _) = ApplicationSource::new(
                id.as_str(),
                ApplicationSourceConfig {
                    properties: Default::default(),
                    durability: None,
                },
            )?;
            Ok(source)
        })
}

fn borrowed() -> Result<(TelemetryBindings, ComponentLogRuntime)> {
    let logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
    Ok((
        TelemetryBindings::new(
            TelemetrySettings::new(TelemetryProfile::Off),
            logging.clone(),
        ),
        logging,
    ))
}

fn child(case: &str) -> Result<()> {
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "builder::telemetry_tests::builder_telemetry_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CASE, case)
        .env("RUST_LOG", "info")
        .output()?;
    assert!(
        output.status.success(),
        "{case}:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    Ok(())
}

#[test]
fn builder_complete_configuration_replaces_previous_selection() -> Result<()> {
    child("replace")
}

#[test]
fn builder_profile_convenience_preserves_other_configuration() -> Result<()> {
    child("profile")
}

#[test]
fn builder_backend_convenience_preserves_other_configuration() -> Result<()> {
    child("backend")
}

#[test]
fn builder_repeated_backend_selection_uses_only_the_final_backend() -> Result<()> {
    child("repeat-backend")
}

#[test]
fn builder_final_listener_options_use_yaml_validation() -> Result<()> {
    child("listeners")
}

#[test]
fn builder_configured_and_borrowed_inputs_are_exclusive_in_both_orders() -> Result<()> {
    child("exclusive")
}

#[test]
fn builder_legacy_core_results_reject_new_owned_telemetry() -> Result<()> {
    child("legacy-results")
}

#[test]
fn legacy_progress_cannot_be_added_to_new_telemetry() -> Result<()> {
    child("legacy-progress-borrowed")?;
    child("legacy-progress-configured")
}

#[test]
fn builder_unselected_logging_borrows_the_known_core_default() -> Result<()> {
    child("known-default")
}

#[test]
fn builder_unselected_logging_never_borrows_unknown_globals() -> Result<()> {
    child("unknown-default")
}

#[test]
fn builder_explicit_bindings_do_not_install_globals() -> Result<()> {
    child("embedded-globals")
}

#[test]
#[ignore = "isolated global-installation subprocess entry point"]
fn builder_telemetry_child() -> Result<()> {
    let case = std::env::var(CASE)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(builder_case(&case))
}

async fn rejected_before_factories(
    builder: DrasiServerBuilder,
    calls: &AtomicUsize,
    cause: &str,
) -> Result<()> {
    let preparation = builder.prepare();
    let cleanup = preparation.cleanup_handle();
    let error = match preparation.finish().await {
        Err(error) => error,
        Ok(_) => anyhow::bail!("invalid telemetry was accepted"),
    };
    assert!(
        format!("{error:#}").contains(cause),
        "lost validation cause {cause:?}: {error:#}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        cleanup.status().phase,
        crate::server::CleanupPhase::Complete
    );
    Ok(())
}

// These cases catch setters that reset unrelated fields, accumulate backend
// selections, or construct components before final validation. Fresh child
// processes keep real standalone global installation isolated from other tests.
async fn builder_case(case: &str) -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    match case {
        "replace" | "profile" | "backend" | "repeat-backend" => {
            let retained = config(
                "profile: basic\nfilters:\n  exclude: [keep-this-filter]\ntracing:\n  sampleRatio: 0.25",
            )?;
            let builder = match case {
                "replace" => source_builder(calls.clone())
                    .with_telemetry(config("shutdownTimeoutSeconds: 0")?)
                    .with_telemetry_profile(TelemetryProfile::Persistence)
                    .with_metrics_backend(MetricsBackend::Prometheus)
                    .with_telemetry(retained),
                "profile" => source_builder(calls.clone())
                    .with_telemetry(retained)
                    .with_telemetry_profile(TelemetryProfile::Debug),
                "backend" => source_builder(calls.clone())
                    .with_telemetry(config(
                        "profile: basic\nfilters:\n  exclude: [keep-this-filter]\ntracing:\n  sampleRatio: 0.25\nmetrics:\n  backend: otlp",
                    )?)
                    .with_metrics_backend(MetricsBackend::None),
                "repeat-backend" => source_builder(calls.clone())
                    .with_telemetry(retained)
                    .with_metrics_backend(MetricsBackend::Prometheus)
                    .with_metrics_backend(MetricsBackend::Otlp)
                    .with_metrics_backend(MetricsBackend::None),
                _ => unreachable!(),
            };
            let preparation = builder.prepare_with_owned_handles();
            let cleanup = preparation.cleanup_handle();
            let built: crate::DrasiServerWithOwnedHandles = preparation.finish().await?;
            let core = &built.server;
            timeout(WAIT, core.component_by_name("input")?.wait_started()).await??;
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            let policy = core.telemetry().policy();
            assert_eq!(
                policy.profile(),
                if case == "profile" {
                    TelemetryProfile::Debug
                } else {
                    TelemetryProfile::Basic
                }
            );
            assert!(!policy.metrics_enabled());
            assert!(!policy.tracing_enabled());
            assert_eq!(
                policy.sampling(),
                RootSampling::Ratio(SamplingRatio::new(0.25)?)
            );
            assert!(policy
                .metric_exclusions()
                .iter()
                .any(|value| value == "keep-this-filter"));
            let logging = core.component_logging().context("missing owned logging")?;
            assert!(!logging.is_closed());
            built.shutdown().await?;
            assert_eq!(cleanup.status().phase, crate::server::CleanupPhase::Complete);
            assert_eq!(
                built.cleanup_handle().status().phase,
                crate::server::CleanupPhase::Complete
            );
            assert!(logging.is_closed());
        }
        "listeners" => {
            for (yaml, port, ui, cause) in [
                (
                    "profile: basic\nmetrics:\n  backend: prometheus\n  prometheus:\n    port: 9090",
                    9090,
                    false,
                    "conflicts with the management listener",
                ),
                (
                    "profile: basic\nmetrics:\n  backend: prometheus\n  prometheus:\n    port: 9090",
                    8080,
                    false,
                    "cannot activate: Prometheus",
                ),
                (
                    "profile: basic\nmetrics:\n  backend: prometheus\n  prometheus:\n    path: /ui/metrics",
                    8080,
                    true,
                    "conflicts with a reserved management route",
                ),
                (
                    "profile: basic\nmetrics:\n  backend: prometheus\n  prometheus:\n    path: /ui/metrics",
                    8080,
                    false,
                    "cannot activate: Prometheus",
                ),
            ] {
                let builder = source_builder(calls.clone())
                    .disable_ui()
                    .with_port(7070)
                    .with_telemetry(config(yaml)?)
                    .with_port(port);
                let builder = if ui {
                    builder.enable_ui()
                } else {
                    builder.disable_ui()
                };
                rejected_before_factories(builder, &calls, cause).await?;
            }
        }
        "exclusive" => {
            let (bindings, logging) = borrowed()?;
            for setter in 0..3 {
                let configure = |builder: DrasiServerBuilder| match setter {
                    0 => builder.with_telemetry(TelemetryConfig::default()),
                    1 => builder.with_telemetry_profile(TelemetryProfile::Off),
                    _ => builder.with_metrics_backend(MetricsBackend::None),
                };
                for config_first in [true, false] {
                    let builder = source_builder(calls.clone());
                    let builder = if config_first {
                        configure(builder).with_telemetry_bindings(bindings.clone())
                    } else {
                        configure(builder.with_telemetry_bindings(bindings.clone()))
                    };
                    let preparation = builder.prepare();
                    let cleanup = preparation.cleanup_handle();
                    assert!(preparation.finish().await.is_err());
                    assert_eq!(calls.load(Ordering::SeqCst), 0);
                    assert_eq!(cleanup.status().phase, crate::server::CleanupPhase::Complete);
                    assert!(!logging.is_closed());
                }
            }
            logging.shutdown(WAIT).await?;
        }
        "legacy-results" => {
            for yaml in ["profile: off", "profile: basic"] {
                assert!(source_builder(calls.clone())
                    .with_telemetry(config(yaml)?)
                    .build_core()
                    .await
                    .is_err());
                assert!(source_builder(calls.clone())
                    .with_telemetry(config(yaml)?)
                    .build_with_handles()
                    .await
                    .is_err());
            }
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert_eq!(default_log_level()?, None);
        }
        "legacy-progress-borrowed" | "legacy-progress-configured" => {
            let (bindings, logging) = borrowed()?;
            let builder = DrasiServerBuilder::new().with_id("legacy-progress-conflict");
            let builder = if case == "legacy-progress-borrowed" {
                builder.with_telemetry_bindings(bindings)
            } else {
                builder.with_telemetry(TelemetryConfig::default())
            };
            let server = builder.build().await?;
            let cleanup = server.cleanup_handle();
            let legacy_config = MetricsConfig {
                endpoint: ConfigValue::Static("http://127.0.0.1:4318/v1/metrics".into()),
                export_interval_ms: 100,
                export_timeout_ms: 100,
            };
            assert!(server.with_metrics(legacy_config).is_err());
            cleanup.wait_until(Instant::now() + WAIT).await?;
            assert!(!logging.is_closed());
            logging.shutdown(WAIT).await?;
        }
        "known-default" => {
            let logging = init_default_subscriber()?;
            let observed = calls.clone();
            let built = DrasiServerBuilder::new()
                .with_source_factory("input", true, move |id| {
                    assert_eq!(default_log_level()?, Some(log::LevelFilter::Info));
                    observed.fetch_add(1, Ordering::SeqCst);
                    let (source, _) = ApplicationSource::new(
                        id.as_str(),
                        ApplicationSourceConfig {
                            properties: Default::default(),
                            durability: None,
                        },
                    )?;
                    Ok(source)
                })
                .build_with_owned_handles()
                .await?;
            timeout(WAIT, built.server.component_by_name("input")?.wait_started()).await??;
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(Arc::ptr_eq(
                &built.server.log_registry(),
                &logging.registry()
            ));
            built.shutdown().await?;
            assert!(!logging.is_closed());
            let server = DrasiServerBuilder::new()
                .with_id("legacy-progress-still-supported")
                .build()
                .await?
                .with_metrics(MetricsConfig {
                    endpoint: ConfigValue::Static("http://127.0.0.1:4318/v1/metrics".into()),
                    export_interval_ms: 100,
                    export_timeout_ms: 100,
                })?;
            server
                .cleanup_handle()
                .wait_until(Instant::now() + WAIT)
                .await?;
            assert!(!logging.is_closed());
        }
        "unknown-default" | "embedded-globals" => {
            tracing_log::LogTracer::init()?;
            tracing::subscriber::set_global_default(tracing_subscriber::registry())?;
            if case == "unknown-default" {
                assert!(source_builder(calls.clone()).build().await.is_err());
                assert_eq!(calls.load(Ordering::SeqCst), 0);
            } else {
                let (bindings, logging) = borrowed()?;
                let built = source_builder(calls.clone())
                    .with_telemetry_bindings(bindings)
                    .build_with_owned_handles()
                    .await?;
                timeout(WAIT, built.server.component_by_name("input")?.wait_started()).await??;
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert!(Arc::ptr_eq(
                    &built.server.log_registry(),
                    &logging.registry()
                ));
                built.shutdown().await?;
                assert!(!logging.is_closed());
                logging.shutdown(WAIT).await?;
            }
        }
        _ => anyhow::bail!("unknown builder scenario"),
    }
    Ok(())
}

#[tokio::test]
async fn builder_conveniences_preserve_invalid_unrelated_fields() -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    rejected_before_factories(
        source_builder(calls.clone())
            .with_telemetry(config("shutdownTimeoutSeconds: 0")?)
            .with_metrics_backend(MetricsBackend::None),
        &calls,
        "telemetry.shutdownTimeoutSeconds",
    )
    .await?;
    rejected_before_factories(
        source_builder(calls.clone())
            .with_telemetry(config("tracing:\n  sampleRatio: 2")?)
            .with_telemetry_profile(TelemetryProfile::Off),
        &calls,
        "telemetry.tracing.sampleRatio",
    )
    .await?;
    rejected_before_factories(
        source_builder(calls.clone())
            .with_telemetry(config("profile: off\nmetrics:\n  backend: otlp")?)
            .with_telemetry_profile(TelemetryProfile::Basic),
        &calls,
        "telemetry.metrics.backend otlp requires",
    )
    .await?;
    rejected_before_factories(
        source_builder(calls.clone())
            .with_telemetry(config("profile: basic")?)
            .with_metrics_backend(MetricsBackend::None)
            .with_metrics_backend(MetricsBackend::Prometheus),
        &calls,
        "cannot activate: Prometheus",
    )
    .await
}

#[tokio::test]
async fn builder_production_destinations_stay_gated_before_factories() -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    for yaml in [
        "profile: basic\ntracing:\n  endpoint: http://127.0.0.1:4318/v1/traces\n  protocol: http/protobuf",
        "profile: basic\nmetrics:\n  backend: prometheus",
        "profile: basic\nmetrics:\n  backend: otlp\n  otlp:\n    endpoint: http://127.0.0.1:4318/v1/metrics\n    protocol: http/protobuf",
        "profile: basic\ninspection:\n  enabled: true",
        "profile: basic\nmetrics:\n  backend: prometheus\n  processMetrics: true",
    ] {
        rejected_before_factories(
            source_builder(calls.clone()).with_telemetry(config(yaml)?),
            &calls,
            "cannot activate",
        )
        .await?;
    }
    Ok(())
}
