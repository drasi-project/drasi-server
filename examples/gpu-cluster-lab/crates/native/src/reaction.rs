use crate::status::Hub;
use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::computation::v1::{
    ChangeOperation, ComponentDescriptor, ComputationComponent, EnvelopeSink, InputEnvelope,
    QueryChangeCodec, SinkCompletion,
};
use gpu_contracts::Candidate;
use gpu_control::writer::{Outcome, PlanWriter};
use serde::Deserialize;
use std::sync::Arc;
use tokio::{sync::watch, task::JoinHandle};
use uuid::Uuid;

pub struct Reaction {
    descriptor: ComponentDescriptor,
    configuration: serde_json::Value,
    hub: Arc<Hub>,
    sender: Option<watch::Sender<Option<Candidate>>>,
    desired: Option<watch::Sender<Option<Uuid>>>,
    task: Option<JoinHandle<Result<()>>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    endpoint: String,
    token: String,
}
impl Reaction {
    pub fn new(
        descriptor: ComponentDescriptor,
        configuration: serde_json::Value,
        hub: Arc<Hub>,
    ) -> Result<Self> {
        let _: Config = serde_json::from_value(configuration.clone())?;
        Ok(Self {
            descriptor,
            configuration,
            hub,
            sender: None,
            desired: None,
            task: None,
        })
    }
    fn decode(&self, input: InputEnvelope) -> Result<Option<Candidate>> {
        anyhow::ensure!(input.port.as_str() == "in", "invalid plan reaction port");
        let mut candidate = None;
        let mut after_images = 0;
        for operation in input.envelope.changes().operations() {
            let row = match operation {
                ChangeOperation::Added { after, .. } | ChangeOperation::Updated { after, .. } => {
                    after
                }
                ChangeOperation::Deleted { .. } => continue,
            };
            after_images += 1;
            anyhow::ensure!(
                after_images == 1,
                "plan reaction requires one complete singleton candidate"
            );
            let row = QueryChangeCodec::decode_row(row)?;
            let value = row
                .values
                .get("payload")
                .and_then(|v| v.as_str())
                .context("candidate query requires payload")?;
            anyhow::ensure!(value.len() <= 256 * 1024, "candidate exceeds bound");
            let decoded = serde_json::from_str(value)?;
            Hub::candidate_epoch(&decoded)?;
            candidate = Some(decoded);
        }
        Ok(candidate)
    }
}
#[async_trait]
impl ComputationComponent for Reaction {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<serde_json::Value> {
        Ok(self.configuration.clone())
    }
    async fn start(&mut self) -> Result<()> {
        anyhow::ensure!(self.task.is_none(), "plan reaction already started");
        let config: Config = serde_json::from_value(self.configuration.clone())?;
        let mut writer = PlanWriter::new(config.endpoint, config.token)?;
        let (sender, mut pending) = watch::channel::<Option<Candidate>>(None);
        let (desired, mut current) = watch::channel(None);
        let hub = self.hub.clone();
        let id = self.descriptor.id().to_string();
        self.task = Some(tokio::spawn(async move {
            while pending.changed().await.is_ok() {
                let candidate = pending.borrow_and_update().clone();
                let Some(candidate) = candidate else {
                    continue;
                };
                let epoch = Hub::candidate_epoch(&candidate)?;
                hub.publish(&id, "writing", None, Some(epoch))?;
                match writer.write(&candidate, &mut current).await {
                    Ok(Outcome::Committed(receipt)) => {
                        hub.receipt(&receipt)?;
                        hub.write_outcome(
                            &candidate,
                            "receipt",
                            "HTTP commit receipt received; awaiting committed allocation input.",
                        )?;
                        hub.event(epoch, &id, "write-receipt",
                            "Plan writer received an HTTP commit receipt. Waiting for the committed allocation input; application is not confirmed.".into(),
                            Some(candidate.decision_id), Some(receipt.plan_version.to_string()))?;
                        hub.publish(&id, "committed", None, Some(epoch))?;
                    }
                    Ok(Outcome::Conflict) => {
                        let detail = "Candidate rejected with 409; no retry";
                        hub.write_outcome(&candidate, "rejected", detail)?;
                        hub.event(
                            epoch,
                            &id,
                            "write-rejected",
                            detail.into(),
                            Some(candidate.decision_id),
                            None,
                        )?;
                        hub.publish(&id, "awaiting-newer-cdc", Some(detail.into()), Some(epoch))?;
                    }
                    Ok(Outcome::Superseded) => {
                        hub.write_outcome(&candidate, "superseded", "Candidate superseded; this is not proof that an earlier in-flight request did not commit.")?;
                        hub.publish(&id, "superseded", None, Some(epoch))?;
                    }
                    Err(error) => {
                        let detail = format!("{error:#}");
                        hub.write_outcome(&candidate, "unknown", &detail)?;
                        hub.event(
                            epoch,
                            &id,
                            "write-unknown",
                            detail.clone(),
                            Some(candidate.decision_id),
                            None,
                        )?;
                        hub.publish(&id, "write-error", Some(detail), Some(epoch))?;
                    }
                }
            }
            Ok(())
        }));
        self.sender = Some(sender);
        self.desired = Some(desired);
        self.hub
            .publish(self.descriptor.id().as_str(), "running", None, None)
    }
    async fn stop(&mut self) -> Result<()> {
        if let Some(desired) = self.desired.take() {
            desired.send_replace(None);
        }
        self.sender = None;
        if let Some(task) = self.task.take() {
            task.await.context("plan reaction worker panicked")??;
        }
        self.hub
            .publish(self.descriptor.id().as_str(), "stopped", None, None)
    }
}
#[async_trait]
impl EnvelopeSink for Reaction {
    fn completion(&self) -> SinkCompletion {
        SinkCompletion::Accepted
    }
    fn supports_snapshot(&self) -> bool {
        true
    }
    async fn replace_snapshot(&mut self, input: InputEnvelope) -> Result<()> {
        self.handle(input).await
    }
    async fn handle(&mut self, input: InputEnvelope) -> Result<()> {
        if QueryChangeCodec::is_progress_only(&input.envelope) {
            return Ok(());
        }
        let candidate = self.decode(input)?;
        self.desired
            .as_ref()
            .context("plan reaction stopped")?
            .send_replace(candidate.as_ref().map(|c| c.decision_id));
        self.sender
            .as_ref()
            .context("plan reaction stopped")?
            .send(candidate)
            .context("plan reaction worker exited")?;
        Ok(())
    }
}
