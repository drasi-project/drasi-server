use anyhow::{ensure, Context, Result};
use drasi_core::models::{
    Element, ElementMetadata, ElementPropertyMap, ElementReference, ElementValue, SourceChange,
};
use drasi_lib::computation::v1::{
    ChangeEnvelope, ChangeOperation, GraphChangeCodec, InputEnvelope, OutputEnvelope, PortId,
    QueryChangeCodec, StreamId,
};
use gpu_contracts::{Capacity, Configuration, Plan, Settings};
use gpu_policy::Assessment;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolveInput {
    pub configuration: Configuration,
    pub policy: Assessment,
    pub capacities: BTreeMap<Uuid, Capacity>,
    pub plan: Plan,
}
impl SolveInput {
    pub fn signature(&self) -> Result<String> {
        gpu_placement::signature(&self.configuration, &self.policy, &self.capacities)
    }
    pub fn validate(&self) -> Result<()> {
        self.configuration.validate()?;
        self.policy.validate_current(&self.configuration)?;
        ensure!(
            self.capacities.len() == self.configuration.gpus.len(),
            "incomplete capacity snapshot"
        );
        ensure!(
            self.capacities
                .iter()
                .all(|(id, c)| id == &c.gpu_id && self.configuration.gpus.contains_key(id)),
            "invalid capacity identities"
        );
        ensure!(self.plan.fleet_id == "demo", "unexpected fleet");
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Message {
    Bootstrap {
        epoch: Uuid,
        commit: String,
        configuration: Configuration,
        settings: BTreeMap<Uuid, Settings>,
        plan: Plan,
    },
    Configuration {
        epoch: Uuid,
        commit: String,
        configuration: Configuration,
        settings: BTreeMap<Uuid, Settings>,
    },
    Policy {
        epoch: Uuid,
        config_fingerprint: String,
        assessment: Assessment,
    },
    Allocation {
        epoch: Uuid,
        plan: Plan,
    },
    Schedule {
        epoch: Uuid,
        input: SolveInput,
    },
    Unavailable {
        epoch: Uuid,
        reason: String,
    },
}
impl Message {
    pub fn epoch(&self) -> Uuid {
        match self {
            Self::Bootstrap { epoch, .. }
            | Self::Configuration { epoch, .. }
            | Self::Policy { epoch, .. }
            | Self::Allocation { epoch, .. }
            | Self::Schedule { epoch, .. }
            | Self::Unavailable { epoch, .. } => *epoch,
        }
    }
}

pub fn decode(input: &InputEnvelope) -> Result<Vec<Message>> {
    ensure!(input.port.as_str() == "in", "unexpected input port");
    let mut messages = Vec::new();
    for operation in input.envelope.changes().operations() {
        let row = match operation {
            ChangeOperation::Added { after, .. } | ChangeOperation::Updated { after, .. } => after,
            ChangeOperation::Deleted { .. } => {
                anyhow::bail!("input deletion requires an explicit fail-closed Unavailable message")
            }
        };
        let row = QueryChangeCodec::decode_row(row)?;
        let payload = row
            .values
            .get("payload")
            .and_then(|v| v.as_str())
            .context("query row requires a string payload")?;
        ensure!(payload.len() <= 256 * 1024, "input payload exceeds bound");
        messages.push(serde_json::from_str(payload)?);
    }
    Ok(messages)
}

pub struct Emitter {
    pub stream: StreamId,
    sequence: u64,
    rows: BTreeMap<String, serde_json::Value>,
}
impl Emitter {
    pub fn new(stream: StreamId) -> Self {
        Self {
            stream,
            sequence: 0,
            rows: BTreeMap::new(),
        }
    }
    pub fn record<T: Serialize>(
        &mut self,
        label: &str,
        key: &str,
        value: &T,
    ) -> Result<Option<SourceChange>> {
        let value = serde_json::to_value(value)?;
        let identity = format!("{label}/{key}");
        if self.rows.get(&identity) == Some(&value) {
            return Ok(None);
        }
        let existed = self.rows.contains_key(&identity);
        let mut properties = ElementPropertyMap::new();
        properties.insert("id", ElementValue::String(key.into()));
        if key == "demo" {
            properties.insert("fleet_id", ElementValue::String("demo".into()));
        }
        properties.insert(
            "payload",
            ElementValue::String(serde_json::to_string(&value)?.into()),
        );
        // Query fields retain typed values; payload also carries the lossless shared Rust contract.
        if let Some(object) = value.as_object() {
            for (key, value) in object {
                validate_graph_numbers(value)?;
                properties.insert(key.as_str(), ElementValue::from(value));
            }
        }
        let element = Element::Node {
            metadata: metadata(&self.stream, &identity, label)?,
            properties,
        };
        self.rows.insert(identity, value);
        Ok(Some(if existed {
            SourceChange::Update { element }
        } else {
            SourceChange::Insert { element }
        }))
    }
    pub fn remove(&mut self, label: &str, key: &str) -> Result<Option<SourceChange>> {
        let identity = format!("{label}/{key}");
        if self.rows.remove(&identity).is_none() {
            return Ok(None);
        }
        Ok(Some(SourceChange::Delete {
            metadata: metadata(&self.stream, &identity, label)?,
        }))
    }
    pub fn retain(
        &mut self,
        label: &str,
        keys: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<SourceChange>> {
        let prefix = format!("{label}/");
        let removed = self
            .rows
            .keys()
            .filter_map(|key| key.strip_prefix(&prefix))
            .filter(|key| !keys.contains(*key))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut changes = Vec::new();
        for key in removed {
            if let Some(change) = self.remove(label, &key)? {
                changes.push(change);
            }
        }
        Ok(changes)
    }
    pub fn emit(
        &mut self,
        changes: Vec<SourceChange>,
        input: Option<&ChangeEnvelope>,
    ) -> Result<Vec<OutputEnvelope>> {
        if changes.is_empty() {
            return Ok(vec![]);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("native stream sequence exhausted")?;
        let envelope = match input {
            Some(input) => GraphChangeCodec::derive_changes(
                input,
                &changes,
                self.stream.clone(),
                self.sequence,
            )?,
            None => GraphChangeCodec::encode_changes(
                &changes,
                self.stream.clone(),
                self.sequence,
                Some(chrono::Utc::now()),
            )?,
        };
        Ok(vec![OutputEnvelope {
            port: PortId::try_new("out")?,
            envelope,
        }])
    }
}

fn validate_graph_numbers(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Number(number) => ensure!(
            !number.is_u64() || number.as_i64().is_some(),
            "graph integer exceeds signed 64-bit range; encode counters as decimal strings"
        ),
        serde_json::Value::Array(values) => {
            for value in values {
                validate_graph_numbers(value)?;
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                validate_graph_numbers(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn metadata(stream: &StreamId, identity: &str, label: &str) -> Result<ElementMetadata> {
    Ok(ElementMetadata {
        reference: ElementReference::new(stream.as_str(), identity),
        labels: Arc::from([Arc::from(label)]),
        effective_from: utc_millis()?,
    })
}

// Legacy effective_from and drasi.changeDateTime use milliseconds, not microseconds.
pub fn utc_millis() -> Result<u64> {
    Ok(u64::try_from(chrono::Utc::now().timestamp_millis())?)
}
