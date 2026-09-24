// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Convert a standard HTTP/gRPC Server perf configuration to a native graph.
//! Usage: native-perf-config <native-network-library> <adapter-config.json> <capacity>

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use drasi_host_sdk::computation::{NativeFactory, NativePlugin};
use drasi_lib::computation::v1::*;
use drasi_server::{
    api::mappings::DtoMapper, computation::ComputationGraphConfig, DrasiServerConfig,
};
use serde_json::{json, Value};

fn factory(plugin: &NativePlugin, name: &str) -> Result<Arc<NativeFactory>> {
    plugin
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == name)
        .cloned()
        .with_context(|| format!("network library does not supply {name}"))
}

fn endpoint(component: &str, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(
        ComponentId::try_new(component)?,
        PortId::try_new(port)?,
    ))
}

fn literal(value: Value) -> ConfigurationValue {
    ConfigurationValue::Literal(value)
}

fn http_sink_config(reaction: &Value, query_id: &str) -> Result<Value> {
    anyhow::ensure!(
        reaction.get("adaptive").is_none() && reaction.get("token").is_none(),
        "adaptive batching and authentication need a separate comparison profile"
    );
    let headers = json!({"Content-Type":"application/json","X-Query-Sequence":query_id});
    let call = json!({"method":"POST","url":"/reaction","headers":headers});
    let expected_templates = json!({
        "routes": BTreeMap::from([(query_id, json!({
            "added": call, "updated": call, "deleted": call,
        }))]),
    });
    anyhow::ensure!(
        reaction.get("outputTemplates") == Some(&expected_templates),
        "HTTP comparison requires the standard /reaction routes without custom body templates or headers"
    );
    Ok(json!({
        "queryId": query_id,
        "url": format!("{}/reaction", reaction["baseUrl"].as_str().context("HTTP baseUrl")?.trim_end_matches('/')),
        "headers": headers,
        "timeoutMs": reaction["timeoutMs"],
        "maxRetries": 3,
        "failurePolicy": "skip",
    }))
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let library = PathBuf::from(
        args.next()
            .context("expected native network library path")?,
    );
    let input = PathBuf::from(args.next().context("expected adapter Server JSON path")?);
    let capacity: usize = args
        .next()
        .context("expected queue capacity")?
        .to_str()
        .context("queue capacity is not UTF-8")?
        .parse()
        .context("queue capacity is not an integer")?;
    anyhow::ensure!(
        capacity > 0 && args.next().is_none(),
        "expected one nonzero queue capacity"
    );
    let value: Value = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut config: DrasiServerConfig = serde_json::from_value(value.clone())?;
    config.validate()?;
    anyhow::ensure!(
        config.instances.is_empty()
            && config.computation_graphs.is_empty()
            && config.sources.len() == 1
            && !config.queries.is_empty()
            && !config.persist_index
            && config.state_store.is_none(),
        "comparison requires one standard, in-memory instance with one network source"
    );
    let source = &value["sources"][0];
    let transport = source["kind"].as_str().context("source kind")?;
    anyhow::ensure!(
        matches!(transport, "http" | "grpc"),
        "unsupported comparison transport"
    );
    anyhow::ensure!(
        source.get("adaptiveEnabled").and_then(Value::as_bool) != Some(true)
            && source.get("durability").is_none()
            && source.get("bootstrapProvider").is_none(),
        "adaptive batching, source durability and bootstrap-provider comparisons require a separate profile"
    );
    let source_id = source["id"].as_str().context("source ID")?;
    let plugin = drasi_host_sdk::computation::load(library)?;
    let source_factory = factory(&plugin, &format!("drasi.network/{transport}-source"))?;
    let sink_factory = factory(&plugin, &format!("drasi.network/{transport}-sink"))?;
    let source_config = json!({
        "stream": source_id,
        "sourceId": source_id,
        "host": source["host"],
        "port": source["port"],
        "ingressCapacity": capacity,
        "timeoutMs": source["timeoutMs"],
    });
    let source_spec =
        source_factory.specification(ComponentId::try_new(source_id)?, source_config)?;
    let indexes = ResourceId::try_new("indexes")?;
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    let mut builder = ComputationGraph::builder("performance")
        .declare_resource(ResourceSpecification {
            id: indexes.clone(),
            role: ResourceRole::IndexBackend,
            ownership: ResourceOwnership::Borrowed,
            binding: "indexes".into(),
        })?
        .resource_configuration(indexes.clone(), json!({"kind":"memoryIndexes"}))?
        .component(source_spec, source_factory)
        .bind_stream(endpoint(source_id, "out")?, StreamId::try_new(source_id)?);
    let mut resolved = config.resolved_instances(&DtoMapper::new())?;
    let instance = resolved.pop().context("resolved instance")?;
    anyhow::ensure!(
        resolved.is_empty(),
        "comparison must contain exactly one instance"
    );
    for query in &instance.queries {
        anyhow::ensure!(
            !query.enable_bootstrap
                && query.middleware.is_empty()
                && query.sources.len() == 1
                && query.sources[0].source_id == source_id,
            "comparison expects one direct source and no query bootstrap/middleware"
        );
        let stream = StreamId::try_new(format!("{}/out", query.id))?;
        let language = match query.query_language {
            drasi_lib::QueryLanguage::Cypher => ComputationQueryLanguage::Cypher,
            drasi_lib::QueryLanguage::GQL => ComputationQueryLanguage::Gql,
        };
        let definition = ContinuousQueryDefinition {
            graph_id: "performance".into(),
            id: ComponentId::try_new(query.id.as_str())?,
            query: query.query.clone(),
            language,
            output_stream: stream.clone(),
            outbox_capacity: std::num::NonZeroUsize::new(query.outbox_capacity)
                .context("query outbox capacity must be nonzero")?,
        };
        let specification = ComponentSpecification {
            descriptor: definition.descriptor(),
            role: ComponentRole::Query,
            completion: None,
            implementation: query_factory.descriptor().implementation.clone(),
            configuration_version: 1,
            configuration: BTreeMap::from([
                ("query".into(), literal(json!(query.query))),
                (
                    "language".into(),
                    literal(json!(match language {
                        ComputationQueryLanguage::Cypher => "cypher",
                        ComputationQueryLanguage::Gql => "gql",
                    })),
                ),
                ("stream".into(), literal(json!(stream))),
                (
                    "outbox_capacity".into(),
                    literal(json!(query.outbox_capacity)),
                ),
                (
                    "execution".into(),
                    literal(serde_json::to_value(
                        QueryExecutionSettings::from_legacy_config(query),
                    )?),
                ),
                // Preserve outward metadata, not the old execution or plugin model.
                ("runtime_compatibility".into(), literal(json!(true))),
            ]),
            dependencies: BTreeMap::from([("indexes".into(), vec![indexes.clone()])]),
        };
        builder = builder
            .component(specification, query_factory.clone())
            .bind_stream(endpoint(&query.id, "out")?, stream)
            .connect(
                EdgeDefinition::new(endpoint(source_id, "out")?, endpoint(&query.id, "in")?),
                Box::new(BoundedPipeConfig { capacity }),
            );
        let matches: Vec<_> = value["reactions"]
            .as_array()
            .context("reaction array")?
            .iter()
            .filter(|reaction| reaction["queries"] == json!([query.id]))
            .collect();
        anyhow::ensure!(
            matches.len() == 1,
            "each query needs exactly one matching reaction"
        );
        let reaction = matches[0];
        anyhow::ensure!(
            reaction["kind"] == transport,
            "mixed transport comparison is not supported"
        );
        let reaction_id = reaction["id"].as_str().context("reaction ID")?;
        let native_config = if transport == "http" {
            http_sink_config(reaction, &query.id)?
        } else {
            anyhow::ensure!(
                reaction["batchSize"] == 1,
                "native comparison requires gRPC batchSize=1"
            );
            json!({
                "queryId": query.id,
                "endpoint": reaction["endpoint"],
                "metadata": reaction["metadata"],
                "timeoutMs": reaction["timeoutMs"],
                "maxRetries": reaction["maxRetries"],
                "connectionRetryAttempts": reaction["connectionRetryAttempts"],
                "initialConnectionTimeoutMs": reaction["initialConnectionTimeoutMs"],
                "batchSize": 1,
                "batchFlushTimeoutMs": reaction["batchFlushTimeoutMs"],
            })
        };
        builder = builder
            .component(
                sink_factory.specification(ComponentId::try_new(reaction_id)?, native_config)?,
                sink_factory.clone(),
            )
            .connect(
                EdgeDefinition::new(endpoint(&query.id, "out")?, endpoint(reaction_id, "in")?),
                Box::new(BoundedPipeConfig { capacity }),
            );
    }
    anyhow::ensure!(
        value["reactions"]
            .as_array()
            .context("reaction array")?
            .len()
            == instance.queries.len(),
        "extra reactions would change the measured work"
    );
    let graph = builder.build()?;
    config.sources.clear();
    config.queries.clear();
    config.reactions.clear();
    config.computation_graphs = vec![ComputationGraphConfig {
        auto_start: true,
        definition: graph.configuration_snapshot()?.topology,
    }];
    config.validate()?;
    println!("{}", serde_json::to_string_pretty(&config)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reaction() -> Value {
        let call = json!({
            "method":"POST", "url":"/reaction",
            "headers":{"Content-Type":"application/json","X-Query-Sequence":"query"},
        });
        json!({
            "baseUrl":"http://127.0.0.1:9001", "timeoutMs":60000,
            "outputTemplates":{"routes":{"query":{
                "added":call, "updated":call, "deleted":call,
            }}},
        })
    }

    #[test]
    fn standard_http_profile_preserves_endpoint_headers_and_delivery_settings() {
        assert_eq!(
            http_sink_config(&reaction(), "query").expect("standard HTTP profile"),
            json!({
                "queryId":"query", "url":"http://127.0.0.1:9001/reaction",
                "headers":{"Content-Type":"application/json","X-Query-Sequence":"query"},
                "timeoutMs":60000, "maxRetries":3, "failurePolicy":"skip",
            }),
        );
    }

    #[test]
    fn unsupported_http_behavior_is_not_silently_dropped() {
        for field in ["method", "url", "headers", "template"] {
            let mut changed = reaction();
            changed["outputTemplates"]["routes"]["query"]["added"][field] = json!("changed");
            assert!(http_sink_config(&changed, "query").is_err(), "{field}");
        }
        for field in ["adaptive", "token"] {
            let mut changed = reaction();
            changed[field] = json!("unsupported");
            assert!(http_sink_config(&changed, "query").is_err(), "{field}");
        }
    }
}
