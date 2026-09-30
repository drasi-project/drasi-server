use crate::wire::Emitter;
use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::computation::v1::{
    ComponentDescriptor, ComputationComponent, EnvelopeSource, OutputEnvelope, StreamId,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::watch;
use uuid::Uuid;

pub use gpu_contracts::runtime::{RuntimeComponent, RuntimeObservation};

#[derive(Clone, Serialize, PartialEq, Eq)]
struct Status {
    schema_version: u32,
    component_id: String,
    status: String,
    error: Option<String>,
    observation_epoch: Option<Uuid>,
}
#[derive(Clone, Serialize)]
struct Event {
    event_id: String,
    observation_epoch: Uuid,
    #[serde(with = "gpu_contracts::decimal")]
    event_sequence: u64,
    time_ms: u64,
    component_id: String,
    kind: String,
    message: String,
    decision_id: Option<Uuid>,
    plan_version: Option<String>,
}
#[derive(Default)]
struct State {
    sequence: u64,
    producer_active: bool,
    had_runtime: bool,
    statuses: BTreeMap<String, Status>,
    events: VecDeque<Event>,
    receipt: Option<serde_json::Value>,
    writes: VecDeque<serde_json::Value>,
    observation: Option<RuntimeObservation>,
}
pub struct Hub {
    state: Mutex<State>,
    changed: watch::Sender<u64>,
}
impl Default for Hub {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            changed: watch::channel(0).0,
        }
    }
}
impl Hub {
    fn begin_runtime(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?;
        anyhow::ensure!(
            !state.producer_active,
            "a runtime status producer is already active"
        );
        let sequence = Self::advance(&mut state)?;
        if state.had_runtime {
            *state = State {
                sequence,
                producer_active: true,
                had_runtime: true,
                ..Default::default()
            };
        } else {
            state.producer_active = true;
            state.had_runtime = true;
        }
        self.changed.send_replace(sequence);
        Ok(())
    }

    fn end_runtime(&self) -> Result<()> {
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?
            .producer_active = false;
        Ok(())
    }

    pub fn observe_runtime(&self, observation: RuntimeObservation) -> Result<()> {
        anyhow::ensure!(
            observation.sequence > 0,
            "runtime observation sequence must be positive"
        );
        anyhow::ensure!(
            gpu_contracts::fixtures::NAMES.contains(&observation.scenario.as_str()),
            "unknown starting scenario"
        );
        gpu_contracts::nonempty(&observation.detail)?;
        anyhow::ensure!(
            observation.components.len() <= 128 && observation.required_components.len() <= 128,
            "runtime component limit exceeded"
        );
        for id in &observation.required_components {
            gpu_contracts::nonempty(id)?;
        }
        let mut ids = BTreeSet::new();
        for component in &observation.components {
            gpu_contracts::nonempty(&component.component_id)?;
            gpu_contracts::nonempty(&component.status)?;
            anyhow::ensure!(
                ids.insert(&component.component_id),
                "duplicate runtime component"
            );
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?;
        if let Some(previous) = &state.observation {
            if previous == &observation {
                return Ok(());
            }
            anyhow::ensure!(
                observation.sequence > previous.sequence,
                "stale runtime observation"
            );
        }
        state.observation = Some(observation);
        Self::advance(&mut state)?;
        self.changed.send_replace(state.sequence);
        Ok(())
    }

    fn push_event(state: &mut State, event: Event) {
        state.events.push_back(event);
        while state.events.len() > 128 {
            state.events.pop_front();
        }
    }
    pub fn receipt(&self, value: &gpu_control::writer::CommitReceipt) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?;
        state.receipt = Some(serde_json::to_value(value)?);
        state.sequence = state
            .sequence
            .checked_add(1)
            .context("status sequence exhausted")?;
        self.changed.send_replace(state.sequence);
        Ok(())
    }
    pub fn write_outcome(
        &self,
        candidate: &gpu_contracts::Candidate,
        outcome: &str,
        detail: &str,
    ) -> Result<()> {
        let epoch = Self::candidate_epoch(candidate)?;
        let value = json!({
            "decision_id":candidate.decision_id, "observation_epoch":epoch,
            "outcome":outcome, "detail":detail
        });
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?;
        state
            .writes
            .retain(|old| old["decision_id"] != value["decision_id"]);
        state.writes.push_back(value);
        while state.writes.len() > 64 {
            state.writes.pop_front();
        }
        Self::advance(&mut state)?;
        self.changed.send_replace(state.sequence);
        Ok(())
    }
    fn advance(state: &mut State) -> Result<u64> {
        state.sequence = state
            .sequence
            .checked_add(1)
            .context("status sequence exhausted")?;
        Ok(state.sequence)
    }
    pub fn event(
        &self,
        epoch: Uuid,
        id: &str,
        kind: &str,
        message: String,
        decision_id: Option<Uuid>,
        plan_version: Option<String>,
    ) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?;
        let sequence = Self::advance(&mut state)?;
        Self::push_event(
            &mut state,
            Event {
                event_id: format!("{epoch}/{sequence}"),
                observation_epoch: epoch,
                event_sequence: sequence,
                time_ms: crate::wire::utc_millis()?,
                component_id: id.into(),
                kind: kind.into(),
                message,
                decision_id,
                plan_version,
            },
        );
        self.changed.send_replace(sequence);
        Ok(())
    }
    pub fn publish(
        &self,
        id: &str,
        status: &str,
        error: Option<String>,
        epoch: Option<Uuid>,
    ) -> Result<()> {
        let status = Status {
            schema_version: 1,
            component_id: id.into(),
            status: status.into(),
            error,
            observation_epoch: epoch,
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?;
        if state.statuses.get(id) == Some(&status) {
            return Ok(());
        }
        let sequence = Self::advance(&mut state)?;
        if let Some(epoch) = epoch {
            Self::push_event(
                &mut state,
                Event {
                    event_id: format!("{epoch}/{sequence}"),
                    observation_epoch: epoch,
                    event_sequence: sequence,
                    time_ms: crate::wire::utc_millis()?,
                    component_id: id.into(),
                    kind: "component-status".into(),
                    message: match &status.error {
                        Some(error) => format!("{id}: {}. {error}", status.status),
                        None => format!("{id}: {}", status.status),
                    },
                    decision_id: None,
                    plan_version: None,
                },
            );
        }
        state.statuses.insert(id.into(), status);
        self.changed.send_replace(sequence);
        Ok(())
    }

    pub fn candidate_epoch(candidate: &gpu_contracts::Candidate) -> Result<Uuid> {
        serde_json::from_value(
            candidate
                .decision_details
                .get("observation_epoch")
                .context("candidate observation epoch missing")?
                .clone(),
        )
        .context("invalid candidate observation epoch")
    }
}
pub struct Producer {
    descriptor: ComponentDescriptor,
    configuration: serde_json::Value,
    hub: Arc<Hub>,
    changed: watch::Receiver<u64>,
    emitter: Emitter,
    observed: Option<u64>,
    running: bool,
    stopped: bool,
    queued: VecDeque<OutputEnvelope>,
}
impl Producer {
    pub fn new(
        descriptor: ComponentDescriptor,
        configuration: serde_json::Value,
        hub: Arc<Hub>,
    ) -> Result<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Config {
            stream: StreamId,
        }
        let config: Config = serde_json::from_value(configuration.clone())?;
        Ok(Self {
            descriptor,
            configuration,
            changed: hub.changed.subscribe(),
            hub,
            emitter: Emitter::new(config.stream),
            observed: None,
            running: false,
            stopped: false,
            queued: VecDeque::new(),
        })
    }
}
#[async_trait]
impl ComputationComponent for Producer {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<serde_json::Value> {
        Ok(self.configuration.clone())
    }
    async fn start(&mut self) -> Result<()> {
        anyhow::ensure!(!self.running, "status producer already running");
        anyhow::ensure!(
            !self.stopped,
            "stopped status producer requires fresh factory construction"
        );
        self.hub.begin_runtime()?;
        self.running = true;
        self.observed = None;
        self.hub
            .publish(self.descriptor.id().as_str(), "running", None, None)
    }
    async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        self.running = false;
        self.stopped = true;
        let published = self
            .hub
            .publish(self.descriptor.id().as_str(), "stopped", None, None);
        let released = self.hub.end_runtime();
        published.and(released)
    }
}
#[async_trait]
impl EnvelopeSource for Producer {
    async fn next(&mut self) -> Result<Option<OutputEnvelope>> {
        anyhow::ensure!(self.running, "status producer is stopped");
        loop {
            if let Some(output) = self.queued.pop_front() {
                return Ok(Some(output));
            }
            self.changed.borrow_and_update();
            let snapshot = {
                let state = self
                    .hub
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("status hub lock poisoned"))?;
                if self.observed == Some(state.sequence) {
                    None
                } else {
                    Some((
                        state.sequence,
                        state.statuses.values().cloned().collect::<Vec<_>>(),
                        state.events.clone(),
                        state.receipt.clone(),
                        state.writes.clone(),
                        state.observation.clone(),
                    ))
                }
            };
            if let Some((sequence, statuses, events, receipt, writes, observation)) = snapshot {
                let mut changes = Vec::new();
                let current_statuses: Vec<_> = statuses
                    .iter()
                    .filter(|status| {
                        observation.as_ref().is_none_or(|observation| {
                            status
                                .observation_epoch
                                .is_none_or(|epoch| epoch == observation.observation_epoch)
                        })
                    })
                    .collect();
                let mut components: BTreeMap<_, _> = current_statuses
                    .iter()
                    .map(|status| {
                        (
                            status.component_id.clone(),
                            RuntimeComponent {
                                component_id: status.component_id.clone(),
                                status: status.status.clone(),
                                error: status.error.clone(),
                            },
                        )
                    })
                    .collect();
                if let Some(observation) = &observation {
                    for component in &observation.components {
                        // Host failures outrank older plugin-local diagnostics.
                        if component.status != "running"
                            || component.error.is_some()
                            || !components.contains_key(&component.component_id)
                        {
                            components.insert(component.component_id.clone(), component.clone());
                        }
                    }
                }
                let inputs_ready = observation.as_ref().is_some_and(|observation| {
                    observation.inputs_ready()
                        && !current_statuses.iter().any(|status| {
                            observation
                                .required_components
                                .contains(&status.component_id)
                                && matches!(
                                    status.status.as_str(),
                                    "stopped"
                                        | "unavailable"
                                        | "initialization-error"
                                        | "awaiting-source-bootstrap"
                                        | "awaiting-source-reauthorization"
                                        | "awaiting-policy-bootstrap"
                                )
                        })
                });
                let ready = inputs_ready
                    && observation
                        .as_ref()
                        .is_some_and(|observation| observation.query_results_current);
                let readiness = json!({
                    "fleet_id":"demo",
                    "observation_sequence":observation.as_ref().map(|o| o.sequence.to_string()),
                    "source_bootstrap_complete":observation.as_ref().map(|o| o.source_bootstrap_complete),
                    "query_bootstrap_complete":observation.as_ref().map(|o| o.query_bootstrap_complete),
                    "observation_epoch":observation.as_ref().map(|o| o.observation_epoch.to_string()).unwrap_or_default(),
                    "scheduling_signature":observation.as_ref().map(|o| o.scheduling_signature.as_str()).unwrap_or_default(),
                    "policy_signature":observation.as_ref().map(|o| o.policy_signature.as_str()).unwrap_or_default(),
                    "scenario":observation.as_ref().map(|o| o.scenario.as_str()).unwrap_or("unknown"),
                    "scenario_ready":ready,
                    "inputs_ready":inputs_ready,
                    "state":if ready { "ready" } else if observation.as_ref().is_some_and(|o| o.reset_in_progress) { "resetting" } else { "not-ready" },
                    "detail":observation.as_ref().map(|o| o.detail.as_str()).unwrap_or("No lifecycle observation: source and query bootstrap readiness are unknown."),
                    "components":components.into_values().collect::<Vec<_>>(),
                });
                changes.extend(self.emitter.record("DemoReadiness", "demo", &readiness)?);
                for status in statuses {
                    if let Some(change) =
                        self.emitter
                            .record("RuntimeStatus", &status.component_id, &status)?
                    {
                        changes.push(change);
                    }
                }
                let keys = events
                    .iter()
                    .map(|event| event.event_id.clone())
                    .collect::<BTreeSet<_>>();
                changes.extend(self.emitter.retain("DemoEvent", &keys)?);
                for event in events {
                    if let Some(change) =
                        self.emitter.record("DemoEvent", &event.event_id, &event)?
                    {
                        changes.push(change);
                    }
                }
                let keys = writes
                    .iter()
                    .map(|write| {
                        write["decision_id"]
                            .as_str()
                            .map(str::to_owned)
                            .context("write decision identity missing")
                    })
                    .collect::<Result<BTreeSet<_>>>()?;
                changes.extend(self.emitter.retain("PlanWriteOutcome", &keys)?);
                for write in writes {
                    let key = write["decision_id"]
                        .as_str()
                        .context("write decision identity missing")?;
                    if let Some(change) = self.emitter.record("PlanWriteOutcome", key, &write)? {
                        changes.push(change);
                    }
                }
                if let Some(receipt) = receipt {
                    if let Some(change) =
                        self.emitter.record("PlanWriteReceipt", "demo", &receipt)?
                    {
                        changes.push(change);
                    }
                }
                self.observed = Some(sequence);
                self.queued.extend(self.emitter.emit(changes, None)?);
            } else {
                self.changed.changed().await.context("status hub closed")?;
            }
        }
    }
}
