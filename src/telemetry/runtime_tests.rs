// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::TelemetryBindings;
use anyhow::{Context, Result};
use drasi_lib::{
    observability::{
        ComponentLogRuntime, ComponentLogType, LogLevel, LogMessage, LogPolicy,
        MetricRegistrationContext, OpenTelemetryContextAdapter, RootSampling, SamplingRatio,
        TelemetryOptions, TelemetryProfile, TelemetrySettings,
    },
    DrasiLib, DrasiLibBuilder,
};
use drasi_source_application::{ApplicationSource, ApplicationSourceConfig};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::{
    api::{mappings::DtoMapper, models::ConfigValue},
    config::telemetry::TelemetryConfig,
};

const WAIT: Duration = Duration::from_secs(5);

fn source_builder(id: &str, calls: Arc<AtomicUsize>) -> DrasiLibBuilder {
    DrasiLib::builder()
        .with_id(id)
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

fn logging() -> Result<ComponentLogRuntime> {
    Ok(ComponentLogRuntime::new(LogPolicy::new("info")?)?)
}

fn metrics_only() -> TelemetrySettings {
    TelemetrySettings::new(TelemetryProfile::Basic).with_tracing(false)
}

// Break caught: the new common-binding helper replaces settings on a legacy
// registry that has no Server runtime context.
#[tokio::test]
async fn instance_registry_without_context_keeps_core_default_binding_behavior() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let bindings = TelemetryBindings::new(metrics_only(), logging.clone())
        .with_metric_context(context);
    let supplied = bindings.apply_to(DrasiLib::builder().with_id("legacy-registry"))?;
    let builder = crate::instance_registry::InstanceRegistry::new().bind_builder(supplied)?;
    let core = builder.build().await?;
    bindings.validate_prebuilt(&core).await?;
    core.shutdown().await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

#[tokio::test]
async fn bindings_preserve_actual_instance_scope() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let metrics = recorder.handle();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let bindings =
        TelemetryBindings::new(metrics_only(), logging.clone()).with_metric_context(context);
    bindings.validate()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let first = bindings.apply_to(source_builder("a", calls.clone()))?;
    let second = bindings.apply_to(source_builder("b", calls.clone()))?;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let first = first.build().await?;
    let second = second.build().await?;

    for (core, instance_id, other_instance) in [(&first, "a", "b"), (&second, "b", "a")] {
        assert_eq!(
            core.telemetry()
                .scope()
                .context("missing actual scope")?
                .instance_id(),
            instance_id
        );
        core.start().await?;
        let input = core.component_by_name("input")?;
        tokio::time::timeout(WAIT, input.wait_started()).await??;
        let rendered = metrics.render();
        let component = format!("component_id=\"{}\"", input.id());
        let instance = format!("instance_id=\"{instance_id}\"");
        let other_instance = format!("instance_id=\"{other_instance}\"");
        let observations = rendered
            .lines()
            .filter(|line| line.starts_with("drasi_component_") && line.contains(&component))
            .collect::<Vec<_>>();
        assert!(
            !observations.is_empty(),
            "missing observations for {component}"
        );
        assert!(observations.iter().all(|line| line.contains(&instance)));
        assert!(observations
            .iter()
            .all(|line| !line.contains(&other_instance)));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    first.shutdown().await?;
    second.shutdown().await?;
    logging.shutdown(WAIT).await?;
    Ok(())
}

#[tokio::test]
async fn enabled_collection_requires_explicit_handles() -> Result<()> {
    let open = logging()?;
    let closed = logging()?;
    closed.shutdown(WAIT).await?;
    let invalid_settings = (0..65).fold(metrics_only().with_metrics(false), |settings, _| {
        settings.exclude_metrics("excluded")
    });
    let cases = [
        (
            "metrics without context",
            TelemetryBindings::new(metrics_only(), open.clone()),
        ),
        (
            "distributed tracing without adapter",
            TelemetryBindings::new(
                TelemetrySettings::new(TelemetryProfile::Basic)
                    .with_metrics(false)
                    .require_distributed_tracing(),
                open.clone(),
            ),
        ),
        (
            "closed logging even when telemetry is off",
            TelemetryBindings::new(TelemetrySettings::new(TelemetryProfile::Off), closed),
        ),
        (
            "invalid Core collection settings",
            TelemetryBindings::new(invalid_settings, open.clone()),
        ),
    ];
    for (case, bindings) in cases {
        let constructor_calls = Arc::new(AtomicUsize::new(0));
        let builder = source_builder("invalid", constructor_calls.clone());
        assert!(bindings.validate().is_err(), "{case}");
        assert!(bindings.apply_to(builder).is_err(), "{case}");
        assert_eq!(constructor_calls.load(Ordering::SeqCst), 0, "{case}");
    }
    open.shutdown(WAIT).await?;
    Ok(())
}

#[tokio::test]
async fn off_does_not_close_borrowed_logging() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let bindings = TelemetryBindings::new(
        TelemetrySettings::new(TelemetryProfile::Off),
        logging.clone(),
    )
    .with_metric_context(context.clone());
    bindings.validate()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let core = bindings
        .apply_to(source_builder("off", calls.clone()))?
        .build()
        .await?;
    core.start().await?;
    let input = core.component_by_name("input")?;
    tokio::time::timeout(WAIT, input.wait_started()).await??;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(context.registrations().counts()?, (0, 0));
    assert!(Arc::ptr_eq(&core.log_registry(), &logging.registry()));
    let attached = core
        .component_logging()
        .context("logging was lost under Off")?;
    let before = LogMessage::with_instance(
        LogLevel::Info,
        "before Core shutdown",
        "off",
        input.id().to_string(),
        ComponentLogType::Source,
    );
    let key = before.key();
    assert!(attached.try_log(before));
    logging.flush().await?;
    core.shutdown().await?;
    drop(core);
    drop(bindings);
    assert!(!logging.is_closed());
    assert!(logging.try_log(LogMessage::with_instance(
        LogLevel::Info,
        "after Core shutdown",
        "off",
        input.id().to_string(),
        ComponentLogType::Source,
    )));
    logging.flush().await?;
    let history = logging.registry().get_history_by_key(&key).await;
    assert!(history
        .iter()
        .any(|entry| entry.message == "before Core shutdown"));
    assert!(history
        .iter()
        .any(|entry| entry.message == "after Core shutdown"));
    assert_eq!(context.registrations().counts()?, (0, 0));
    logging.shutdown(WAIT).await?;
    Ok(())
}

#[tokio::test]
async fn prebuilt_validation_rejects_exposed_mismatch() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let settings = TelemetrySettings::new(TelemetryProfile::Basic)
        .with_sampling(RootSampling::Ratio(SamplingRatio::new(0.25)?))
        .exclude_metrics("excluded");
    let bindings = TelemetryBindings::new(settings.clone(), logging.clone())
        .with_metric_context(context.clone());
    let options = |settings| TelemetryOptions::new(settings).with_metric_context(context.clone());
    let matching = DrasiLib::builder()
        .with_id("prebuilt")
        .with_telemetry(options(settings.clone()))
        .with_component_logging(logging.clone())
        .build()
        .await?;
    bindings.validate_prebuilt(&matching).await?;
    let distributed = TelemetryBindings::new(
        settings.clone().require_distributed_tracing(),
        logging.clone(),
    )
    .with_metric_context(context.clone())
    .with_context_adapter(Arc::new(OpenTelemetryContextAdapter));
    distributed.validate()?;
    assert!(matching.telemetry().context_adapter().is_none());
    assert!(distributed.validate_prebuilt(&matching).await.is_err());
    matching.shutdown().await?;

    let mismatches = [
        (
            "profile",
            TelemetrySettings::new(TelemetryProfile::Debug)
                .with_sampling(RootSampling::Ratio(SamplingRatio::new(0.25)?))
                .exclude_metrics("excluded"),
        ),
        ("metrics signal", settings.clone().with_metrics(false)),
        ("tracing signal", settings.clone().with_tracing(false)),
        (
            "root sampling",
            settings.clone().with_sampling(RootSampling::Never),
        ),
        (
            "sampling ratio",
            settings
                .clone()
                .with_sampling(RootSampling::Ratio(SamplingRatio::new(0.5)?)),
        ),
        ("metric filters", settings.clone().exclude_metrics("other")),
        (
            "native metrics requirement",
            settings.clone().require_native_metrics(),
        ),
        (
            "native query requirement",
            settings.clone().require_native_query_observations(),
        ),
    ];
    for (case, settings) in mismatches {
        let core = DrasiLib::builder()
            .with_id("prebuilt")
            .with_telemetry(options(settings))
            .with_component_logging(logging.clone())
            .build()
            .await?;
        let validation = bindings.validate_prebuilt(&core).await;
        core.shutdown().await?;
        assert!(validation.is_err(), "accepted mismatched {case}");
    }
    let other_logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
    let core = DrasiLib::builder()
        .with_id("prebuilt")
        .with_telemetry(options(settings))
        .with_component_logging(other_logging.clone())
        .build()
        .await?;
    let validation = bindings.validate_prebuilt(&core).await;
    core.shutdown().await?;
    other_logging.shutdown(WAIT).await?;
    logging.shutdown(WAIT).await?;
    assert!(validation.is_err(), "accepted a different logging registry");
    Ok(())
}

#[tokio::test]
async fn embedded_shutdown_budget_uses_shared_validation() -> Result<()> {
    let logging = logging()?;
    let bindings = TelemetryBindings::new(
        TelemetrySettings::new(TelemetryProfile::Off),
        logging.clone(),
    );
    bindings.validate()?;
    let mapper = DtoMapper::new();
    assert_eq!(
        TelemetryConfig::default()
            .resolve(&mapper, 8080, true)?
            .shutdown_timeout,
        Duration::from_secs(5)
    );
    for (seconds, accepted) in [(0, false), (1, true), (5, true), (300, true), (301, false)] {
        let bindings = bindings.clone().with_shutdown_timeout_seconds(seconds);
        let config = TelemetryConfig {
            shutdown_timeout_seconds: ConfigValue::Static(seconds),
            ..Default::default()
        };
        assert_eq!(bindings.validate().is_ok(), accepted, "embedded {seconds}");
        assert_eq!(
            bindings
                .apply_to(DrasiLib::builder().with_id("budget"))
                .is_ok(),
            accepted,
            "embedded builder handoff {seconds}"
        );
        let resolved = config.resolve(&mapper, 8080, true);
        assert_eq!(resolved.is_ok(), accepted, "configuration {seconds}");
        if accepted {
            assert_eq!(resolved?.shutdown_timeout, Duration::from_secs(seconds));
        }
    }
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: Server binds only its primary builder, overwrites actual
// instance identity, or allocates a second recorder for an additional Core.
#[tokio::test]
async fn builder_borrowed_policy_reaches_every_core_on_one_recorder() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let rendered = recorder.handle();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let bindings = TelemetryBindings::new(metrics_only(), logging.clone())
        .with_metric_context(context.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    let primary_calls = calls.clone();
    let preparation = crate::DrasiServerBuilder::new()
        .with_id("server-primary")
        .with_source_factory("input", true, move |id| {
            primary_calls.fetch_add(1, Ordering::SeqCst);
            let (source, _) = ApplicationSource::new(
                id.as_str(),
                ApplicationSourceConfig {
                    properties: Default::default(),
                    durability: None,
                },
            )?;
            Ok(source)
        })
        .with_telemetry_bindings(TelemetryBindings::new(metrics_only(), logging.clone()))
        .with_telemetry_bindings(bindings)
        .add_instance_builder(
            source_builder("server-secondary", calls.clone()).with_telemetry(
                TelemetryOptions::new(TelemetrySettings::new(TelemetryProfile::Off)),
            ),
        )
        .prepare_with_owned_handles();
    let cleanup = preparation.cleanup_handle();
    let built = preparation.finish().await?;
    assert_eq!(built.servers.len(), 2);
    assert!(Arc::ptr_eq(
        &built.server,
        built.servers.get("server-primary").context("primary key")?
    ));
    for id in ["server-primary", "server-secondary"] {
        let core = built.servers.get(id).context("actual Core ID missing")?;
        assert_eq!(core.get_config().id, id);
        assert_eq!(core.telemetry().policy().profile(), TelemetryProfile::Basic);
        assert_eq!(
            core.telemetry()
                .scope()
                .context("missing scope")?
                .instance_id(),
            id
        );
        assert!(Arc::ptr_eq(&core.log_registry(), &logging.registry()));
        let input = core.component_by_name("input")?;
        tokio::time::timeout(WAIT, input.wait_started()).await??;
        let metrics = rendered.render();
        let component = format!("component_id=\"{}\"", input.id());
        let observations = metrics
            .lines()
            .filter(|line| line.starts_with("drasi_component_") && line.contains(&component))
            .collect::<Vec<_>>();
        assert!(!observations.is_empty(), "missing observations for {id}");
        assert!(observations
            .iter()
            .all(|line| line.contains(&format!("instance_id=\"{id}\""))));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    built.shutdown().await?;
    assert_eq!(
        cleanup.status().phase,
        crate::server::CleanupPhase::Complete
    );
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: legacy result paths reject borrowed collection or drop an
// implicit new owner which shuts down the returned Cores. The literal keeps
// the old public result's two-field construction contract exercised.
#[tokio::test]
async fn builder_legacy_results_keep_borrowed_handles_and_literal_shape() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let bindings =
        TelemetryBindings::new(metrics_only(), logging.clone()).with_metric_context(context);
    let core = crate::DrasiServerBuilder::new()
        .with_id("legacy-core")
        .with_telemetry_bindings(bindings.clone())
        .build_core()
        .await?;
    assert_eq!(core.telemetry().policy().profile(), TelemetryProfile::Basic);
    assert!(core.telemetry().metric_context().is_some());
    core.start().await?;
    core.shutdown().await?;
    assert!(!logging.is_closed());

    let built = crate::DrasiServerBuilder::new()
        .with_id("legacy-handles")
        .with_telemetry_bindings(bindings)
        .build_with_handles()
        .await?;
    let crate::DrasiServerWithHandles { server, servers } = built;
    let built = crate::DrasiServerWithHandles { server, servers };
    assert_eq!(built.servers.len(), 1);
    assert!(Arc::ptr_eq(
        &built.server,
        built
            .servers
            .get("legacy-handles")
            .context("actual ID key")?
    ));
    assert_eq!(
        built.server.telemetry().policy().profile(),
        TelemetryProfile::Basic
    );
    built.server.shutdown().await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

// Break caught: checked wrapping retrofits collection into an existing Core,
// rejects a matching exposed policy, or takes ownership of borrowed providers.
#[tokio::test]
async fn builder_checked_prebuilt_wrapper_accepts_existing_collection() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let bindings =
        TelemetryBindings::new(metrics_only(), logging.clone()).with_metric_context(context);
    let calls = Arc::new(AtomicUsize::new(0));
    let core = bindings
        .apply_to(source_builder("checked-existing", calls.clone()))?
        .build()
        .await?;
    core.start().await?;
    let input = core.component_by_name("input")?;
    tokio::time::timeout(WAIT, input.wait_started()).await??;
    let retained = core.clone();
    let server = crate::DrasiServer::from_core_with_telemetry(
        core,
        false,
        false,
        "127.0.0.1".into(),
        8080,
        None,
        bindings,
    )
    .await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(retained.component_by_name("input")?.id(), input.id());
    assert!(Arc::ptr_eq(&retained.log_registry(), &logging.registry()));
    server
        .cleanup_handle()
        .wait_until(tokio::time::Instant::now() + WAIT)
        .await?;
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

#[tokio::test]
async fn builder_invalid_final_bindings_do_not_construct_components() -> Result<()> {
    let logging = logging()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let preparation = crate::DrasiServerBuilder::new()
        .with_telemetry_bindings(TelemetryBindings::new(
            TelemetrySettings::new(TelemetryProfile::Off),
            logging.clone(),
        ))
        .with_telemetry_bindings(TelemetryBindings::new(metrics_only(), logging.clone()))
        .add_instance_builder(source_builder("never-built", calls.clone()))
        .prepare();
    let cleanup = preparation.cleanup_handle();
    let result = preparation.finish().await;
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        cleanup.status().phase,
        crate::server::CleanupPhase::Complete
    );
    assert!(!logging.is_closed());
    logging.shutdown(WAIT).await?;
    Ok(())
}

#[tokio::test]
async fn builder_checked_prebuilt_wrapper_rejects_policy_and_capability_mismatch() -> Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let context = MetricRegistrationContext::new(Arc::new(recorder), Default::default())?;
    let logging = logging()?;
    let expected = TelemetryBindings::new(metrics_only(), logging.clone())
        .with_metric_context(context.clone());
    for (settings, bindings) in [
        (
            TelemetrySettings::new(TelemetryProfile::Off),
            expected.clone(),
        ),
        (
            metrics_only(),
            expected
                .clone()
                .with_context_adapter(Arc::new(OpenTelemetryContextAdapter)),
        ),
    ] {
        let core = DrasiLib::builder()
            .with_id("checked-mismatch")
            .with_telemetry(TelemetryOptions::new(settings).with_metric_context(context.clone()))
            .with_component_logging(logging.clone())
            .build()
            .await?;
        assert!(crate::DrasiServer::from_core_with_telemetry(
            core,
            false,
            false,
            "127.0.0.1".into(),
            8080,
            None,
            bindings,
        )
        .await
        .is_err());
        assert!(!logging.is_closed());
    }
    logging.shutdown(WAIT).await?;
    Ok(())
}
