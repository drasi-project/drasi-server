// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Optional standalone logging composition, independent of collection policy.
//!
//! The caller obtains the cleanup address before awaiting installation. Global
//! logger/subscriber conflicts are errors, never requests to borrow an unknown
//! host integration. Global installation cannot be undone: after a partial
//! failure or shutdown, start a fresh process rather than retrying installation.
//! Embedded hosts should compose their own layers and use `TelemetryBindings`.
//! Synchronous capture initialization runs in an owned blocking worker, so
//! current-thread executors remain responsive while startup is pending.

use anyhow::Result;
use drasi_lib::observability::{
    ComponentLogRuntime, LogPolicy, LoggingError, TelemetryProfile, TelemetrySettings,
};
use tracing_subscriber::{prelude::*, EnvFilter};

use crate::{
    config::telemetry::EffectiveTelemetry,
    server::{ServerCleanupHandle, ServerPreparation},
    TelemetryBindings,
};

/// Owns standalone component-log capture and its retained cleanup operation.
///
/// This installs only console logging, capture and the `log` bridge, not
/// exporters, collection providers, recorders or listeners. Moving this value
/// transfers ownership; cloning [`Self::bindings`] only lends collection handles.
/// Keep [`Self::cleanup_handle`] and await its cleanup before stopping Tokio.
/// Dropping this value requests cleanup, but cannot await it.
///
/// ```no_run
/// use drasi_server::telemetry::StandaloneTelemetry;
/// use std::time::Duration;
/// use tokio::time::Instant;
///
/// # async fn example() -> anyhow::Result<()> {
/// let preparation = StandaloneTelemetry::prepare("info".into());
/// let cleanup = preparation.cleanup_handle();
/// let telemetry = preparation.finish().await?;
/// log::info!("logging is available before application construction");
/// // Pass telemetry.bindings() to Core builders; retain this owner.
/// cleanup.wait_until(Instant::now() + Duration::from_secs(30)).await?;
/// # Ok(())
/// # }
/// ```
pub struct StandaloneTelemetry {
    bindings: TelemetryBindings,
    cleanup: ServerCleanupHandle,
    transferred: bool,
}

impl StandaloneTelemetry {
    /// Prepare logs-only ownership with collection Off on either Tokio flavor.
    ///
    /// Call inside a live Tokio runtime. `RUST_LOG` takes precedence; when
    /// absent, use `<log_level>,oci_client=error`. Invalid directives and
    /// unrelated installed globals fail with the same reachable cleanup owner.
    /// No environment variables are changed.
    pub fn prepare(log_level: String) -> ServerPreparation<Self> {
        Self::prepare_inner(log_level, None)
    }

    /// Task 5 supplies already-resolved standalone policy before Core creation.
    #[allow(dead_code)]
    pub(crate) fn prepare_configured(
        log_level: String,
        effective: EffectiveTelemetry,
    ) -> ServerPreparation<Self> {
        Self::prepare_inner(log_level, Some(effective))
    }

    fn prepare_inner(
        log_level: String,
        effective: Option<EffectiveTelemetry>,
    ) -> ServerPreparation<Self> {
        Self::prepare_with(log_level, effective, |policy| {
            ComponentLogRuntime::new(policy).map_err(Into::into)
        })
    }

    fn prepare_with(
        log_level: String,
        effective: Option<EffectiveTelemetry>,
        constructor: impl FnOnce(LogPolicy) -> Result<ComponentLogRuntime> + Send + 'static,
    ) -> ServerPreparation<Self> {
        let cleanup = ServerCleanupHandle::bootstrap();
        let submitted: Result<ServerPreparation<Self>> = (|| {
            let admission = cleanup.admit()?;
            ServerPreparation::start_blocking(admission, move |owner| {
                let bindings = Self::initialize(&owner, log_level, effective, constructor)?;
                Ok(Self {
                    bindings,
                    cleanup: owner,
                    transferred: false,
                })
            })
        })();
        match submitted {
            Ok(preparation) => preparation,
            Err(error) => ServerPreparation::failed(cleanup, error),
        }
    }

    /// Called only inside Server's admitted, owned blocking initializer. Capture
    /// joins the same owner as its producers, rather than a nested preparation.
    pub(crate) fn initialize_configured(
        owner: &ServerCleanupHandle,
        effective: EffectiveTelemetry,
    ) -> Result<TelemetryBindings> {
        Self::initialize(owner, "info".into(), Some(effective), |policy| {
            ComponentLogRuntime::new(policy).map_err(Into::into)
        })
    }

    pub(crate) fn initialize_logging(
        owner: &ServerCleanupHandle,
        log_level: String,
    ) -> Result<TelemetryBindings> {
        Self::initialize(owner, log_level, None, |policy| {
            ComponentLogRuntime::new(policy).map_err(Into::into)
        })
    }

    pub(crate) fn into_server_parts(mut self) -> (TelemetryBindings, ServerCleanupHandle) {
        self.transferred = true;
        (self.bindings.clone(), self.cleanup.clone())
    }

    fn initialize(
        owner: &ServerCleanupHandle,
        log_level: String,
        effective: Option<EffectiveTelemetry>,
        constructor: impl FnOnce(LogPolicy) -> Result<ComponentLogRuntime>,
    ) -> Result<TelemetryBindings> {
        if let Some(effective) = &effective {
            effective.ensure_runtime_available()?;
        }
        let directives = match std::env::var("RUST_LOG") {
            Ok(directives) => directives,
            Err(std::env::VarError::NotPresent) => format!("{log_level},oci_client=error"),
            Err(error) => return Err(LoggingError::Configuration(error.to_string()).into()),
        };
        let policy = LogPolicy::new(&directives)?;
        let console_filter = EnvFilter::try_new(&directives)
            .map_err(|error| LoggingError::Configuration(error.to_string()))?;
        let logging = constructor(policy.clone())?;
        owner.retain_logging(logging.clone())?;
        let settings = effective
            .as_ref()
            .map(|effective| effective.settings().clone())
            .unwrap_or_else(|| TelemetrySettings::new(TelemetryProfile::Off));
        let mut bindings = TelemetryBindings::new(settings, logging.clone());
        if let Some(effective) = effective {
            bindings = bindings.with_shutdown_timeout_seconds(effective.shutdown_timeout.as_secs());
        }
        owner.retain_bindings(bindings.clone())?;
        tracing_log::LogTracer::builder()
            .with_max_level(policy.max_level())
            .init()
            .map_err(|_| LoggingError::LoggerConflict)?;
        let subscriber = tracing_subscriber::registry()
            .with(logging.layer())
            .with(tracing_subscriber::fmt::layer().with_filter(console_filter));
        tracing::subscriber::set_global_default(subscriber)
            .map_err(|_| LoggingError::SubscriberConflict)?;
        Ok(bindings)
    }

    /// Lend handles without transferring or extending shutdown responsibility.
    pub fn bindings(&self) -> TelemetryBindings {
        self.bindings.clone()
    }

    /// Address the same cancellation-safe owner as the original preparation.
    pub fn cleanup_handle(&self) -> ServerCleanupHandle {
        self.cleanup.clone()
    }
}

impl Drop for StandaloneTelemetry {
    fn drop(&mut self) {
        if self.transferred {
            return;
        }
        if let Err(error) = self.cleanup.request_shutdown() {
            log::error!(
                "Standalone logging cleanup remains owned but could not be scheduled: {error}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{lifecycle::test_support::owner_liveness, CleanupPhase};
    use drasi_lib::observability::{ComponentLogKey, ComponentLogType, LogLevel, LogMessage};
    use std::{
        process::Command,
        sync::{mpsc, Arc, Condvar, Mutex},
        time::Duration,
    };
    use tokio::{
        sync::oneshot,
        time::{timeout, Instant},
    };

    const WAIT: Duration = Duration::from_secs(5);
    const CASE: &str = "DRASI_TASK3_INITIALIZATION_CASE";
    const COMPONENT: &str = "00000000-0000-4000-8000-000000000304";

    #[derive(Default)]
    struct Gate {
        open: Mutex<bool>,
        changed: Condvar,
    }

    impl Gate {
        fn wait(&self) {
            let open = self.open.lock().expect("gate lock");
            drop(
                self.changed
                    .wait_while(open, |open| !*open)
                    .expect("gate wait"),
            );
        }

        fn release(&self) {
            *self.open.lock().expect("gate lock") = true;
            self.changed.notify_all();
        }
    }

    struct ReleaseOnDrop(Arc<Gate>, Arc<Gate>);

    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            self.0.release();
            self.1.release();
        }
    }

    fn child(case: &str) -> Result<()> {
        let output = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "telemetry::runtime::tests::initialization_child",
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
    fn cancelled_logging_initialization_does_not_block_current_thread() -> Result<()> {
        child("cancelled")
    }

    #[test]
    fn expired_logging_initialization_retains_actual_capture() -> Result<()> {
        child("expired")
    }

    #[test]
    #[ignore = "isolated global-installation subprocess entry point"]
    fn initialization_child() -> Result<()> {
        let case = std::env::var(CASE)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(initialization_case(&case))
    }

    // Break caught: capture construction blocks the current-thread executor, or
    // abandoning all public waiters lets cleanup finish before a late capture
    // is registered and joined. Gates, not elapsed sleeps, establish ordering.
    async fn initialization_case(case: &str) -> Result<()> {
        let before_capture = Arc::new(Gate::default());
        let before_return = Arc::new(Gate::default());
        let _release_on_drop = ReleaseOnDrop(before_capture.clone(), before_return.clone());
        let executor_thread = std::thread::current().id();
        let (entered, entering) = mpsc::channel();
        let (observed, observation) = oneshot::channel();
        let (created, capture) = oneshot::channel();
        let supervisor = std::thread::spawn({
            let before_capture = before_capture.clone();
            let before_return = before_return.clone();
            move || {
                let on_executor = entering.recv().expect("constructor enters") == executor_thread;
                if on_executor {
                    // Let the broken implementation terminate deterministically,
                    // without relying on a timeout running on its blocked executor.
                    before_capture.release();
                    before_return.release();
                }
                observed
                    .send(on_executor)
                    .expect("executor observer present");
            }
        });
        let preparation = StandaloneTelemetry::prepare_with("info".into(), None, {
            let before_capture = before_capture.clone();
            let before_return = before_return.clone();
            move |policy| {
                entered
                    .send(std::thread::current().id())
                    .expect("supervisor present");
                before_capture.wait();
                let logging = ComponentLogRuntime::new(policy)?;
                created
                    .send(logging.clone())
                    .expect("capture observer present");
                before_return.wait();
                Ok(logging)
            }
        });
        let cleanup = preparation.cleanup_handle();
        let owner_alive = owner_liveness(&cleanup);
        let on_executor = timeout(WAIT, observation).await??;
        supervisor.join().expect("initializer supervisor");
        if on_executor {
            let logging = timeout(WAIT, capture).await??;
            drop(preparation);
            cleanup.wait_until(Instant::now() + WAIT).await?;
            assert!(logging.is_closed());
            assert!(
                !on_executor,
                "capture initialization ran on and blocked the current-thread Tokio executor"
            );
            return Ok(());
        }

        // This distinct Tokio task must progress while synchronous construction
        // is still gated. The expired-wait case also exercises deadline timers.
        tokio::spawn(async {}).await?;
        drop(preparation);
        match case {
            "cancelled" => {
                let wait = cleanup.wait_until(Instant::now() + WAIT);
                tokio::pin!(wait);
                assert!(futures::poll!(&mut wait).is_pending());
            }
            "expired" => {
                let error = cleanup
                    .wait_until(Instant::now())
                    .await
                    .expect_err("the blocked initializer cannot finish before this deadline");
                assert!(cleanup.status().deadline_missed);
                drop(error);
            }
            _ => anyhow::bail!("unknown initialization scenario"),
        }
        assert_ne!(cleanup.status().phase, CleanupPhase::Complete);
        assert!(cleanup.status().pending_workers > 0);
        drop(cleanup);
        assert!(
            owner_alive(),
            "last public handle loss released pending initialization"
        );

        before_capture.release();
        let logging = timeout(WAIT, capture).await??;
        assert!(
            owner_alive(),
            "the constructor can still hand off a real capture"
        );
        assert!(
            !logging.is_closed(),
            "construction is still gated before handoff"
        );
        assert!(logging.try_log(LogMessage::with_instance(
            LogLevel::Info,
            "capture admitted after its last public waiter disappeared",
            "initializer",
            COMPONENT,
            ComponentLogType::Source,
        )));
        before_return.release();
        timeout(WAIT, async {
            while owner_alive() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(logging.is_closed(), "cleanup released its unjoined capture");
        let key = ComponentLogKey::new("initializer", ComponentLogType::Source, COMPONENT);
        assert!(logging
            .registry()
            .get_history_by_key(&key)
            .await
            .iter()
            .any(|entry| entry.message
                == "capture admitted after its last public waiter disappeared"));
        Ok(())
    }
}
