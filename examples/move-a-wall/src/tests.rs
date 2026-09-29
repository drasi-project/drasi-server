use super::*;
use anyhow::{Context, Result};
use drasi_core::{computation::InMemoryComputationProvider, models::SourceChange};
use drasi_lib::{computation::v1::*, config::QueryJoinConfig};
use model::{Entity, Shape};
use source::{Command, SceneSource, Store};
use std::{num::NonZeroUsize, sync::Arc};

struct Harness {
    store: Store,
    source: SceneSource,
    context: ContinuousQueryTransformer,
    geometry: transformer::Geometry,
    impact: ContinuousQueryTransformer,
}
async fn query(
    id: &str,
    text: &str,
    joins: Vec<QueryJoinConfig>,
) -> Result<ContinuousQueryTransformer> {
    let mut q = ContinuousQueryTransformer::new_configured(
        ContinuousQueryDefinition {
            graph_id: "wall-test".into(),
            id: ComponentId::try_new(id)?,
            query: text.into(),
            language: ComputationQueryLanguage::Cypher,
            output_stream: StreamId::try_new(format!("{id}/out"))?,
            outbox_capacity: NonZeroUsize::new(128).unwrap(),
        },
        Arc::new(InMemoryComputationProvider),
        QueryOptions::default(),
        QueryExecutionSettings {
            joins,
            ..Default::default()
        },
        None,
    )
    .await?;
    q.start().await?;
    Ok(q)
}
fn input(output: &OutputEnvelope) -> Result<InputEnvelope> {
    Ok(InputEnvelope {
        port: PortId::try_new("in")?,
        envelope: output.envelope.clone(),
    })
}
impl Harness {
    async fn new() -> Result<Self> {
        let (store, mut source) = source::channel()?;
        source.start().await?;
        let mut geometry = transformer::Geometry::new(ComponentId::try_new("geometry")?)?;
        geometry.start().await?;
        Ok(Self {
            store,
            source,
            geometry,
            context: query(
                "geometry-context",
                include_str!("../queries/geometry-context.cypher"),
                vec![],
            )
            .await?,
            impact: query(
                "affected-journeys",
                include_str!("../queries/affected-journeys.cypher"),
                joins(),
            )
            .await?,
        })
    }
    async fn send(
        &mut self,
        command: Command,
    ) -> Result<(Vec<OutputEnvelope>, Vec<OutputEnvelope>)> {
        self.store.command(self.store.revision, command).await?;
        let source = self.source.next().await?.context("source output")?;
        let mut context = self.context.transform(input(&source)?).await?;
        self.context.delivery_completed(&context).await?;
        while self.context.has_pending_emissions() {
            let more = self.context.continue_transform().await?;
            self.context.delivery_completed(&more).await?;
            context.extend(more);
        }
        let mut outputs = vec![];
        for row in &context {
            let next = self.geometry.transform(input(row)?).await?;
            for output in &next {
                let projected = self.impact.transform(input(output)?).await?;
                self.impact.delivery_completed(&projected).await?;
                while self.impact.has_pending_emissions() {
                    let more = self.impact.continue_transform().await?;
                    self.impact.delivery_completed(&more).await?;
                }
            }
            outputs.extend(next);
        }
        Ok((context, outputs))
    }
    fn rows(&self) -> Result<Vec<serde_json::Value>> {
        self.impact
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
    async fn close(mut self) -> Result<()> {
        self.source.stop().await?;
        self.geometry.stop().await?;
        self.context.stop().await?;
        self.impact.stop().await
    }
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
        .filter(|c| c.get_reference().element_id.starts_with("Obstruction/"))
        .collect())
}
#[tokio::test]
async fn real_queries_enrich_obstacle_only_changes_and_retract() -> Result<()> {
    let mut h = Harness::new().await?;
    h.send(Command::Reset).await?;
    assert!(h.rows()?.is_empty());
    let (stale, output) = h.send(Command::Put { entity: wall(4.5) }).await?;
    for repeated in &stale {
        assert!(
            h.geometry.transform(input(repeated)?).await?.is_empty(),
            "duplicate query input is a no-op"
        );
    }
    assert!(matches!(changes(&output)?[0], SourceChange::Insert { .. }));
    let rows = h.rows()?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["cart"], "Ada");
    assert_eq!(rows[0]["task"], "Deliver packaging");
    assert_eq!(rows[0]["destination"], "Packing station");
    assert_eq!(rows[0]["obstacle"], "Movable wall");
    let identity = rows[0]["id"].clone();
    let (_, output) = h.send(Command::Put { entity: wall(5.5) }).await?;
    assert!(matches!(changes(&output)?[0], SourceChange::Update { .. }));
    assert_eq!(h.rows()?[0]["id"], identity);
    let (_, output) = h.send(Command::Delete { id: "wall".into() }).await?;
    assert!(matches!(changes(&output)?[0], SourceChange::Delete { .. }));
    assert!(h.rows()?.is_empty());
    for old in &stale {
        assert!(
            h.geometry.transform(input(old)?).await?.is_empty(),
            "stale context cannot resurrect deleted obstruction"
        );
    }
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
    h.send(Command::Put {
        entity: destination,
    })
    .await?;
    assert_eq!(h.rows()?[0]["destination"], "Dispatch desk");
    h.close().await
}
#[tokio::test]
async fn query_filters_inactive_and_deletes_cart_path_obstacle() -> Result<()> {
    let mut h = Harness::new().await?;
    for deleted in ["cart-ada", "journey-ada", "wall"] {
        h.send(Command::Reset).await?;
        let mut inactive = wall(4.5);
        inactive.active = false;
        let (context, _) = h.send(Command::Put { entity: inactive }).await?;
        assert!(h.rows()?.is_empty());
        let last = context.last().context("filtered context")?;
        let result = QueryChangeCodec::to_legacy_result(&last.envelope)?;
        assert!(!format!("{:?}", result.results).contains("\\\"active\\\":false"));
        h.send(Command::Put { entity: wall(4.5) }).await?;
        assert_eq!(h.rows()?.len(), 1);
        h.send(Command::Delete { id: deleted.into() }).await?;
        assert!(
            h.rows()?.is_empty(),
            "deleting {deleted} must retract impact"
        );
    }
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
    h.send(Command::Clear).await?;
    assert!(h.rows()?.is_empty());
    h.send(Command::Reset).await?;
    assert!(h.rows()?.is_empty());
    h.close().await
}

#[tokio::test]
async fn sparse_query_delete_clears_cache_and_old_snapshot_cannot_restore_it() -> Result<()> {
    use drasi_core::evaluation::context::QueryPartEvaluationContext;
    let mut h = Harness::new().await?;
    h.send(Command::Reset).await?;
    let (previous, _) = h.send(Command::Put { entity: wall(4.5) }).await?;
    let prior = previous.last().context("prior context")?;
    let snapshot = h.context.results().snapshot()?;
    let record = snapshot
        .rows
        .values()
        .next()
        .context("context snapshot row")?;
    let row = QueryChangeCodec::decode_row(record)?;
    let stream = StreamId::try_new("geometry-context/out")?;
    let sequence = QueryChangeCodec::query_sequence(&prior.envelope)? + 1;
    let full_delete = QueryChangeCodec::encode_evaluation(
        None,
        &ComponentId::try_new("geometry-context")?,
        SystemMetadata::new(stream.clone(), sequence),
        &[QueryPartEvaluationContext::Removing {
            before: row.values,
            row_signature: row.signature,
        }],
        QueryChangeCodec::metadata(&prior.envelope)?,
    )?
    .context("delete envelope")?;
    let sparse = full_delete
        .changes()
        .operations()
        .iter()
        .map(|op| match op {
            ChangeOperation::Deleted {
                ordinal, identity, ..
            } => ChangeOperation::Deleted {
                ordinal: *ordinal,
                identity: identity.clone(),
                before: None,
            },
            other => panic!("expected deletion, got {other:?}"),
        })
        .collect();
    let change_set = ChangeSet::try_new(
        ChangeSetId::try_new("sparse-test", vec![1].into())?,
        QueryChangeCodec::schema().descriptor().clone(),
        sparse,
    )?;
    let envelope = full_delete.derive(
        emission_id(&stream, sequence)?,
        change_set,
        SystemMetadata::new(stream, sequence),
    );
    let output = h
        .geometry
        .transform(InputEnvelope {
            port: PortId::try_new("in")?,
            envelope,
        })
        .await?;
    assert!(changes(&output)?
        .iter()
        .any(|c| matches!(c, SourceChange::Delete { .. })));
    assert!(h.geometry.transform(input(prior)?).await?.is_empty());
    h.close().await
}
