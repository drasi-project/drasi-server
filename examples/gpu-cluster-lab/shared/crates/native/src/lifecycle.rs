use crate::{inputs::BootstrapBoundary, status::Hub, RuntimeObservation};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use drasi_computation_plugin_sdk::{
    ControlMessage, ControlNotification, ControlSender, NativeControlHandler,
};
use std::sync::{Arc, Mutex};

pub const BOOTSTRAP_COMPLETE: &str = "gpu.lab/database-bootstrap-complete";
pub const RUNTIME_OBSERVATION: &str = "gpu.lab/runtime-observation";

#[derive(Default)]
pub(crate) struct Signals {
    pub bootstrap: Option<BootstrapBoundary>,
    pub unavailable: Option<String>,
}

pub(crate) struct Handler {
    pub signals: Option<Arc<Mutex<Signals>>>,
    pub hub: Arc<Hub>,
}

#[async_trait]
impl NativeControlHandler for Handler {
    async fn on_message(&self, message: ControlMessage, _control: ControlSender) -> Result<()> {
        match message.notification {
            ControlNotification::Custom { kind, payload } if kind == RUNTIME_OBSERVATION => {
                ensure!(
                    self.signals.is_none(),
                    "runtime observations target the status producer"
                );
                self.hub
                    .observe_runtime(serde_json::from_value::<RuntimeObservation>(payload)?)
            }
            ControlNotification::Custom { kind, payload } if kind == BOOTSTRAP_COMPLETE => {
                let boundary: BootstrapBoundary = serde_json::from_value(payload)?;
                boundary.validate()?;
                let mut signals = self
                    .signals
                    .as_ref()
                    .context("component has no database inputs")?
                    .lock()
                    .map_err(|_| anyhow::anyhow!("lifecycle signal lock poisoned"))?;
                signals.bootstrap = Some(boundary);
                Ok(())
            }
            ControlNotification::Unavailable { reason } => {
                if let Some(signals) = &self.signals {
                    let mut signals = signals
                        .lock()
                        .map_err(|_| anyhow::anyhow!("lifecycle signal lock poisoned"))?;
                    signals.bootstrap = None;
                    signals.unavailable = Some(reason);
                }
                Ok(())
            }
            ControlNotification::Custom { kind, .. } => {
                anyhow::bail!("unsupported GPU runtime control message: {kind}")
            }
            ControlNotification::Ready
            | ControlNotification::NotReady
            | ControlNotification::Available => Ok(()),
        }
    }
}
