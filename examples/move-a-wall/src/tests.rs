use super::*;
use anyhow::{Context, Result};
use drasi_computation_plugin_sdk::{
    Component, ControlSender, CreateRequest, Factory, NativeComponent, NativeSource,
    NativeTransformer,
};
use drasi_computation_transformer_geometry::{GeometryFactory, GEOMETRY};
use drasi_core::{computation::InMemoryComputationProvider, models::SourceChange};
use drasi_lib::computation::v1::*;
use model::{Entity, Shape};
use source::{Command, SceneSource, Store};
use std::{num::NonZeroUsize, sync::Arc};

fn scope() -> OperationScope {
    OperationScope::new(
        "test",
        "component",
        1,
        None,
        tokio_util::sync::CancellationToken::new(),
    )
}
/// A stable UUID standing in for a named component outside a graph.
pub(crate) fn test_id(name: &str) -> String {
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("00000000-0000-0000-0000-{:012x}", hash & 0xffff_ffff_ffff)
}
pub(crate) fn native_start() -> drasi_computation_plugin_sdk::lifecycle::StartContext {
    drasi_computation_plugin_sdk::lifecycle::StartContext::new(scope(), None)
}
pub(crate) fn native_stop() -> drasi_computation_plugin_sdk::lifecycle::StopContext {
    drasi_computation_plugin_sdk::lifecycle::StopContext::new(scope(), StopMode::Drain)
}

fn component(name: &str) -> Result<serde_json::Value> {
    let configuration: serde_json::Value = serde_yaml::from_str(include_str!("../server.yaml"))?;
    let instance = configuration["instances"]
        .as_array()
        .context("configured instances")?
        .iter()
        .find(|instance| instance["id"].as_str() == Some("move-a-wall"))
        .context("move-a-wall instance")?;
    instance["computation"]["graph"]["components"]
        .as_array()
        .context("static graph components")?
        .iter()
        .find(|component| component["name"].as_str() == Some(name))
        .cloned()
        .with_context(|| format!("static component '{name}'"))
}
async fn query(id: &str) -> Result<ContinuousQueryTransformer> {
    let component = component(id)?;
    let config = component.get("config").context("static query config")?;
    let text = config["query"].as_str().context("static query text")?;
    let capacity = config["outbox_capacity"]
        .as_u64()
        .and_then(|capacity| usize::try_from(capacity).ok())
        .and_then(NonZeroUsize::new)
        .context("static query outbox capacity")?;
    let execution = config
        .get("execution")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?
        .unwrap_or_default();
    let mut q = ContinuousQueryTransformer::new_configured(
        ContinuousQueryDefinition {
            graph_id: "wall-test".into(),
            id: ComponentId::try_new(test_id(id))?,
            query: text.into(),
            language: ComputationQueryLanguage::Cypher,
            output_stream: StreamId::try_new(format!("{id}/out"))?,
            outbox_capacity: capacity,
        },
        Arc::new(InMemoryComputationProvider),
        QueryOptions::default(),
        execution,
        None,
    )
    .await?;
    q.start(&StartContext::new(scope())).await?;
    Ok(q)
}
/// The standard geometry transformer, configured exactly as in server.yaml.
async fn geometry(
    id: &str,
    scope: Option<drasi_computation_plugin_sdk::wire::Scope>,
) -> Result<Box<dyn NativeTransformer>> {
    let component = component("geometry")?;
    assert_eq!(component["kind"], GEOMETRY);
    let created = GeometryFactory.create(
        &CreateRequest {
            id: ComponentId::try_new(test_id(id))?,
            implementation: ImplementationIdentity::try_new(GEOMETRY, "1")?,
            configuration_version: 1,
            configuration: component["config"].clone(),
            scope,
            inputs: vec![],
            activation_credentials_bound: false,
            update_directive: None,
        },
        ControlSender::default(),
    )?;
    let Component::Transformer(mut geometry) = created.component else {
        anyhow::bail!("geometry is a transformer");
    };
    geometry.start(&mut native_start()).await?;
    Ok(geometry)
}
fn input(output: &OutputEnvelope) -> Result<InputEnvelope> {
    Ok(InputEnvelope {
        port: PortId::try_new("in")?,
        envelope: output.envelope.clone(),
    })
}
async fn drive(
    query: &mut ContinuousQueryTransformer,
    output: &OutputEnvelope,
) -> Result<Vec<OutputEnvelope>> {
    let mut outputs = query.transform(input(output)?).await?;
    query.delivery_completed(&outputs).await?;
    while query.has_pending_emissions() {
        let more = query.continue_transform().await?;
        query.delivery_completed(&more).await?;
        outputs.extend(more);
    }
    Ok(outputs)
}

/// One command's journey through the demo graph.
struct Step {
    source: OutputEnvelope,
    footprints: Vec<OutputEnvelope>,
    geometry: Vec<OutputEnvelope>,
}
/// The demo graph with real queries and the real standard geometry component:
/// scene -> journey-footprints + obstacle-footprints -> geometry, and
/// scene + geometry -> affected-journeys.
struct Harness {
    store: Store,
    source: SceneSource,
    journeys: ContinuousQueryTransformer,
    obstacles: ContinuousQueryTransformer,
    geometry: Box<dyn NativeTransformer>,
    impact: ContinuousQueryTransformer,
}
impl Harness {
    async fn new() -> Result<Self> {
        Self::with_geometry(geometry("geometry", None).await?).await
    }
    async fn with_geometry(geometry: Box<dyn NativeTransformer>) -> Result<Self> {
        let (store, mut source) = source::channel()?;
        source.start(&mut native_start()).await?;
        Ok(Self {
            store,
            source,
            geometry,
            journeys: query("journey-footprints").await?,
            obstacles: query("obstacle-footprints").await?,
            impact: query("affected-journeys").await?,
        })
    }
    async fn send(&mut self, command: Command) -> Result<Step> {
        self.store.command(self.store.revision, command).await?;
        let source = self.source.next().await?.context("source output")?;
        drive(&mut self.impact, &source).await?;
        let mut footprints = drive(&mut self.journeys, &source).await?;
        footprints.extend(drive(&mut self.obstacles, &source).await?);
        let mut outputs = vec![];
        for footprint in &footprints {
            let next = self.geometry.transform(input(footprint)?).await?;
            for output in &next {
                drive(&mut self.impact, output).await?;
            }
            outputs.extend(next);
        }
        Ok(Step {
            source,
            footprints,
            geometry: outputs,
        })
    }
    fn rows(&self) -> Result<Vec<serde_json::Value>> {
        rows(&self.impact)
    }
    async fn close(mut self) -> Result<()> {
        self.source.stop(&native_stop()).await?;
        self.geometry.stop(&native_stop()).await?;
        for query in [&mut self.journeys, &mut self.obstacles, &mut self.impact] {
            query
                .stop(&StopContext::new(scope(), StopMode::Drain))
                .await?;
        }
        Ok(())
    }
}
fn rows(query: &ContinuousQueryTransformer) -> Result<Vec<serde_json::Value>> {
    query
        .results()
        .snapshot()?
        .rows
        .values()
        .map(|r| {
            Ok(QueryChangeCodec::row_values_to_json(
                &QueryChangeCodec::decode_row(r)?.values,
            ))
        })
        .collect()
}
fn wall(y: f64) -> Entity {
    let mut wall = model::fixture()["wall"].clone();
    wall.shape = Shape::Obstacle {
        vertices: vec![[10., y], [13., y], [13., y + 1.], [10., y + 1.]],
    };
    wall
}
fn changes(outputs: &[OutputEnvelope]) -> Result<Vec<SourceChange>> {
    let mut result = vec![];
    for output in outputs {
        result.extend(GraphChangeCodec::decode_changes(&output.envelope)?);
    }
    Ok(result
        .into_iter()
        .filter(|c| {
            c.get_reference()
                .element_id
                .starts_with(r#"["relation","obstruction","#)
        })
        .collect())
}
fn nodes(output: &OutputEnvelope) -> Result<Vec<drasi_core::models::Element>> {
    Ok(GraphChangeCodec::decode_changes(&output.envelope)?
        .into_iter()
        .filter_map(|change| match change {
            SourceChange::Insert { element }
            | SourceChange::Replace { element }
            | SourceChange::Update { element, .. } => Some(element),
            _ => None,
        })
        .collect())
}
#[tokio::test]
async fn source_publishes_typed_plain_nodes_and_status() -> Result<()> {
    for entity in model::fixture().values() {
        let value = serde_json::to_value(entity)?;
        let mut keys: Vec<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["active", "id", "name", "shape"]);
        let mut obsolete = value;
        obsolete["floor"] = serde_json::json!("ground");
        assert!(serde_json::from_value::<Entity>(obsolete).is_err());
    }
    let mut h = Harness::new().await?;
    let step = h.send(Command::Reset).await?;
    let nodes = nodes(&step.source)?;
    assert_eq!(nodes.len(), model::fixture().len() + 1);
    for node in &nodes {
        let metadata = node.get_metadata();
        let labels: Vec<_> = metadata.labels.iter().map(|l| l.to_string()).collect();
        let properties = serde_json::Value::Object(node.get_properties().into());
        assert!(properties.get("payload").is_none());
        assert!(properties.get("floor").is_none());
        if labels == ["SceneStatus"] {
            assert_eq!(
                metadata.reference.element_id.as_ref(),
                "SceneStatus/current"
            );
            assert_eq!(properties["id"], "current");
            assert_eq!(properties["revision"], 1);
            assert_eq!(properties["command"], "reset");
            assert_eq!(properties["objects"], model::fixture().len());
            continue;
        }
        assert_eq!(labels.len(), 2, "{labels:?}");
        assert_eq!(labels[1], "SceneObject");
        let id = properties["id"].as_str().context("entity id")?;
        assert_eq!(
            metadata.reference.element_id.as_ref(),
            format!("{}/{id}", labels[0])
        );
        let entity = &model::fixture()[id];
        assert_eq!(labels[0], entity.label());
        assert_eq!(properties, entity.properties());
    }
    let journey = nodes
        .iter()
        .map(|node| serde_json::Value::Object(node.get_properties().into()))
        .find(|properties| properties["id"] == "journey-ada")
        .context("journey node")?;
    assert_eq!(journey["kind"], "journey");
    assert_eq!(journey["cart_id"], "cart-ada");
    assert_eq!(journey["destination_id"], "packing");
    assert!(journey["points"].is_array());
    h.close().await
}
#[tokio::test]
async fn footprint_queries_feed_geometry_and_obstructions_match_the_demo() -> Result<()> {
    let mut h = Harness::new().await?;
    let mut obstructions = query("obstructions").await?;
    let step = h.send(Command::Reset).await?;
    for output in &step.geometry {
        drive(&mut obstructions, output).await?;
    }
    let journeys = rows(&h.journeys)?;
    assert_eq!(journeys.len(), 2, "{journeys:?}");
    let ada = journeys
        .iter()
        .find(|row| row["journey_id"] == "journey-ada")
        .context("ada footprint")?;
    assert_eq!(ada["cart_id"], "cart-ada");
    assert!(ada["radius"].is_number() && ada["clearance"].is_number());
    assert_eq!(rows(&h.obstacles)?.len(), 3);
    // The wall sweep the original local geometry was tested against.
    for (y, expected) in [
        (4.5, Some("journey-ada")),
        (5.5, Some("journey-ada")),
        (5.65, Some("journey-ada")),
        (5.65001, None),
        (9.0, Some("journey-grace")),
    ] {
        let step = h.send(Command::Put { entity: wall(y) }).await?;
        for output in &step.geometry {
            drive(&mut obstructions, output).await?;
        }
        let found = rows(&obstructions)?;
        let ids: Vec<_> = found.iter().map(|row| row["id"].clone()).collect();
        match expected {
            None => assert!(found.is_empty(), "y={y}: {ids:?}"),
            Some(journey) => {
                assert_eq!(found.len(), 1, "y={y}: {ids:?}");
                let row = &found[0];
                assert_eq!(row["id"], format!("{journey}/wall"));
                assert_eq!(row["journey_id"], journey);
                assert_eq!(row["obstacle_id"], "wall");
                assert_eq!(
                    row["cart_id"],
                    if journey == "journey-ada" {
                        "cart-ada"
                    } else {
                        "cart-grace"
                    }
                );
                let distance = row["distance_m"].as_f64().context("distance")?;
                let required = row["required_m"].as_f64().context("required")?;
                assert!(distance <= required + 1e-7, "y={y}");
                if journey == "journey-ada" {
                    assert!((required - 0.65).abs() < 1e-12, "{required}");
                }
            }
        }
        assert_eq!(h.rows()?.len(), usize::from(expected.is_some()), "y={y}");
    }
    obstructions
        .stop(&StopContext::new(scope(), StopMode::Drain))
        .await?;
    h.close().await
}
#[tokio::test]
async fn derived_outputs_own_volatile_progress_through_real_query_chain() -> Result<()> {
    let scope = drasi_computation_plugin_sdk::wire::Scope {
        instance_id: "wall-test-instance".into(),
        graph_id: "wall-test".into(),
        generation: 1,
    };
    let mut previous_epoch = None;
    for _ in 0..2 {
        let worker = geometry("geometry-worker", Some(scope.clone())).await?;
        let mut h = Harness::with_geometry(worker).await?;
        let mut identity = None;
        let mut sequence = 0;
        for (command, expected_rows) in [
            (Command::Reset, 0),
            (Command::Put { entity: wall(4.5) }, 1),
            (Command::Put { entity: wall(5.5) }, 1),
            (Command::Delete { id: "wall".into() }, 0),
        ] {
            let step = h.send(command).await?;
            assert!(!step.geometry.is_empty());
            for output in step.geometry {
                assert_eq!(
                    output.envelope.system().ordering(),
                    ChangeOrdering::ProducerSequence
                );
                let progress = GraphProducerProgress::from_envelope(&output.envelope)?
                    .context("geometry output progress")?;
                let current = progress.identity();
                assert_eq!(current.component_id().as_str(), test_id("geometry-worker"));
                assert_eq!(
                    current.stream().as_str(),
                    "00000000-0000-4000-8000-000000000002/out"
                );
                assert_eq!(current.construction_scope(), "wall-test-instance");
                assert_eq!(current.graph_id(), "wall-test");
                assert!(!current.persistent());
                if let Some(identity) = &identity {
                    assert_eq!(current, identity);
                } else {
                    assert_ne!(Some(current.incarnation()), previous_epoch);
                    identity = Some(current.clone());
                }
                sequence += 1;
                assert_eq!(progress.sequence(), sequence);
                assert!(step.footprints.iter().any(|input| {
                    input.envelope.system().timestamp() == output.envelope.system().timestamp()
                }));
            }
            assert_eq!(h.rows()?.len(), expected_rows);
        }
        previous_epoch = identity.map(|identity| identity.incarnation());
        h.close().await?;
    }
    Ok(())
}
#[tokio::test]
async fn real_queries_enrich_obstacle_only_changes_and_retract() -> Result<()> {
    let mut h = Harness::new().await?;
    h.send(Command::Reset).await?;
    assert!(h.rows()?.is_empty());
    let step = h.send(Command::Put { entity: wall(4.5) }).await?;
    for repeated in &step.footprints {
        let replayed = h.geometry.transform(input(repeated)?).await?;
        assert!(
            changes(&replayed)?.is_empty(),
            "an identical footprint is a no-op"
        );
    }
    assert!(matches!(
        changes(&step.geometry)?[..],
        [SourceChange::Insert { .. }]
    ));
    let rows = h.rows()?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "journey-ada/wall");
    assert_eq!(rows[0]["cart"], "Ada");
    assert_eq!(rows[0]["task"], "Deliver packaging");
    assert_eq!(rows[0]["destination"], "Packing station");
    assert_eq!(rows[0]["obstacle"], "Movable wall");
    assert!((rows[0]["required_m"].as_f64().context("required")? - 0.65).abs() < 1e-12);
    let identity = rows[0]["id"].clone();
    let step = h.send(Command::Put { entity: wall(5.5) }).await?;
    assert!(matches!(
        changes(&step.geometry)?[..],
        [SourceChange::Update { .. } | SourceChange::Replace { .. }]
    ));
    assert_eq!(h.rows()?[0]["id"], identity);
    let step = h.send(Command::Delete { id: "wall".into() }).await?;
    assert!(matches!(
        changes(&step.geometry)?[..],
        [SourceChange::Delete { .. }]
    ));
    assert!(h.rows()?.is_empty());
    h.close().await
}
#[tokio::test]
async fn path_only_changes_multiple_causes_and_metadata_updates() -> Result<()> {
    let mut h = Harness::new().await?;
    h.send(Command::Reset).await?;
    let mut path = model::fixture()["journey-ada"].clone();
    if let Shape::Journey { points, .. } = &mut path.shape {
        *points = vec![[2., 6.5], [21., 6.5]];
    }
    h.send(Command::Put { entity: path }).await?;
    assert_eq!(h.rows()?.len(), 1);
    let mut second = wall(7.);
    second.id = "wall-two".into();
    second.name = "Second wall".into();
    h.send(Command::Put { entity: second }).await?;
    assert_eq!(h.rows()?.len(), 2);
    h.send(Command::Delete { id: "wall".into() }).await?;
    assert_eq!(h.rows()?.len(), 1);
    assert_eq!(h.rows()?[0]["obstacle"], "Second wall");
    let mut destination = model::fixture()["packing"].clone();
    destination.name = "Dispatch desk".into();
    let step = h
        .send(Command::Put {
            entity: destination,
        })
        .await?;
    assert!(
        changes(&step.geometry)?.is_empty(),
        "names are not geometry"
    );
    assert_eq!(h.rows()?[0]["destination"], "Dispatch desk");
    let mut cart = model::fixture()["cart-ada"].clone();
    cart.name = "Ada II".into();
    h.send(Command::Put { entity: cart }).await?;
    assert_eq!(h.rows()?[0]["cart"], "Ada II");
    h.close().await
}
#[tokio::test]
async fn query_filters_inactive_and_deletes_cart_path_obstacle() -> Result<()> {
    let mut h = Harness::new().await?;
    for deleted in ["cart-ada", "journey-ada", "wall", "packing"] {
        h.send(Command::Reset).await?;
        let mut inactive = wall(4.5);
        inactive.active = false;
        h.send(Command::Put { entity: inactive }).await?;
        assert!(h.rows()?.is_empty());
        assert!(rows(&h.obstacles)?
            .iter()
            .all(|row| row["obstacle_id"] != "wall"));
        h.send(Command::Put { entity: wall(4.5) }).await?;
        assert_eq!(h.rows()?.len(), 1);
        h.send(Command::Delete { id: deleted.into() }).await?;
        assert!(
            h.rows()?.is_empty(),
            "deleting {deleted} must retract impact"
        );
    }
    h.send(Command::Reset).await?;
    h.send(Command::Put { entity: wall(4.5) }).await?;
    let mut cart = model::fixture()["cart-ada"].clone();
    cart.active = false;
    let step = h.send(Command::Put { entity: cart }).await?;
    assert!(matches!(
        changes(&step.geometry)?[..],
        [SourceChange::Delete { .. }]
    ));
    assert!(h.rows()?.is_empty());
    h.close().await
}
#[tokio::test]
async fn empty_bootstrap_reset_no_op_and_revision_conflicts() -> Result<()> {
    let mut h = Harness::new().await?;
    h.send(Command::Clear).await?;
    assert!(h.rows()?.is_empty());
    h.send(Command::Reset).await?;
    h.send(Command::Put { entity: wall(4.5) }).await?;
    let revision = h.store.revision;
    assert_eq!(
        h.store
            .command(revision, Command::Put { entity: wall(4.5) })
            .await?,
        revision
    );
    assert!(h
        .store
        .command(revision - 1, Command::Delete { id: "wall".into() })
        .await
        .is_err());
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), h.source.next())
            .await
            .is_err(),
        "no-op must not emit"
    );
    let step = h.send(Command::Clear).await?;
    assert!(h.rows()?.is_empty());
    let status = nodes(&step.source)?
        .into_iter()
        .map(|node| serde_json::Value::Object(node.get_properties().into()))
        .find(|properties| properties["id"] == "current")
        .context("status node")?;
    assert_eq!(status["objects"], 0);
    assert_eq!(status["command"], "clear");
    h.send(Command::Reset).await?;
    assert!(h.rows()?.is_empty());
    h.close().await
}
