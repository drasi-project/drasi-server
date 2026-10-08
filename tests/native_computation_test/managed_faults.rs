use super::*;
use drasi_lib::management::{
    AcceptanceReceipt, CommittedConfiguration, ConfigurationSession, ConfigurationStore,
    DesiredInstance,
};
use drasi_state_store_redb::RedbConfigurationStore;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Notify, Semaphore};

pub(super) struct ConfirmationGate {
    pub(super) armed: AtomicBool,
    pub(super) loads_blocked: AtomicBool,
    pub(super) committed: Notify,
    pub(super) release: Semaphore,
    pub(super) fail: bool,
    pub(super) reject: bool,
}

pub(super) struct GatedStore {
    pub(super) inner: RedbConfigurationStore,
    pub(super) gate: Arc<ConfirmationGate>,
}

struct GatedSession {
    inner: Arc<dyn ConfigurationSession>,
    gate: Arc<ConfirmationGate>,
}

#[async_trait::async_trait]
impl ConfigurationStore for GatedStore {
    async fn open(&self, instance: &str) -> Result<Arc<dyn ConfigurationSession>> {
        Ok(Arc::new(GatedSession {
            inner: self.inner.open(instance).await?,
            gate: self.gate.clone(),
        }))
    }
}

#[async_trait::async_trait]
impl ConfigurationSession for GatedSession {
    async fn load(&self) -> Result<CommittedConfiguration> {
        anyhow::ensure!(
            !self.gate.loads_blocked.load(Ordering::SeqCst),
            "private store read failure"
        );
        self.inner.load().await
    }

    async fn commit(
        &self,
        expected_revision: u64,
        request: &str,
        desired: &DesiredInstance,
    ) -> Result<AcceptanceReceipt> {
        let armed = self.gate.armed.swap(false, Ordering::SeqCst);
        if armed && self.gate.reject {
            self.gate.committed.notify_one();
            self.gate.release.acquire().await?.forget();
            self.gate
                .loads_blocked
                .store(self.gate.fail, Ordering::SeqCst);
            anyhow::bail!("private store rejected commit");
        }
        let receipt = self
            .inner
            .commit(expected_revision, request, desired)
            .await?;
        if armed {
            self.gate.committed.notify_one();
            self.gate.release.acquire().await?.forget();
            if self.gate.fail {
                self.gate.loads_blocked.store(true, Ordering::SeqCst);
                anyhow::bail!("private store confirmation failure");
            }
        }
        Ok(receipt)
    }

    async fn receipt(&self, request: &str) -> Result<Option<AcceptanceReceipt>> {
        self.inner.receipt(request).await
    }

    async fn snapshot(&self, name: &str) -> Result<CommittedConfiguration> {
        self.inner.snapshot(name).await
    }

    async fn load_snapshot(&self, name: &str) -> Result<Option<CommittedConfiguration>> {
        self.inner.load_snapshot(name).await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_requests_and_uncertain_confirmation_resolve_through_durable_router_receipts(
) -> Result<()> {
    for fail_confirmation in [false, true] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("configuration.redb");
        let mut desired = DesiredInstance::default();
        desired.topology.allow_incomplete = !desired.topology.allow_incomplete;
        {
            let gate = Arc::new(ConfirmationGate {
                armed: AtomicBool::new(true),
                loads_blocked: AtomicBool::new(false),
                committed: Notify::new(),
                release: Semaphore::new(0),
                fail: fail_confirmation,
                reject: false,
            });
            let core = Arc::new(
                DrasiLib::builder()
                    .with_id("confirmation")
                    .with_configuration_store(Arc::new(GatedStore {
                        inner: RedbConfigurationStore::new(&path, [19; 32])?,
                        gate: gate.clone(),
                    }))
                    .build()
                    .await?,
            );
            core.start().await?;
            let registry = InstanceRegistry::new();
            registry
                .add("confirmation".into(), core.clone())
                .await
                .map_err(anyhow::Error::msg)?;
            let app = build_v1_router(
                registry,
                Arc::new(false),
                None,
                Arc::new(RwLock::new(PluginRegistry::new())),
                None,
            );
            let payload = json!({"expectedRevision":0, "requestId":"uncertain", "desired":desired});
            let caller_app = app.clone();
            let body = payload.to_string();
            let caller = tokio::spawn(async move {
                request(
                    &caller_app,
                    "PUT",
                    "/instances/confirmation/computation/desired",
                    &body,
                    "application/json",
                )
                .await
            });
            tokio::time::timeout(Duration::from_secs(5), gate.committed.notified()).await?;
            assert_eq!(
                core.desired_configuration()?.revision,
                0,
                "in-flight acceptance must not be published before confirmation"
            );
            if !fail_confirmation {
                caller.abort();
                assert!(caller.await.unwrap_err().is_cancelled());
                gate.release.add_permits(1);
            } else {
                gate.release.add_permits(1);
                let (status, body) = caller.await??;
                assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
                assert_eq!(body["code"], "CONFIGURATION_UNCONFIRMED");
                assert!(!body.to_string().contains("private store"));
                assert!(core.desired_configuration().is_err());
                let (status, body) = request(
                    &app,
                    "GET",
                    "/instances/confirmation/computation/desired",
                    "",
                    "application/json",
                )
                .await?;
                assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
                assert!(!body.to_string().contains("private store"));
            }
            let (status, receipt) = request(
                &app,
                "GET",
                "/instances/confirmation/computation/receipts/uncertain",
                "",
                "application/json",
            )
            .await?;
            assert_eq!(status, StatusCode::OK, "{receipt}");
            assert_eq!(receipt["data"]["revision"], 1);
            assert_eq!(receipt["data"]["durable"], true);
            gate.loads_blocked.store(false, Ordering::SeqCst);
            let (status, body) = request(
                &app,
                "POST",
                "/instances/confirmation/computation/reconcile",
                "",
                "application/json",
            )
            .await?;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["data"]["state"], "ready");
            assert_eq!(core.desired_configuration()?.desired, desired);
            let (status, retry) = request(
                &app,
                "PUT",
                "/instances/confirmation/computation/desired",
                &payload.to_string(),
                "application/json",
            )
            .await?;
            assert_eq!(status, StatusCode::ACCEPTED, "{retry}");
            assert_eq!(retry["data"], receipt["data"]);
            core.shutdown().await?;
        }
        let reopened = RedbConfigurationStore::new(&path, [19; 32])?;
        let session = reopened.open("confirmation").await?;
        assert_eq!(session.load().await?.desired, desired);
        assert_eq!(session.receipt("uncertain").await?.unwrap().revision, 1);
        session.close().await?;
    }
    Ok(())
}
