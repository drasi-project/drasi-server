//! An in-process copy of the runtime's processing graph: the database
//! configuration query, the lab transformers, the processing queries and the
//! real `drasi.standard/policy` and `drasi.standard/optimization` instances,
//! wired by the same `plugins::EDGES` the runtime registers.
use super::*;
use crate::inputs::{
    BootstrapBoundary, QueryBootstrapRow, QueryBootstrapWatermark, DATABASE_QUERY,
};
use drasi_computation_plugin_sdk::{testing, Component, ControlSender, Factory};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

pub(super) enum Node {
    Native(Box<dyn NativeTransformer>),
    Query(Box<ContinuousQueryTransformer>),
}

pub(super) struct Lab {
    nodes: BTreeMap<String, Node>,
    edges: Vec<(String, String)>,
    database: Emitter,
    records: Vec<Value>,
    configuration: Arc<std::sync::Mutex<crate::lifecycle::Signals>>,
    bootstrapped: bool,
    pub(super) workers: Arc<Workers>,
    pub(super) log: Vec<(String, OutputEnvelope)>,
}

fn plugin(factory: &dyn Factory, name: &str, config: Value) -> Result<Box<dyn NativeTransformer>> {
    let request =
        testing::create_request(factory, &crate::test_contexts::component_id(name)?, config);
    match factory
        .create(&request, ControlSender::default())?
        .component
    {
        Component::Transformer(transformer) => Ok(transformer),
        _ => anyhow::bail!("{name} is not a transformer"),
    }
}

impl Lab {
    /// Builds the processing graph plus the named UI queries, which observe
    /// the database tables and every producer as in the runtime.
    pub(super) async fn new(ui: &[&str]) -> Result<Self> {
        let workers = Arc::new(Workers::default());
        let mut nodes = BTreeMap::new();
        let configuration = processor(Kind::Configuration, workers.clone())?;
        let signals = configuration.signals.clone();
        for (name, node) in [
            ("configuration", configuration),
            ("simulator", processor(Kind::Simulator, workers.clone())?),
            ("placement", processor(Kind::Placement, workers.clone())?),
        ] {
            let mut node: Box<dyn NativeTransformer> = Box::new(node);
            node.start(&mut crate::test_contexts::sdk::start()).await?;
            nodes.insert(name.to_owned(), Node::Native(node));
        }
        for (name, mut node) in [
            (
                "policy",
                plugin(
                    &drasi_computation_transformer_policy::PolicyFactory,
                    "policy",
                    crate::plugins::policy(),
                )?,
            ),
            (
                "solver",
                plugin(
                    &drasi_computation_transformer_optimization::OptimizationFactory,
                    "solver",
                    crate::plugins::solver(),
                )?,
            ),
            (
                "capacity-check",
                plugin(
                    &drasi_computation_transformer_optimization::OptimizationFactory,
                    "capacity-check",
                    crate::plugins::capacity_check(),
                )?,
            ),
        ] {
            node.start(&mut drasi_computation_plugin_sdk::testing::contexts::start())
                .await?;
            nodes.insert(name.to_owned(), Node::Native(node));
        }
        let text = crate::inputs::database_queries().remove(0).1;
        nodes.insert(
            DATABASE_QUERY.into(),
            Node::Query(Box::new(query(DATABASE_QUERY, &text).await?)),
        );
        for (id, text, settings) in crate::inputs::processing_queries() {
            nodes.insert(
                id.into(),
                Node::Query(Box::new(configured_query(id, text, settings).await?)),
            );
        }
        let mut edges: Vec<(String, String)> = vec![
            ("database".into(), DATABASE_QUERY.into()),
            (DATABASE_QUERY.into(), "configuration".into()),
        ];
        edges.extend(
            crate::plugins::EDGES
                .iter()
                .filter(|(from, to)| nodes.contains_key(*from) && nodes.contains_key(*to))
                .map(|(from, to)| ((*from).to_owned(), (*to).to_owned())),
        );
        for (id, text, settings) in crate::projections::definitions() {
            if !ui.contains(&id) {
                continue;
            }
            nodes.insert(
                id.into(),
                Node::Query(Box::new(configured_query(id, text, settings).await?)),
            );
            for producer in [
                "runtime-status",
                "configuration",
                "simulator",
                "placement",
                "policy",
                "solver",
                "capacity-check",
            ] {
                edges.push((producer.into(), id.into()));
            }
        }
        Ok(Self {
            nodes,
            edges,
            database: Emitter::new(
                StreamId::try_new("lab-database")?,
                crate::test_contexts::component_id("lab-database")?,
                None,
            )?,
            records: Vec::new(),
            configuration: signals,
            bootstrapped: false,
            workers,
            log: Vec::new(),
        })
    }

    /// Adds a test-only query fed by `producers`.
    pub(super) async fn probe(&mut self, id: &str, text: &str, producers: &[&str]) -> Result<()> {
        self.nodes
            .insert(id.into(), Node::Query(Box::new(query(id, text).await?)));
        for producer in producers {
            self.edges.push(((*producer).into(), id.into()));
        }
        Ok(())
    }

    /// Writes the fixture's tables and bootstraps the configuration processor
    /// from the database query's snapshot, as the runtime's boundary does.
    pub(super) async fn bootstrap(
        &mut self,
        fixture: &fixtures::Fixture,
        epoch: Uuid,
    ) -> Result<()> {
        let records = super::inputs::fixture_rows(fixture, epoch)?
            .remove(DATABASE_QUERY)
            .context("fixture rows missing")?;
        self.records = records.as_array().context("records")?.clone();
        let mut changes = Vec::new();
        for (index, record) in self.records.iter().enumerate() {
            let mut value = record["value"].clone();
            value["__gpu_table"] = record["table"].clone();
            changes.extend(self.database.record(
                record["table"].as_str().context("table")?,
                &index.to_string(),
                &value,
            )?);
        }
        let outputs = self.database.emit(changes, None)?;
        self.deliver("database", outputs).await?;
        let Some(Node::Query(query)) = self.nodes.get(DATABASE_QUERY) else {
            unreachable!()
        };
        let snapshot = query.results().snapshot()?;
        let row = QueryChangeCodec::decode_row(snapshot.rows.values().next().context("no row")?)?;
        self.configuration.lock().unwrap().bootstrap = Some(BootstrapBoundary {
            epoch,
            queries: BTreeMap::from([(
                DATABASE_QUERY.into(),
                QueryBootstrapWatermark {
                    sequence: snapshot.as_of_sequence,
                    row_count: 1,
                    rows: Some(vec![QueryBootstrapRow {
                        signature: row.signature.context("row signature")?,
                        records: serde_json::from_value(serde_json::to_value(
                            row.values["records"].clone(),
                        )?)?,
                    }]),
                },
            )]),
        });
        self.bootstrapped = true;
        self.pump().await
    }

    /// Inserts a database row into `table`.
    pub(super) async fn insert(&mut self, table: &str, value: Value) -> Result<()> {
        self.records.push(json!({"table": table, "value": value}));
        let index = self.records.len() - 1;
        let mut value = self.records[index]["value"].clone();
        value["__gpu_table"] = json!(table);
        let change = self
            .database
            .record(table, &index.to_string(), &value)?
            .context("unchanged row")?;
        let outputs = self.database.emit(vec![change], None)?;
        self.deliver("database", outputs).await
    }

    /// Edits the first database row of `table` accepted by `select`.
    pub(super) async fn update(
        &mut self,
        table: &str,
        select: impl Fn(&Value) -> bool,
        edit: impl FnOnce(&mut Value),
    ) -> Result<()> {
        let (index, record) = self
            .records
            .iter_mut()
            .enumerate()
            .find(|(_, record)| record["table"] == table && select(&record["value"]))
            .context("database row missing")?;
        edit(&mut record["value"]);
        match &record["value"]["revision"] {
            Value::String(revision) => {
                record["value"]["revision"] = json!((revision.parse::<u64>()? + 1).to_string())
            }
            Value::Number(revision) => {
                record["value"]["revision"] = json!(revision.as_u64().context("revision")? + 1)
            }
            _ => {}
        }
        let mut value = record["value"].clone();
        value["__gpu_table"] = json!(table);
        let change = self
            .database
            .record(table, &index.to_string(), &value)?
            .context("unchanged row")?;
        let outputs = self.database.emit(vec![change], None)?;
        self.deliver("database", outputs).await
    }

    /// Delivers `outputs` of `from` through the graph.
    pub(super) async fn deliver(&mut self, from: &str, outputs: Vec<OutputEnvelope>) -> Result<()> {
        let mut queue: VecDeque<_> = outputs.into_iter().map(|o| (from.to_owned(), o)).collect();
        while let Some((from, output)) = queue.pop_front() {
            self.log.push((from.clone(), output.clone()));
            let targets: Vec<_> = self
                .edges
                .iter()
                .filter(|(source, _)| *source == from)
                .map(|(_, target)| target.clone())
                .collect();
            for target in targets {
                if target == "configuration" && !self.bootstrapped {
                    continue;
                }
                let input = InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: output.envelope.clone(),
                };
                let produced = match self.nodes.get_mut(&target).context("node")? {
                    Node::Native(node) => {
                        let produced = node
                            .transform(input)
                            .await
                            .with_context(|| format!("{target} processing {from}"))?;
                        node.delivery_completed(&produced).await?;
                        produced
                    }
                    Node::Query(query) => {
                        let produced = query
                            .transform(input)
                            .await
                            .with_context(|| format!("{target} processing {from}"))?;
                        query.delivery_completed(&produced).await?;
                        produced
                    }
                };
                queue.extend(produced.into_iter().map(|o| (target.clone(), o)));
            }
        }
        Ok(())
    }

    /// Runs every component's due timers once.
    pub(super) async fn pump(&mut self) -> Result<()> {
        let names: Vec<_> = self.nodes.keys().cloned().collect();
        for name in names {
            let produced = match self.nodes.get_mut(&name).context("node")? {
                Node::Native(node) => {
                    let mut produced = Vec::new();
                    while node.has_pending_emissions() {
                        produced.extend(node.continue_transform().await?);
                    }
                    if node.wakeup_source().is_some() {
                        produced.extend(node.on_wakeup().await?);
                    }
                    produced
                }
                Node::Query(query) => {
                    if query.wakeup_source().is_none() {
                        continue;
                    }
                    query.on_wakeup().await?
                }
            };
            if let Node::Query(query) = self.nodes.get_mut(&name).context("node")? {
                query.delivery_completed(&produced).await?;
            }
            self.deliver(&name, produced).await?;
        }
        Ok(())
    }

    /// Pumps timers until `done` holds.
    pub(super) async fn until(
        &mut self,
        what: &str,
        mut done: impl FnMut(&mut Self) -> Result<bool>,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.pump().await?;
            if done(self)? {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// The current rows of query `id`.
    pub(super) fn rows(&self, id: &str) -> Result<Vec<Value>> {
        let Some(Node::Query(query)) = self.nodes.get(id) else {
            anyhow::bail!("{id} is not a query");
        };
        query
            .results()
            .snapshot()?
            .rows
            .values()
            .map(|record| {
                Ok(serde_json::to_value(
                    QueryChangeCodec::decode_row(record)?.values,
                )?)
            })
            .collect()
    }

    /// Payloads `producer` emitted with `label`, oldest first.
    pub(super) fn payloads(&self, producer: &str, label: &str) -> Result<Vec<Value>> {
        self.payloads_since(0, producer, label)
    }

    /// Like [`Self::payloads`], for outputs logged at or after `mark`.
    pub(super) fn payloads_since(
        &self,
        mark: usize,
        producer: &str,
        label: &str,
    ) -> Result<Vec<Value>> {
        let outputs: Vec<_> = self.log[mark..]
            .iter()
            .filter(|(from, _)| from == producer)
            .map(|(_, output)| output.clone())
            .collect();
        payloads(&outputs, label)
    }

    pub(super) async fn stop(self) -> Result<()> {
        for (_, node) in self.nodes {
            match node {
                Node::Native(mut node) => {
                    node.stop(&crate::test_contexts::sdk::stop()).await?;
                }
                Node::Query(mut query) => query.stop(&crate::test_contexts::lib::stop()).await?,
            }
        }
        Ok(())
    }
}
