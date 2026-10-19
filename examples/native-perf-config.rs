// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Convert an HTTP/gRPC Server workload to the instance's native graph.
//! Usage: native-perf-config <native-plugin-directory> <server-config.json-or-yaml> <capacity>

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use drasi_host_sdk::computation::{NativeFactory, NativePlugin};
use drasi_lib::computation::v1::*;
use drasi_server::{api::mappings::DtoMapper, computation::ComputationConfig, DrasiServerConfig};
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

fn network_factory(directory: &Path, transport: &str, role: &str) -> Result<Arc<NativeFactory>> {
    anyhow::ensure!(
        directory.is_dir(),
        "expected the directory containing the independent native libraries"
    );
    let package = format!("drasi_computation_{role}_{transport}");
    let path = directory.join(format!(
        "{}{package}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let plugin = drasi_host_sdk::computation::load(&path)
        .with_context(|| format!("load {}", path.display()))?;
    factory(&plugin, &format!("drasi.network/{transport}-{role}"))
}

fn failure_policy(reaction: &Value) -> Result<&'static str> {
    match reaction.get("recoveryPolicy").and_then(Value::as_str) {
        None | Some("strict") => Ok("strict"),
        Some("auto_skip_gap") => Ok("skip"),
        Some(other) => anyhow::bail!("unsupported reaction recoveryPolicy {other}"),
    }
}

fn copy_fields(input: &Value, output: &mut Value, fields: &[&str]) {
    for field in fields {
        if let Some(value) = input.get(*field).filter(|value| !value.is_null()) {
            output[*field] = value.clone();
        }
    }
}

fn http_sink_config(reaction: &Value, query_id: &str) -> Result<Value> {
    let mut config = json!({
        "queryId": query_id,
        "baseUrl": "http://localhost",
        "timeoutMs": 5000,
        "maxRetries": 2,
        "failurePolicy": failure_policy(reaction)?,
    });
    copy_fields(
        reaction,
        &mut config,
        &[
            "baseUrl",
            "token",
            "timeoutMs",
            "outputTemplates",
            "adaptive",
            "batchEndpoint",
        ],
    );
    Ok(config)
}

fn grpc_sink_config(reaction: &Value, query_id: &str) -> Result<Value> {
    let mut config = json!({
        "queryId": query_id, "endpoint":"grpc://localhost:50052", "timeoutMs":5000,
        "maxRetries":3, "connectionRetryAttempts":5, "initialConnectionTimeoutMs":10000,
        "failurePolicy":failure_policy(reaction)?,
    });
    copy_fields(
        reaction,
        &mut config,
        &[
            "endpoint",
            "metadata",
            "timeoutMs",
            "maxRetries",
            "connectionRetryAttempts",
            "initialConnectionTimeoutMs",
            "outputFormat",
            "outputTemplates",
        ],
    );
    if reaction
        .get("batching")
        .is_some_and(|value| !value.is_null())
    {
        anyhow::ensure!(
            reaction.get("batchSize").is_none() && reaction.get("batchFlushTimeoutMs").is_none(),
            "batching cannot be combined with flat batch settings"
        );
        config["batching"] = reaction["batching"].clone();
    } else {
        config["batchSize"] = reaction.get("batchSize").cloned().unwrap_or(json!(100));
        config["batchFlushTimeoutMs"] = reaction
            .get("batchFlushTimeoutMs")
            .cloned()
            .unwrap_or(json!(1000));
    }
    Ok(config)
}

fn validated_specification(
    factory: &NativeFactory,
    id: ComponentId,
    config: Value,
) -> Result<ComponentSpecification> {
    // Bundled native constructors are configuration-only; validate the real
    // typed settings, not just the catalogue's shallow object-field schema.
    drop(factory.create_component(id.clone(), config.clone())?);
    factory.specification(id, config)
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let libraries = PathBuf::from(args.next().context("expected native plugin directory")?);
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
    let value: Value = serde_yaml::from_slice(&std::fs::read(input)?)?;
    let mut config: DrasiServerConfig = serde_json::from_value(value.clone())?;
    config.validate()?;
    anyhow::ensure!(
        config.instances.len() == 1
            && config.instances[0].computation.is_none()
            && config.instances[0].sources.len() == 1
            && !config.instances[0].queries.is_empty()
            && !config.instances[0].persist_index
            && config.instances[0].state_store.is_none(),
        "comparison requires one standard, in-memory instance with one network source"
    );
    let source = &value["instances"][0]["sources"][0];
    let transport = source["kind"].as_str().context("source kind")?;
    anyhow::ensure!(
        matches!(transport, "http" | "grpc"),
        "unsupported comparison transport"
    );
    anyhow::ensure!(
        source.get("durability").is_none_or(Value::is_null)
            && source.get("bootstrapProvider").is_none_or(Value::is_null),
        "source durability and coordinated bootstrap require explicit native resource bindings"
    );
    let source_id = source["id"].as_str().context("source ID")?;
    let source_factory = network_factory(&libraries, transport, "source")?;
    let sink_factory = network_factory(&libraries, transport, "sink")?;
    let mut source_config = json!({
        "stream": source_id,
        "sourceId": source_id,
        "host": "0.0.0.0",
        "port": if transport=="http" {9000} else {50051},
        "ingressCapacity": capacity,
        "timeoutMs": if transport=="http" {10000} else {5000},
    });
    copy_fields(source, &mut source_config, &["host", "port", "timeoutMs"]);
    if transport == "http" {
        copy_fields(
            source,
            &mut source_config,
            &[
                "webhooks",
                "adaptiveEnabled",
                "adaptiveMaxBatchSize",
                "adaptiveMinBatchSize",
                "adaptiveMaxWaitMs",
                "adaptiveMinWaitMs",
                "adaptiveWindowSecs",
            ],
        );
    }
    let source_spec = validated_specification(
        &source_factory,
        ComponentId::try_new(source_id)?,
        source_config,
    )?;
    let indexes = ResourceId::try_new("indexes")?;
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    let mut builder = ComponentBatch::builder()
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
            graph_id: drasi_lib::management::DesiredInstance::default()
                .topology
                .graph_id,
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
        let matches: Vec<_> = value["instances"][0]["reactions"]
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
            grpc_sink_config(reaction, &query.id)?
        };
        builder = builder
            .component(
                validated_specification(
                    &sink_factory,
                    ComponentId::try_new(reaction_id)?,
                    native_config,
                )?,
                sink_factory.clone(),
            )
            .connect(
                EdgeDefinition::new(endpoint(&query.id, "out")?, endpoint(reaction_id, "in")?),
                Box::new(BoundedPipeConfig { capacity }),
            );
    }
    anyhow::ensure!(
        value["instances"][0]["reactions"]
            .as_array()
            .context("reaction array")?
            .len()
            == instance.queries.len(),
        "extra reactions would change the measured work"
    );
    let graph = builder.build()?;
    config.instances[0].sources.clear();
    config.instances[0].queries.clear();
    config.instances[0].reactions.clear();
    config.auto_install_plugins = false;
    config.plugins.clear();
    config.instances[0].computation = Some(
        ComputationConfig {
            definition: graph.definition,
        }
        .into(),
    );
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
        let original = reaction();
        let native = http_sink_config(&original, "query").expect("HTTP settings");
        assert_eq!(native["outputTemplates"], original["outputTemplates"]);
        assert_eq!(native["baseUrl"], original["baseUrl"]);
        assert_eq!(native["maxRetries"], 2);
        assert_eq!(native["failurePolicy"], "strict");
    }

    #[test]
    fn native_http_configuration_preserves_templates_auth_and_adaptive_options() {
        let mut original = reaction();
        original["token"] = json!("test-only-token");
        original["adaptive"] = json!({"adaptiveMinBatchSize":2,"adaptiveMaxBatchSize":100});
        original["batchEndpoint"] = json!("/reaction/batch");
        original["recoveryPolicy"] = json!("auto_skip_gap");
        original["outputTemplates"]["routes"]["query"]["added"]["template"] =
            json!("{{json after}}");
        let native = http_sink_config(&original, "query").expect("HTTP settings");
        for field in ["token", "adaptive", "batchEndpoint", "outputTemplates"] {
            assert_eq!(native[field], original[field], "{field}");
        }
        assert_eq!(native["failurePolicy"], "skip");
    }

    #[test]
    fn native_grpc_configuration_preserves_fixed_and_adaptive_batch_contracts() {
        let fixed = json!({"batchSize":32,"batchFlushTimeoutMs":15,"outputFormat":"proto"});
        let native = grpc_sink_config(&fixed, "query").expect("fixed");
        assert_eq!(native["batchSize"], 32);
        assert_eq!(native["batchFlushTimeoutMs"], 15);
        assert_eq!(native["outputFormat"], "proto");
        let adaptive = json!({"batching":{"mode":"adaptive","adaptiveMaxBatchSize":100},
            "outputTemplates":{"defaultTemplate":{"added":{"template":"{{json after}}"}}}});
        let native = grpc_sink_config(&adaptive, "query").expect("adaptive");
        assert_eq!(native["batching"], adaptive["batching"]);
        assert_eq!(native["outputTemplates"], adaptive["outputTemplates"]);
        assert!(native.get("batchSize").is_none());
        assert!(grpc_sink_config(&json!({"batching":{},"batchSize":1}), "query").is_err());
    }
}
