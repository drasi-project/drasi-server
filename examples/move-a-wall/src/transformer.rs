use crate::{
    geometry,
    model::{Scene, Shape},
    source::InputRecord,
    wire::{self, Emitter, Records},
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use drasi_lib::computation::v1::*;
use serde_json::json;
use std::collections::BTreeMap;

pub struct Geometry {
    descriptor: ComponentDescriptor,
    rows: Vec<(RecordId, Vec<InputRecord>)>,
    scene: Scene,
    emitter: Emitter,
    sequence: Option<(u64, u64)>,
    revision: u64,
    stopped: bool,
}
impl Geometry {
    pub fn new(id: ComponentId) -> Result<Self> {
        Ok(Self {
            descriptor: descriptor(id)?,
            rows: vec![],
            scene: Scene::new(),
            emitter: Emitter::new("geometry/out")?,
            sequence: None,
            revision: 0,
            stopped: false,
        })
    }
    fn recompute(&mut self) -> Result<Option<Records>> {
        let mut scene = Scene::new();
        let mut versions = BTreeMap::new();
        let mut clock = None;
        for (_, records) in &self.rows {
            for record in records {
                match record {
                    InputRecord::Entity { entity, revision } => {
                        ensure!(entity.active, "context query passed an inactive entity");
                        entity.validate()?;
                        ensure!(
                            scene.insert(entity.id.clone(), entity.clone()).is_none(),
                            "duplicate context entity"
                        );
                        versions.insert(entity.id.clone(), *revision);
                    }
                    InputRecord::Clock {
                        revision, members, ..
                    } => {
                        ensure!(clock.is_none(), "duplicate scene clock");
                        clock = Some((*revision, members));
                    }
                }
            }
        }
        let Some((revision, members)) = clock else {
            ensure!(scene.is_empty(), "nonempty context has no source revision");
            self.scene.clear();
            return Ok(Some(Records::new()));
        };
        // The source's explicit membership/version manifest, not a quiet period,
        // proves that this CQ row includes the entire accepted input revision.
        if revision < self.revision || &versions != members {
            return Ok(None);
        }
        let obstructions = geometry::calculate(&scene)?;
        let mut output = Records::new();
        for entity in scene.values() {
            let mut properties = json!({"id":entity.id, "name":entity.name});
            match &entity.shape {
                Shape::Cart { .. } => {
                    properties["cart_id"] = json!(entity.id);
                }
                Shape::Journey {
                    cart_id,
                    destination_id,
                    ..
                } => {
                    properties["journey_id"] = json!(entity.id);
                    properties["cart_id"] = json!(cart_id);
                    properties["destination_id"] = json!(destination_id);
                }
                Shape::Destination { .. } => {
                    properties["destination_id"] = json!(entity.id);
                }
                Shape::Obstacle { .. } => {
                    properties["obstacle_id"] = json!(entity.id);
                }
            }
            output.insert((entity.label().into(), entity.id.clone()), properties);
        }
        for (id, obstruction) in &obstructions {
            output.insert(
                ("Obstruction".into(), id.clone()),
                serde_json::to_value(obstruction)?,
            );
        }
        output.insert(("GeometryStatus".into(), "current".into()), json!({
            "id":"current", "revision": revision, "objects": scene.len(), "obstructions": obstructions.len(),
        }));
        self.scene = scene;
        self.revision = revision;
        Ok(Some(output))
    }
}
fn descriptor(id: ComponentId) -> Result<ComponentDescriptor> {
    Ok(ComponentDescriptor::try_new(
        id,
        vec![
            wire::port("in", PortDirection::Input, true)?,
            wire::port("out", PortDirection::Output, false)?,
        ],
    )?)
}
#[async_trait]
impl ComputationComponent for Geometry {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<serde_json::Value> {
        Ok(json!({}))
    }
    async fn start(&mut self) -> Result<()> {
        ensure!(
            !self.stopped,
            "geometry restart requires reconstruction and a fresh source bootstrap"
        );
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        self.stopped = true;
        Ok(())
    }
}
#[async_trait]
impl Transformer for Geometry {
    async fn transform(&mut self, input: InputEnvelope) -> Result<Vec<OutputEnvelope>> {
        ensure!(!self.stopped, "geometry transformer stopped");
        ensure!(input.port.as_str() == "in", "unknown geometry port");
        ensure!(
            QueryChangeCodec::metadata(&input.envelope)?.query_id == "geometry-context",
            "unexpected context query"
        );
        let position = (
            QueryChangeCodec::query_generation(&input.envelope)?,
            QueryChangeCodec::query_sequence(&input.envelope)?,
        );
        if self.sequence.is_some_and(|last| position <= last) {
            return Ok(vec![]);
        }
        if self.sequence.is_some_and(|last| position.0 > last.0) {
            ensure!(
                QueryChangeCodec::is_snapshot(&input.envelope),
                "new query generation requires an explicit snapshot"
            );
        }
        if QueryChangeCodec::is_snapshot(&input.envelope) {
            self.rows.clear();
        }
        for operation in input.envelope.changes().operations() {
            match operation {
                ChangeOperation::Added { after, .. } | ChangeOperation::Updated { after, .. } => {
                    let row = QueryChangeCodec::decode_row(after)?;
                    let value = QueryChangeCodec::row_values_to_json(&row.values);
                    let payloads = value["objects"]
                        .as_array()
                        .context("context query requires objects array")?;
                    let records = payloads
                        .iter()
                        .map(|p| -> Result<_> {
                            Ok(serde_json::from_str::<InputRecord>(
                                p.as_str().context("context object must be JSON string")?,
                            )?)
                        })
                        .collect::<Result<Vec<_>>>()?;
                    self.rows.retain(|(id, _)| id != after.identity());
                    self.rows.push((after.identity().clone(), records));
                }
                ChangeOperation::Deleted { identity, .. } => {
                    self.rows.retain(|(id, _)| id != identity.identity());
                }
            }
        }
        let output = match self.recompute()? {
            Some(records) => self.emitter.replace(records, Some(&input.envelope))?,
            None => vec![],
        };
        self.sequence = Some(position);
        Ok(output)
    }
}
