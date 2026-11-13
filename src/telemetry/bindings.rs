// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Validated, borrowed collection handles for embedded Core instances.

use std::{fmt, sync::Arc, time::Duration};

use anyhow::{ensure, Context, Result};
use drasi_lib::{
    observability::{
        ComponentLogRuntime, MetricRegistrationContext, NativeSpanCollector, TelemetryOptions,
        TelemetrySettings, TraceContextAdapter,
    },
    DrasiLib, DrasiLibBuilder,
};

use crate::config::telemetry::{
    validate_shutdown_timeout_seconds, DEFAULT_SHUTDOWN_TIMEOUT_SECONDS,
};

/// Collection policy and application-supplied, borrowed telemetry handles.
///
/// The application retains lifecycle responsibility for logging, recorders,
/// context integration and native span collection. Neither applying nor dropping
/// bindings flushes or shuts down these handles. Collection profiles never
/// disable component logging. No global subscriber, recorder or logger is
/// installed, and no provider, exporter or listener is constructed.
///
/// These bindings are not a configured export destination. Configuration-driven
/// startup must still enforce
/// [`EffectiveTelemetry::ensure_runtime_available`](crate::config::telemetry::EffectiveTelemetry::ensure_runtime_available).
///
/// ```no_run
/// use drasi_lib::{
///     observability::{
///         ComponentLogRuntime, MetricRegistrationContext, TelemetryProfile, TelemetrySettings,
///     },
///     DrasiLib,
/// };
/// use drasi_server::TelemetryBindings;
///
/// # async fn example(
/// #     logging: ComponentLogRuntime,
/// #     metrics: MetricRegistrationContext,
/// # ) -> anyhow::Result<()> {
/// let bindings = TelemetryBindings::new(
///     TelemetrySettings::new(TelemetryProfile::Basic).with_tracing(false),
///     logging.clone(),
/// )
/// .with_metric_context(metrics);
/// bindings.validate()?;
/// let core = bindings
///     .apply_to(DrasiLib::builder().with_id("analytics"))?
///     .build()
///     .await?;
/// core.shutdown().await?;
/// // Logging remains open; its application owner decides when to shut it down.
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct TelemetryBindings {
    settings: TelemetrySettings,
    logging: ComponentLogRuntime,
    metrics: Option<MetricRegistrationContext>,
    context_adapter: Option<Arc<dyn TraceContextAdapter>>,
    native_spans: Option<NativeSpanCollector>,
    shutdown_timeout_seconds: u64,
}

impl fmt::Debug for TelemetryBindings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TelemetryBindings")
            .finish_non_exhaustive()
    }
}

impl TelemetryBindings {
    /// Borrow logging independently of the collection profile.
    ///
    /// Explicit validation is required before handoff; modifiers do not
    /// construct components or validate partially assembled bindings.
    pub fn new(settings: TelemetrySettings, logging: ComponentLogRuntime) -> Self {
        Self {
            settings,
            logging,
            metrics: None,
            context_adapter: None,
            native_spans: None,
            shutdown_timeout_seconds: DEFAULT_SHUTDOWN_TIMEOUT_SECONDS,
        }
    }

    /// Borrow the registration context shared by instances using one recorder.
    pub fn with_metric_context(mut self, context: MetricRegistrationContext) -> Self {
        self.metrics = Some(context);
        self
    }

    /// Borrow the host's context integration; this does not install a subscriber.
    pub fn with_context_adapter(mut self, adapter: Arc<dyn TraceContextAdapter>) -> Self {
        self.context_adapter = Some(adapter);
        self
    }

    /// Borrow native span collection without taking responsibility for shutdown.
    pub fn with_native_span_collector(mut self, collector: NativeSpanCollector) -> Self {
        self.native_spans = Some(collector);
        self
    }

    /// Set the telemetry shutdown budget: 1–300 seconds, defaulting to five.
    ///
    /// Validation shares the configuration-file bound. This does not change
    /// Core's component lifecycle timeout or close borrowed handles.
    pub fn with_shutdown_timeout_seconds(mut self, seconds: u64) -> Self {
        self.shutdown_timeout_seconds = seconds;
        self
    }

    pub(crate) fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_seconds)
    }

    pub(crate) fn logging(&self) -> ComponentLogRuntime {
        self.logging.clone()
    }

    pub(crate) fn plugin_callback_context(
        &self,
    ) -> Result<Arc<drasi_host_sdk::CallbackContext>> {
        Ok(Arc::new(drasi_host_sdk::CallbackContext {
            instance_id: String::new(),
            runtime_handle: tokio::runtime::Handle::try_current()?,
            log_registry: self.logging.registry(),
            source_event_history: Arc::new(tokio::sync::RwLock::new(
                drasi_lib::observability::ComponentEventHistory::new(),
            )),
            reaction_event_history: Arc::new(tokio::sync::RwLock::new(
                drasi_lib::observability::ComponentEventHistory::new(),
            )),
        }))
    }

    pub(crate) fn with_configured_policy(
        mut self,
        effective: crate::config::telemetry::EffectiveTelemetry,
    ) -> Result<Self> {
        effective.ensure_runtime_available()?;
        self.settings = effective.settings().clone();
        self.shutdown_timeout_seconds = effective.shutdown_timeout.as_secs();
        self.validate()?;
        Ok(self)
    }

    /// Validate policy, shutdown bounds and required exposed collection handles.
    pub fn validate(&self) -> Result<()> {
        let policy = self.settings.resolve()?;
        validate_shutdown_timeout_seconds(self.shutdown_timeout_seconds)?;
        ensure!(
            !self.logging.is_closed(),
            "Telemetry bindings require open component logging"
        );
        ensure!(
            !policy.metrics_enabled() || self.metrics.is_some(),
            "Enabled metrics require an explicit MetricRegistrationContext"
        );
        ensure!(
            !policy.distributed_tracing_required() || self.context_adapter.is_some(),
            "Distributed tracing requires an explicit TraceContextAdapter"
        );
        Ok(())
    }

    /// Validate and attach borrowed handles without building components.
    ///
    /// Core resolves telemetry against the builder's actual instance identity;
    /// the binding deliberately never supplies a telemetry instance-ID override.
    pub fn apply_to(&self, builder: DrasiLibBuilder) -> Result<DrasiLibBuilder> {
        self.validate()?;
        let mut options = TelemetryOptions::new(self.settings.clone());
        if let Some(context) = &self.metrics {
            options = options.with_metric_context(context.clone());
        }
        if let Some(adapter) = &self.context_adapter {
            options = options.with_context_adapter(adapter.clone());
        }
        if let Some(collector) = &self.native_spans {
            options = options.with_native_span_collector(collector.clone());
        }
        Ok(builder
            .with_telemetry(options)
            .with_component_logging(self.logging.clone()))
    }

    /// Validate an independently preconfigured Core without mutating it.
    ///
    /// Checks exposed policy, instance scope, required handle presence and shared
    /// logging registry identity. Core exposes no metric-context or native-
    /// collector identity comparison, nor native collector/subscriber inspection.
    /// The caller must ensure these opaque integrations are equivalent to the
    /// supplied bindings, are composed into the host subscriber as needed, and
    /// remain live. Success does not certify their identity or exporter health.
    pub async fn validate_prebuilt(&self, core: &DrasiLib) -> Result<()> {
        self.validate()?;
        let expected = self.settings.resolve()?;
        let telemetry = core.telemetry();
        let actual = telemetry.policy();
        ensure!(
            actual.profile() == expected.profile()
                && actual.tracing_enabled() == expected.tracing_enabled()
                && actual.metrics_enabled() == expected.metrics_enabled()
                && actual.sampling() == expected.sampling()
                && actual.metric_exclusions() == expected.metric_exclusions()
                && actual.distributed_tracing_required() == expected.distributed_tracing_required()
                && actual.native_metrics_required() == expected.native_metrics_required()
                && actual.native_query_observations_required()
                    == expected.native_query_observations_required()
                && actual.handled_branch_limits() == expected.handled_branch_limits(),
            "Prebuilt Core telemetry policy does not match the bindings"
        );
        if expected.enabled() {
            let scope = telemetry
                .scope()
                .context("Prebuilt Core is missing its telemetry instance scope")?;
            ensure!(
                scope.instance_id() == core.get_config().id && scope.component_id().is_none(),
                "Prebuilt Core telemetry scope does not match its actual instance"
            );
        } else {
            ensure!(
                telemetry.scope().is_none(),
                "Prebuilt Core has a scope for disabled telemetry"
            );
        }
        ensure!(
            !expected.metrics_enabled() || telemetry.metric_context().is_some(),
            "Prebuilt Core is missing the required metric registration context"
        );
        ensure!(
            self.context_adapter.is_none() || telemetry.context_adapter().is_some(),
            "Prebuilt Core is missing the required trace context adapter"
        );
        let logging = core
            .component_logging()
            .context("Prebuilt Core is missing explicit component logging")?;
        ensure!(
            !logging.is_closed(),
            "Prebuilt Core component logging is closed"
        );
        ensure!(
            Arc::ptr_eq(&logging.registry(), &self.logging.registry())
                && Arc::ptr_eq(&core.log_registry(), &self.logging.registry()),
            "Prebuilt Core component logging registry does not match the bindings"
        );
        Ok(())
    }
}
