use anyhow::{Context, Result};
use drasi_core::models::{
    Element, ElementMetadata, ElementPropertyMap, ElementReference, SourceChange,
};
use drasi_lib::computation::v1::*;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

pub type Records = BTreeMap<(String, String), Value>;

pub fn metadata(stream: &str, label: &str, id: &str, effective_from: u64) -> ElementMetadata {
    ElementMetadata {
        reference: ElementReference::new(stream, &format!("{label}/{id}")),
        labels: Arc::from([Arc::from(label)]),
        effective_from,
    }
}

pub fn diff(stream: &str, before: &Records, after: &Records, revision: u64) -> Vec<SourceChange> {
    let mut changes = Vec::new();
    for (label, id) in before.keys().filter(|key| !after.contains_key(*key)) {
        changes.push(SourceChange::Delete {
            metadata: metadata(stream, label, id, revision),
        });
    }
    for ((label, id), value) in after {
        if before.get(&(label.clone(), id.clone())) == Some(value) {
            continue;
        }
        let element = Element::Node {
            metadata: metadata(stream, label, id, revision),
            properties: ElementPropertyMap::from(value.clone()),
        };
        changes.push(if before.contains_key(&(label.clone(), id.clone())) {
            SourceChange::Update { element }
        } else {
            SourceChange::Insert { element }
        });
    }
    changes
}

pub struct Emitter {
    stream: StreamId,
    sequence: u64,
    records: Records,
}
impl Emitter {
    pub fn new(stream: &str) -> Result<Self> {
        Ok(Self {
            stream: StreamId::try_new(stream)?,
            sequence: 0,
            records: Records::new(),
        })
    }
    pub fn replace(
        &mut self,
        records: Records,
        input: Option<&ChangeEnvelope>,
    ) -> Result<Vec<OutputEnvelope>> {
        let next = self
            .sequence
            .checked_add(1)
            .context("output sequence exhausted")?;
        let now = chrono::Utc::now();
        let effective_from = u64::try_from(now.timestamp_millis())?;
        let changes = diff(
            self.stream.as_str(),
            &self.records,
            &records,
            effective_from,
        );
        if changes.is_empty() {
            return Ok(vec![]);
        }
        let envelope = if let Some(input) = input {
            GraphChangeCodec::derive_changes(input, &changes, self.stream.clone(), next)?
        } else {
            GraphChangeCodec::encode_changes(&changes, self.stream.clone(), next, Some(now))?
        };
        self.records = records;
        self.sequence = next;
        Ok(vec![OutputEnvelope {
            port: PortId::try_new("out")?,
            envelope,
        }])
    }
}

pub fn port(name: &str, direction: PortDirection, query: bool) -> Result<PortDescriptor> {
    Ok(PortDescriptor::new(
        PortId::try_new(name)?,
        direction,
        if query {
            QueryChangeCodec::schema()
        } else {
            GraphChangeCodec::schema()
        }
        .descriptor()
        .clone(),
        PipeRequirements::default(),
    ))
}
pub fn endpoint(component: &str, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(
        ComponentId::try_new(component)?,
        PortId::try_new(port)?,
    ))
}
