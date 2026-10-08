//! Configuration generator only. The stock drasi-server binary owns all execution.
//! Requires the stock Server's queryCatalog resource recipe.
use anyhow::{Context, Result};
use drasi_lib::computation::v1::*;
use drasi_server::{
    api::models::ConfigValue, computation::ComputationConfig, config::DrasiServerConfig,
};
use move_a_wall::{joins, wire::endpoint, INSTANCE, QUERIES};
use serde_json::json;
use std::{collections::BTreeMap, num::NonZeroUsize, sync::Arc};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .context("usage: wall-config <scene-plugin-path> <network-plugin-path>")?;
    let network_path = args
        .next()
        .context("usage: wall-config <scene-plugin-path> <network-plugin-path>")?;
    anyhow::ensure!(args.next().is_none(), "unexpected wall-config argument");
    let plugin = drasi_host_sdk::computation::load(path)?;
    let network = drasi_host_sdk::computation::load(network_path)?;
    let mut batch = ComponentBatch::builder();
    for id in ["scene", "geometry"] {
        let name = format!("move-a-wall/{id}");
        let factory = plugin
            .factories()
            .iter()
            .find(|f| f.metadata().implementation.name.as_ref() == name)
            .context("missing native factory")?
            .clone();
        batch = batch
            .component(
                factory.specification(ComponentId::try_new(id)?, json!({}))?,
                factory,
            )
            .bind_stream(
                endpoint(id, "out")?,
                StreamId::try_new(format!("{id}/out"))?,
            );
    }
    let indexes = ResourceId::try_new("wall-memory-indexes")?;
    let catalog_id = ResourceId::try_new("wall-query-catalog")?;
    let catalog = QueryResultsCatalog::new("__drasi_lib_runtime__")?;
    batch = batch
        .declare_resource(ResourceSpecification {
            id: indexes.clone(),
            role: ResourceRole::IndexBackend,
            ownership: ResourceOwnership::Graph,
            binding: "wall-memory".into(),
        })?
        .provide_resource(
            indexes.clone(),
            ResourceHandle::new(
                ResourceRole::IndexBackend,
                Arc::new(QueryIndexProviderResource(Arc::new(
                    drasi_core::computation::InMemoryComputationProvider,
                ))),
            ),
        )?
        .declare_resource(ResourceSpecification {
            id: catalog_id.clone(),
            role: ResourceRole::QueryCatalog,
            ownership: ResourceOwnership::Graph,
            binding: "wall-results".into(),
        })?
        .provide_resource(
            catalog_id.clone(),
            ResourceHandle::new(ResourceRole::QueryCatalog, Arc::new(catalog.clone())),
        )?;
    let outlet_factory = Arc::new(QueryResultsOutletFactory::default());
    batch = batch.component(
        ComponentSpecification {
            descriptor: QueryResultsOutlet::new(ComponentId::try_new("query-results")?, catalog)
                .descriptor()
                .clone(),
            role: ComponentRole::Sink,
            completion: Some(SinkCompletion::Handled),
            implementation: outlet_factory.descriptor().implementation.clone(),
            configuration_version: 1,
            configuration: BTreeMap::new(),
            dependencies: BTreeMap::from([("catalog".into(), vec![catalog_id.clone()])]),
        },
        outlet_factory,
    );
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    for (id,text) in [
        ("scene-inputs",include_str!("../queries/scene-inputs.cypher")),
        ("geometry-context",include_str!("../queries/geometry-context.cypher")),
        ("affected-journeys",include_str!("../queries/affected-journeys.cypher")),
        ("obstructions","MATCH (o:Obstruction) RETURN o.id AS id, o.journey_id AS journey_id, o.cart_id AS cart_id, o.obstacle_id AS obstacle_id, o.distance_m AS distance_m, o.required_m AS required_m"),
        ("geometry-status","MATCH (s:GeometryStatus) RETURN s.id AS id, s.revision AS revision, s.objects AS objects, s.obstructions AS obstructions"),
    ] {
        let stream = StreamId::try_new(format!("{id}/out"))?;
        let definition = ContinuousQueryDefinition {
            graph_id:"__drasi_lib_runtime__".into(), id:ComponentId::try_new(id)?,query:text.into(),
            language:ComputationQueryLanguage::Cypher,output_stream:stream.clone(),outbox_capacity:NonZeroUsize::new(128).unwrap(),
        };
        let settings = QueryExecutionSettings { joins:if id == "affected-journeys" { joins() } else {vec![]},..Default::default() };
        let configuration = BTreeMap::from([
            ("query".into(),ConfigurationValue::Literal(json!(text))),
            ("stream".into(),ConfigurationValue::Literal(json!(stream.as_str()))),
            ("outbox_capacity".into(),ConfigurationValue::Literal(json!(128))),
            ("execution".into(),ConfigurationValue::Literal(serde_json::to_value(settings)?)),
        ]);
        batch = batch.component(ComponentSpecification {
            descriptor:definition.descriptor(),role:ComponentRole::Query,completion:None,
            implementation:query_factory.descriptor().implementation.clone(),configuration_version:1,
            configuration,dependencies:BTreeMap::from([
                ("indexes".into(),vec![indexes.clone()]),
                ("catalog".into(),vec![catalog_id.clone()]),
            ]),
        },query_factory.clone()).bind_stream(endpoint(id,"out")?,stream);
    }
    let ui_factory = network
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == "drasi.network/sse-sink")
        .context("missing native SSE factory; rebuild the network plugin")?
        .clone();
    let query_streams: BTreeMap<_, _> = QUERIES
        .iter()
        .map(|query| (*query, format!("{query}/out")))
        .collect();
    batch = batch.component(
        ui_factory.specification(
            ComponentId::try_new("wall-ui")?,
            json!({
                "queryStreams": query_streams, "host": "127.0.0.1", "port": 8422,
                "ssePath": "/events", "heartbeatIntervalMs": 5000,
            }),
        )?,
        ui_factory,
    );
    let mut definition = batch.build()?.definition;
    definition
        .resource_configurations
        .insert(indexes, json!({"kind":"memoryIndexes"}));
    definition
        .resource_configurations
        .insert(catalog_id, json!({"kind":"queryCatalog"}));
    for (from, to) in [
        ("scene", "scene-inputs"),
        ("scene", "geometry-context"),
        ("geometry-context", "geometry"),
        ("geometry", "obstructions"),
        ("geometry", "affected-journeys"),
        ("geometry", "geometry-status"),
        ("scene-inputs", "query-results"),
        ("geometry-context", "query-results"),
        ("obstructions", "query-results"),
        ("affected-journeys", "query-results"),
        ("geometry-status", "query-results"),
    ] {
        definition.relationships.push(DesiredRelationship {
            definition: EdgeDefinition::new(endpoint(from, "out")?, endpoint(to, "in")?),
            policy: RelationshipPolicy::default(),
            pipe: DesiredPipe::Bounded { capacity: 32 },
        });
    }
    for query in QUERIES {
        definition.relationships.push(DesiredRelationship {
            definition: EdgeDefinition::new(endpoint(query, "out")?, endpoint("wall-ui", "in")?),
            policy: RelationshipPolicy::default(),
            pipe: DesiredPipe::Bounded { capacity: 32 },
        });
    }
    let config = DrasiServerConfig {
        id: ConfigValue::Static(INSTANCE.into()),
        host: ConfigValue::Static("127.0.0.1".into()),
        port: ConfigValue::Static(8421),
        verify_plugins: false,
        enable_ui: false,
        persist_config: false,
        cors_allowed_origins: vec!["http://127.0.0.1:5421".into()],
        computation: Some(ComputationConfig { definition }),
        ..Default::default()
    };
    config.validate()?;
    print!("{}", serde_yaml::to_string(&config)?);
    Ok(())
}
