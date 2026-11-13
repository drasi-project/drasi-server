// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_lib::{
    observability::{
        ComponentLogKey, ComponentLogRuntime, ComponentLogType, LogPolicy, LoggingError,
        OpenTelemetryContextAdapter, TelemetryProfile, TelemetrySettings, TELEMETRY_TARGET,
    },
    DrasiLib, ShutdownCompletion,
};
use drasi_server::{
    server::{CleanupPhase, ServerCleanupError, ServerPreparation},
    telemetry::StandaloneTelemetry,
    DrasiServer, TelemetryBindings,
};
use drasi_source_application::ApplicationSource;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use std::{
    io::Write,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::time::{timeout, Instant};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::{filter::LevelFilter, prelude::*};

// Exercise the real command entry point without exporting binary-only CLI APIs.
#[allow(dead_code)]
#[path = "../src/cli_styles.rs"]
mod cli_styles;
#[allow(dead_code)]
#[path = "../src/plugin/mod.rs"]
mod plugin;

const WAIT: Duration = Duration::from_secs(5);
const COMPONENT: &str = "00000000-0000-4000-8000-000000000303";
const SCENARIO: &str = "DRASI_TASK3_LOGGING_SCENARIO";
const FLAVOR: &str = "DRASI_TASK3_TOKIO_FLAVOR";

fn children(scenario: &str, rust_log: Option<&str>) -> Result<Vec<String>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut consoles = Vec::new();
        for flavor in ["current_thread", "multi_thread"] {
            // Test-only destination: ambient OTLP settings must not turn
            // logs-only preparation into an exporter or a network request.
            let unexpected_exports = wiremock::MockServer::start().await;
            let endpoint = unexpected_exports.uri();
            let mut command = tokio::process::Command::new(std::env::current_exe()?);
            command
                .args([
                    "--exact",
                    "observability_runtime_child",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .env(SCENARIO, scenario)
                .env(FLAVOR, flavor)
                .env_remove("RUST_LOG")
                .env("NO_COLOR", "1")
                .env("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint)
                .env(
                    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                    format!("{endpoint}/v1/traces"),
                )
                .env(
                    "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
                    format!("{endpoint}/v1/metrics"),
                )
                .env(
                    "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
                    format!("{endpoint}/v1/logs"),
                )
                .env("OTEL_TRACES_EXPORTER", "otlp")
                .env("OTEL_METRICS_EXPORTER", "otlp")
                .env("OTEL_LOGS_EXPORTER", "otlp")
                .env("OTEL_BSP_SCHEDULE_DELAY", "1")
                .env("OTEL_METRIC_EXPORT_INTERVAL", "1")
                .kill_on_drop(true);
            if let Some(directives) = rust_log {
                command.env("RUST_LOG", directives);
            }
            let output = timeout(Duration::from_secs(30), command.output()).await??;
            let console = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.status.success(), "{scenario}/{flavor}:\n{console}");
            assert!(
                unexpected_exports
                    .received_requests()
                    .await
                    .context("request recording was disabled")?
                    .is_empty(),
                "{scenario}/{flavor} unexpectedly exported telemetry"
            );
            consoles.push(console);
        }
        Ok(consoles)
    })
}

// Break caught: capture/console installation occurs after a fallible constructor,
// or collection Off is incorrectly used as a log filter.
#[test]
fn off_preserves_early_constructor_diagnostics() -> Result<()> {
    for console in children("early-constructor", None)? {
        assert_eq!(console.matches("early constructor diagnostic").count(), 1);
        assert!(!console.contains("suppressed OCI warning"));
        assert!(console.contains("visible OCI error"));
    }
    Ok(())
}

// Break caught: the fallback overrides RUST_LOG, or noisy OCI defaults are
// appended even when the caller supplied its own complete logging policy.
#[test]
fn rust_log_wins_over_the_fallback() -> Result<()> {
    for console in children("environment-override", Some("info"))? {
        assert!(console.contains("early constructor diagnostic"));
        assert!(console.contains("suppressed OCI warning"));
    }
    Ok(())
}

#[test]
fn invalid_rust_log_fails_with_reachable_cleanup() -> Result<()> {
    children("invalid-filter", Some("task3=not-a-level"))?;
    Ok(())
}

#[test]
fn unrelated_logger_is_rejected_not_adopted() -> Result<()> {
    children("logger-conflict", None)?;
    Ok(())
}

#[test]
fn unrelated_subscriber_is_rejected_even_after_bridge_installation() -> Result<()> {
    children("subscriber-conflict", None)?;
    Ok(())
}

// Break caught: plugin work runs before logging, or discovery diagnostics go to
// an unrelated/default registry. An empty local registry needs no real cdylib.
#[test]
fn plugin_discovery_logs_the_selected_registry() -> Result<()> {
    for console in children("plugin-discovery", Some("debug"))? {
        assert!(console.contains("task3-selected-registry"));
        assert!(console.contains("No plugins found matching"));
    }
    Ok(())
}

#[test]
fn plugin_fallback_preserves_the_environment_and_suppresses_oci_noise() -> Result<()> {
    for console in children("plugin-fallback", None)? {
        assert!(console.contains("plugin fallback warning"));
        assert!(!console.contains("plugin filtered info"));
        assert!(!console.contains("suppressed OCI warning"));
        assert!(console.contains("visible OCI error"));
    }
    Ok(())
}

// Break caught: a direct or nested process::exit bypasses the owned scope,
// preventing the command future from returning through its cleanup boundary.
#[test]
fn plugin_errors_return_instead_of_exiting_inside_the_owned_scope() -> Result<()> {
    for scenario in ["plugin-invalid-action", "plugin-remove-missing"] {
        for console in children(scenario, None)? {
            assert!(console.contains("plugin error returned through cleanup"));
        }
    }
    Ok(())
}

#[test]
fn embedded_debug_collection_is_independent_of_disabled_logging() -> Result<()> {
    children("embedded-debug", None)?;
    Ok(())
}

#[test]
fn configured_empty_server_owns_logging_before_any_core_exists() -> Result<()> {
    for console in children("task5-configured-off", None)? {
        assert!(console.contains("task5 empty configured startup diagnostic"));
    }
    Ok(())
}

#[test]
fn standalone_runtime_handoff_reuses_the_original_capture_and_owner() -> Result<()> {
    for console in children("task5-runtime-handoff", None)? {
        assert_eq!(console.matches("task5 capture survives ownership handoff").count(), 1);
    }
    Ok(())
}

#[test]
fn configured_policy_rejection_awaits_the_handed_off_logging_owner() -> Result<()> {
    children("task5-runtime-rejection", None)?;
    Ok(())
}

#[tokio::test]
async fn configured_file_and_borrowed_policy_are_explicitly_exclusive() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-exclusive-").tempdir_in(".")?;
    let path = directory.path().join("server.yaml");
    std::fs::write(&path, "persistConfig: false\ntelemetry:\n  profile: off\ninstances: []\n")?;
    let logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
    let bindings = TelemetryBindings::new(
        TelemetrySettings::new(TelemetryProfile::Off), logging.clone(),
    );
    let preparation = DrasiServer::prepare_new_with_telemetry(
        path, 0, directory.path().join("plugins"), false, false, bindings,
    );
    let cleanup = preparation.cleanup_handle();
    let error = match preparation.finish().await {
        Ok(_) => anyhow::bail!("YAML telemetry silently replaced borrowed policy"),
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("mutually exclusive"), "{error:#}");
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    assert!(!logging.is_closed(), "rejection must not close borrowed capture");
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: CLI uses a passive registry instead of initializing logging
// before scanning plugins, even when startup has no Core to initialize it.
#[tokio::test]
async fn cli_empty_server_logs_before_plugins_and_constructor_failure() -> Result<()> {
    let console = cli_startup(false, None).await?;
    assert_eq!(console.matches("Loading cdylib plugins from:").count(), 1, "{console}");
    assert!(console.contains("No cdylib plugins found"), "{console}");
    Ok(())
}

#[tokio::test]
async fn cli_missing_config_diagnostics_are_visible_before_file_creation() -> Result<()> {
    let console = cli_startup(true, None).await?;
    assert_eq!(console.matches("Creating default configuration.").count(), 1, "{console}");
    assert_eq!(console.matches("Default configuration created at:").count(), 1, "{console}");
    assert_eq!(console.matches("Loading cdylib plugins from:").count(), 1, "{console}");
    let creating = console.find("Creating default configuration.").context("creation diagnostic")?;
    let created = console.find("Default configuration created at:").context("created diagnostic")?;
    let loading = console.find("Loading cdylib plugins from:").context("plugin loading diagnostic")?;
    assert!(creating < created && created < loading, "{console}");
    Ok(())
}

#[tokio::test]
async fn cli_rust_log_overrides_the_file_level_without_reinstalling_logging() -> Result<()> {
    let console = cli_startup(false, Some("info")).await?;
    assert_eq!(console.matches("Loading cdylib plugins from:").count(), 1, "{console}");
    assert!(!console.contains("global default trace dispatcher has already been set"), "{console}");
    Ok(())
}

#[tokio::test]
async fn cli_validate_logs_plugin_scan_without_activating_unavailable_policy() -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-validate-").tempdir_in(".")?;
    let path = directory.path().join("server.yaml");
    let plugins = directory.path().join("plugins");
    std::fs::create_dir(&plugins)?;
    std::fs::write(&path, concat!(
        "persistConfig: false\nverifyPlugins: false\ninstances: []\n",
        "telemetry:\n  profile: basic\n  metrics:\n    backend: prometheus\n",
    ))?;
    let output = timeout(Duration::from_secs(40), tokio::process::Command::new(
        env!("CARGO_BIN_EXE_drasi-server"),
    )
        .arg("validate").arg("--config").arg(&path)
        .arg("--plugins-dir").arg(&plugins)
        .env("RUST_LOG", "info").env("NO_COLOR", "1")
        .kill_on_drop(true).output()).await??;
    let console = format!("{}{}", String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "{console}");
    assert!(console.contains("Configuration policy valid"), "{console}");
    assert_eq!(console.matches("Loading cdylib plugins from:").count(), 1, "{console}");
    Ok(())
}

async fn cli_startup(missing: bool, rust_log: Option<&str>) -> Result<String> {
    use tokio::io::AsyncReadExt;

    let directory = tempfile::Builder::new().prefix("task5-cli-").tempdir_in(".")?;
    let path = directory.path().join("server.yaml");
    let plugins = directory.path().join("plugins");
    std::fs::create_dir(&plugins)?;
    // An occupied API address makes the real CLI return through startup cleanup,
    // without signals, a long-lived server or Task 6 drain assumptions.
    // A loopback listener can coexist with a wildcard listener on macOS.
    // Reserve the exact address each branch supplies to the real CLI.
    let address = if missing { "0.0.0.0:0" } else { "127.0.0.1:0" };
    let occupied = tokio::net::TcpListener::bind(address).await?;
    if !missing {
        let level = if rust_log.is_some() { "error" } else { "debug" };
        std::fs::write(&path, format!(
            "host: 127.0.0.1\nlogLevel: {level}\npersistConfig: false\nverifyPlugins: false\ninstances: []\n"
        ))?;
    }
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_drasi-server"));
    command.arg("--config").arg(&path)
        .arg("--port").arg(occupied.local_addr()?.port().to_string())
        .arg("--plugins-dir").arg(&plugins).arg("--disable-ui")
        .env_remove("RUST_LOG").env("NO_COLOR", "1").kill_on_drop(true);
    if let Some(directives) = rust_log {
        command.env("RUST_LOG", directives);
    }
    let mut child = command.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().context("CLI stdout was not piped")?;
    let mut stderr = child.stderr.take().context("CLI stderr was not piped")?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let result = timeout(Duration::from_secs(40), async {
        let (status, _, _) = tokio::try_join!(
            child.wait(),
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err),
        )?;
        Ok::<_, std::io::Error>(status)
    }).await;
    let status = match result {
        Ok(status) => status?,
        Err(error) => {
            let pid = child.id();
            let killed = child.start_kill();
            let status = child.wait().await.context("reaping timed-out CLI")?;
            tokio::try_join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err))?;
            let settings = std::fs::read_to_string(&path).ok()
                .and_then(|yaml| serde_yaml::from_str::<serde_yaml::Value>(&yaml).ok())
                .map(|document| format!(
                    "host={:?}, port={:?}, verifyPlugins={:?}, persistConfig={:?}",
                    document["host"], document["port"],
                    document["verifyPlugins"], document["persistConfig"],
                ));
            let console = format!("{}{}", String::from_utf8_lossy(&out),
                String::from_utf8_lossy(&err));
            return Err(error).with_context(|| format!(
                "CLI startup stalled: missing={missing}, reserved={}, pid={pid:?}, \
                 kill={killed:?}, exit={status:?}, generated={settings:?}\n{console}",
                occupied.local_addr().expect("reserved listener address"),
            ));
        }
    };
    let console = format!("{}{}", String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err));
    assert!(!status.success(), "occupied management port unexpectedly started: {console}");
    let document: serde_yaml::Value = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
    assert!(document["instances"].as_sequence().is_none_or(Vec::is_empty));
    Ok(console)
}

#[test]
#[ignore = "subprocess entry point; run through the scenario tests"]
fn observability_runtime_child() -> Result<()> {
    let scenario = std::env::var(SCENARIO)?;
    let mut builder = match std::env::var(FLAVOR)?.as_str() {
        "current_thread" => tokio::runtime::Builder::new_current_thread(),
        "multi_thread" => {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.worker_threads(2);
            builder
        }
        flavor => anyhow::bail!("unknown runtime flavor: {flavor}"),
    };
    builder.enable_all().build()?.block_on(async {
        match scenario.as_str() {
            "early-constructor" => early_constructor("info").await,
            "environment-override" => early_constructor("error").await,
            "invalid-filter" => {
                let error = failed_preparation(StandaloneTelemetry::prepare("info".into())).await?;
                assert!(matches!(
                    logging_cause(&error),
                    Some(LoggingError::Configuration(_))
                ));
                Ok(())
            }
            "logger-conflict" => logger_conflict().await,
            "subscriber-conflict" => subscriber_conflict().await,
            "embedded-debug" => embedded_debug().await,
            _ if scenario.starts_with("task5-") => configured_startup_scenario(&scenario).await,
            _ if scenario.starts_with("plugin-") => plugin_scenario(&scenario).await,
            _ => anyhow::bail!("unknown logging scenario: {scenario}"),
        }
    })
}

async fn configured_startup_scenario(scenario: &str) -> Result<()> {
    let directory = tempfile::Builder::new().prefix("task5-runtime-").tempdir_in(".")?;
    let path = directory.path().join("server.yaml");
    let plugins = directory.path().join("plugins");
    std::fs::create_dir(&plugins)?;
    let policy = if scenario == "task5-runtime-rejection" {
        "telemetry:\n  profile: basic\n  metrics:\n    backend: prometheus\n"
    } else {
        "telemetry:\n  profile: off\n"
    };
    std::fs::write(&path, format!(
        "persistConfig: false\nverifyPlugins: false\ninstances: []\n{policy}"
    ))?;
    let environment = std::env::var_os("RUST_LOG");
    if scenario == "task5-configured-off" {
        let preparation = DrasiServer::prepare_new(path, 0, plugins, false, false);
        let cleanup = preparation.cleanup_handle();
        let server = preparation.finish().await?;
        log::info!("task5 empty configured startup diagnostic");
        cleanup.wait_until(Instant::now() + WAIT).await?;
        assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
        drop(server);
    } else {
        let standalone = StandaloneTelemetry::prepare("info".into()).finish().await?;
        let original_owner = standalone.cleanup_handle();
        // A real, empty borrowing Core exposes the capture for the handoff
        // assertions; it is retired before transferring its standalone owner.
        let probe = standalone.bindings().apply_to(
            DrasiLib::builder().with_id("task5-handoff-probe"),
        )?.build().await?;
        let logging = probe.component_logging().context("missing handoff capture")?.clone();
        assert_eq!(probe.shutdown_with_outcome().await.completion, ShutdownCompletion::Complete);
        drop(probe);
        let preparation = DrasiServer::prepare_new_with_runtime(
            path, 0, plugins, false, false, standalone,
        );
        let owner = preparation.cleanup_handle();
        if scenario == "task5-runtime-rejection" {
            assert!(preparation.finish().await.is_err(), "unavailable policy activated");
            assert_eq!(owner.status().phase, CleanupPhase::Complete);
            assert_eq!(original_owner.status().phase, CleanupPhase::Complete);
            assert!(logging.is_closed(), "early rejection bypassed owned capture cleanup");
        } else {
            let server = preparation.finish().await?;
            assert!(!logging.is_closed(), "handoff dropped the original standalone owner");
            assert_eq!(original_owner.status().phase, CleanupPhase::Prepared);
            log::info!("task5 capture survives ownership handoff");
            assert!(logging.try_log(drasi_lib::observability::LogMessage::with_instance(
                drasi_lib::observability::LogLevel::Info,
                "task5 same capture after handoff", "task5-handoff-probe",
                COMPONENT, ComponentLogType::Source,
            )));
            // Closing the original address must close Server's same owner,
            // rather than shutting down a second, detached logging scope.
            original_owner.wait_until(Instant::now() + WAIT).await?;
            assert_eq!(owner.status().phase, CleanupPhase::Complete);
            assert!(logging.is_closed());
            owner.wait_until(Instant::now() + WAIT).await?;
            let key = ComponentLogKey::new(
                "task5-handoff-probe", ComponentLogType::Source, COMPONENT,
            );
            assert!(logging.registry().get_history_by_key(&key).await.iter()
                .any(|entry| entry.message == "task5 same capture after handoff"));
            drop(server);
        }
    }
    assert_eq!(std::env::var_os("RUST_LOG"), environment);
    Ok(())
}

async fn early_constructor(fallback: &str) -> Result<()> {
    let environment = std::env::var_os("RUST_LOG");
    let preparation = StandaloneTelemetry::prepare(fallback.into());
    let cleanup = preparation.cleanup_handle();
    let standalone = preparation.finish().await?;
    let bindings = standalone.bindings();
    let (constructed, constructor_observed) = tokio::sync::oneshot::channel();
    let core = bindings
        .apply_to(DrasiLib::builder().with_id("early").with_source_factory(
            "failing",
            true,
            move |id| {
                let span = tracing::info_span!(
                    "task3.constructor",
                    instance_id = "early",
                    component_id = %id,
                    component_type = "source"
                );
                span.in_scope(|| {
                    log::info!("early constructor diagnostic");
                    constructed.send(id).expect("constructor observer present");
                    Err::<ApplicationSource, _>(anyhow::anyhow!("deliberate constructor failure"))
                })
            },
        ))?
        .build()
        .await?;
    // Construction starts only after the graph publishes its component registry.
    let assigned_id = timeout(WAIT, constructor_observed).await??;
    let source = core.component_by_name("failing")?;
    assert_eq!(source.id(), &assigned_id);
    assert!(timeout(WAIT, source.wait_created()).await?.is_err());
    let logging = core
        .component_logging()
        .context("missing owned capture")?
        .clone();
    let key = ComponentLogKey::new("early", ComponentLogType::Source, source.id().to_string());
    logging.flush().await?;
    assert!(logging
        .registry()
        .get_history_by_key(&key)
        .await
        .iter()
        .any(|entry| entry.message.contains("early constructor diagnostic")));
    assert!(!core.telemetry().policy().enabled());
    log::warn!(target: "oci_client", "suppressed OCI warning");
    log::error!(target: "oci_client", "visible OCI error");
    assert_eq!(
        core.shutdown_with_outcome().await.completion,
        ShutdownCompletion::Complete
    );
    assert!(
        !logging.is_closed(),
        "Core only borrowed the standalone capture"
    );
    cleanup.wait_until(Instant::now() + WAIT).await?;
    assert!(
        logging.is_closed(),
        "the standalone owner must join its capture"
    );
    assert_eq!(cleanup.status().phase, CleanupPhase::Complete);
    standalone
        .cleanup_handle()
        .wait_until(Instant::now() + WAIT)
        .await?;
    assert_eq!(std::env::var_os("RUST_LOG"), environment);
    Ok(())
}

fn logging_cause(error: &anyhow::Error) -> Option<&LoggingError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<LoggingError>())
}

async fn failed_preparation(
    preparation: ServerPreparation<StandaloneTelemetry>,
) -> Result<anyhow::Error> {
    let cleanup = preparation.cleanup_handle();
    let error = match timeout(WAIT, preparation.finish()).await? {
        Ok(_) => anyhow::bail!("initialization incorrectly adopted an unrelated global"),
        Err(error) => error,
    };
    let aggregate = error
        .downcast_ref::<ServerCleanupError>()
        .context("initialization failure lost its cleanup handle")?;
    assert_eq!(
        aggregate.cleanup_handle().status().phase,
        CleanupPhase::Complete
    );
    assert!(cleanup.wait_until(Instant::now() + WAIT).await.is_err());
    assert_eq!(cleanup.status().pending_workers, 0);
    assert!(!cleanup.status().retry_required);
    Ok(error)
}

struct HostLogger(AtomicUsize);
static HOST_LOGGER: HostLogger = HostLogger(AtomicUsize::new(0));

impl log::Log for HostLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if record.target() == "task3::host" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn flush(&self) {}
}

async fn logger_conflict() -> Result<()> {
    log::set_logger(&HOST_LOGGER)?;
    log::set_max_level(log::LevelFilter::Trace);
    for _ in 0..2 {
        let error = failed_preparation(StandaloneTelemetry::prepare("info".into())).await?;
        assert!(matches!(
            logging_cause(&error),
            Some(LoggingError::LoggerConflict)
        ));
    }
    log::info!(target: "task3::host", "host retains its logger");
    assert_eq!(HOST_LOGGER.0.load(Ordering::SeqCst), 1);
    Ok(())
}

#[derive(Clone, Default)]
struct Console(Arc<Mutex<Vec<u8>>>);

impl Write for Console {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("console lock")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn subscriber_conflict() -> Result<()> {
    let console = Console::default();
    let writer = console.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_writer(move || writer.clone())),
    )?;
    let error = failed_preparation(StandaloneTelemetry::prepare("info".into())).await?;
    assert!(matches!(
        logging_cause(&error),
        Some(LoggingError::SubscriberConflict)
    ));
    let retry = failed_preparation(StandaloneTelemetry::prepare("info".into())).await?;
    assert!(matches!(
        logging_cause(&retry),
        Some(LoggingError::SubscriberConflict | LoggingError::LoggerConflict)
    ));
    tracing::info!("host retains its subscriber");
    assert!(
        String::from_utf8_lossy(&console.0.lock().expect("console lock"))
            .contains("host retains its subscriber")
    );
    Ok(())
}

async fn plugin_scenario(scenario: &str) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("task3-selected-registry-")
        .tempdir_in(".")?;
    let config = directory.path().join("missing-config.yaml");
    let environment = std::env::var_os("RUST_LOG");
    let action = match scenario {
        "plugin-discovery" => plugin::PluginAction::Search {
            reference: "task3-no-match".into(),
            registry: Some(
                directory
                    .path()
                    .canonicalize()?
                    .to_string_lossy()
                    .into_owned(),
            ),
        },
        "plugin-fallback" => plugin::PluginAction::List,
        "plugin-invalid-action" => plugin::PluginAction::Install {
            reference: None,
            from_config: false,
            registry: None,
            platform: None,
            locked: false,
        },
        "plugin-remove-missing" => plugin::PluginAction::Remove {
            reference: "source/task3-missing".into(),
        },
        _ => anyhow::bail!("unknown plugin scenario"),
    };
    let outcome = plugin::run_plugin_command(action, config, Some(directory.path().into())).await;
    if matches!(scenario, "plugin-invalid-action" | "plugin-remove-missing") {
        assert!(outcome.is_err(), "invalid plugin work must remain an error");
        println!("plugin error returned through cleanup");
    } else {
        outcome?;
    }
    assert_eq!(std::env::var_os("RUST_LOG"), environment);
    if scenario == "plugin-fallback" {
        log::info!("plugin filtered info");
        log::warn!("plugin fallback warning");
        log::warn!(target: "oci_client", "suppressed OCI warning");
        log::error!(target: "oci_client", "visible OCI error");
    }
    directory.close()?;
    Ok(())
}

// Break caught: applying bindings loses Debug collection, or a log-only Off
// filter is attached to the whole subscriber and suppresses operation spans.
async fn embedded_debug() -> Result<()> {
    let logging = ComponentLogRuntime::new(LogPolicy::new("off")?)?;
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let bindings = TelemetryBindings::new(
        TelemetrySettings::new(TelemetryProfile::Debug).with_metrics(false),
        logging.clone(),
    )
    .with_context_adapter(Arc::new(OpenTelemetryContextAdapter));
    let core = bindings
        .apply_to(DrasiLib::builder().with_id("embedded-debug"))?
        .build()
        .await?;
    let console = Console::default();
    let writer = console.clone();
    let subscriber = tracing_subscriber::registry()
        .with(logging.layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(move || writer.clone())
                .with_filter(LevelFilter::OFF),
        )
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(provider.tracer("task3-embedded"))
                .with_filter(core.telemetry().operation_filter()),
        );
    async {
        let span = tracing::debug_span!(
            target: TELEMETRY_TARGET,
            "query.process",
            instance_id = "embedded-debug",
            component_id = COMPONENT,
            component_type = "query"
        );
        span.in_scope(|| tracing::info!("disabled console diagnostic"));
    }
    .with_subscriber(subscriber)
    .await;
    logging.flush().await?;
    provider.force_flush()?;
    let spans = exporter.get_finished_spans()?;
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "query.process");
    assert!(console.0.lock().expect("console lock").is_empty());
    let key = ComponentLogKey::new("embedded-debug", ComponentLogType::Query, COMPONENT);
    assert!(logging.registry().get_history_by_key(&key).await.is_empty());
    core.shutdown().await?;
    drop((core, bindings));
    assert!(
        !logging.is_closed(),
        "embedded logging belongs to its caller"
    );
    provider.force_flush()?;
    provider.shutdown()?;
    logging.shutdown(WAIT).await?;
    Ok(())
}
