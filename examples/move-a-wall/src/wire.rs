use anyhow::{Context, Result};
use drasi_computation_contracts::*;
use drasi_computation_plugin_sdk::wire::Scope;
use drasi_core_types::models::{
    Element, ElementMetadata, ElementPropertyMap, ElementReference, SourceChange,
};
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
            SourceChange::Replace { element }
        } else {
            SourceChange::Insert { element }
        });
    }
    changes
}

pub struct Emitter {
    stream: StreamId,
    identity: GraphProducerIdentity,
    sequence: u64,
    records: Records,
}
impl Emitter {
    pub fn new(stream: &str, component: ComponentId, scope: Option<&Scope>) -> Result<Self> {
        let stream = StreamId::try_new(stream)?;
        let identity = GraphProducerIdentity::volatile(
            scope
                .map_or("volatile", |scope| scope.instance_id.as_str())
                .into(),
            scope
                .map_or("volatile", |scope| scope.graph_id.as_str())
                .into(),
            component,
            stream.clone(),
        )?;
        Ok(Self {
            stream,
            identity,
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
        let mut envelope = if let Some(input) = input {
            GraphChangeCodec::derive_changes(input, &changes, self.stream.clone(), next)?
        } else {
            GraphChangeCodec::encode_changes(&changes, self.stream.clone(), next, Some(now))?
        };
        GraphProducerProgress::annotate(&mut envelope, &self.identity, next)?;
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

#[cfg(test)]
mod replacement_tests {
    use super::*;

    #[test]
    fn complete_views_replace_fields_instead_of_leaving_omitted_properties() {
        let key = ("Room".to_string(), "one".to_string());
        let before = Records::from([(key.clone(), serde_json::json!({"kept":1,"removed":2}))]);
        let after = Records::from([(key, serde_json::json!({"kept":1}))]);
        let changes = diff("rooms/out", &before, &after, 2);
        let [SourceChange::Replace { element }] = changes.as_slice() else {
            panic!("a changed complete view must produce one full replacement");
        };
        assert!(element.get_properties().get("removed").is_none());
        assert_eq!(element.get_effective_from(), 2);
    }
}
