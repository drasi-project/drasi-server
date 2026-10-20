// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::*;

struct Scoped {
    reader: SourceProgressReader,
}
#[async_trait::async_trait]
impl ComputationBootstrapProvider for Scoped {
    fn validate_query_scope(
        &self,
        instance: &str,
        graph: &str,
        component: &ComponentId,
    ) -> Result<()> {
        anyhow::ensure!(
            instance == INSTANCE
                && graph == self.reader.graph_id()
                && component == self.reader.component_id(),
            "wrong child scope"
        );
        Ok(())
    }
    fn recovery_reader(&self) -> Option<SourceProgressReader> {
        Some(self.reader.clone())
    }
    async fn snapshot(&self) -> Result<ComputationBootstrapSnapshot> {
        Ok(ComputationBootstrapSnapshot {
            changes: Box::pin(futures::stream::empty()),
            watermarks: Vec::new(),
        })
    }
}

#[tokio::test]
async fn canonical_composite_recipe_roundtrips_and_resolves_real_child_resources() -> Result<()> {
    let mut topology = bootstrap_topology(json!({"kind":"memoryIndexes"}))?;
    for child in ["pg-bootstrap", "http-bootstrap"] {
        declare(
            &mut topology,
            child,
            ResourceRole::Bootstrap,
            json!({
                "kind":"nativeBootstrap","component":"query",
                "implementation":{"name":"fixture/bootstrap","version":"1"},
                "configurationVersion":1,"configuration":{},
            }),
        );
    }
    declare(
        &mut topology,
        "bootstrap",
        ResourceRole::Bootstrap,
        json!({
            "kind":"compositeBootstrap","component":"query","providers":["pg-bootstrap","http-bootstrap"],
        }),
    );
    topology.resource_dependencies.insert(
        resource("bootstrap"),
        BTreeMap::from([
            (resource("pg-bootstrap"), ResourceRole::Bootstrap),
            (resource("http-bootstrap"), ResourceRole::Bootstrap),
        ]),
    );
    let config = ComputationConfig {
        definition: topology,
    };
    validate_definition(&config)?;
    let json = serde_json::to_value(&config)?;
    let decoded: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
    assert_eq!(serde_json::to_value(decoded)?, json);
    let owner = Arc::new(QuerySourceProgress::new(
        config.definition.graph_id.as_str(),
        id("query"),
    )?);
    let child = || {
        ResourceHandle::new(
            ResourceRole::Bootstrap,
            Arc::new(QueryBootstrapResource(Arc::new(Scoped {
                reader: owner.clone().into(),
            }))),
        )
    };
    let dependencies = BTreeMap::from([
        (resource("pg-bootstrap"), child()),
        (resource("http-bootstrap"), child()),
    ]);
    let resolver = ServerManagementResources::new(&PluginRegistry::new(), None)?;
    let spec = config
        .definition
        .resources
        .iter()
        .find(|spec| spec.id == resource("bootstrap"))
        .context("composite declaration")?;
    let recipe = &config.definition.resource_configurations[&spec.id];
    let handle = resolver
        .resolve_with_dependencies(
            INSTANCE,
            &config.definition.graph_id,
            spec,
            recipe,
            &dependencies,
        )
        .await?;
    let provider = handle.get::<QueryBootstrapResource>()?;
    assert!(provider
        .0
        .recovery_reader()
        .context("actual progress")?
        .same_owner(&owner.into()));
    assert!(resolver
        .resolve(INSTANCE, &config.definition.graph_id, spec, recipe)
        .await
        .is_err());
    let mut wrong = dependencies.clone();
    wrong.insert(
        resource("pg-bootstrap"),
        ResourceHandle::new(ResourceRole::Bootstrap, Arc::new("not a bootstrap")),
    );
    assert!(resolver
        .resolve_with_dependencies(INSTANCE, &config.definition.graph_id, spec, recipe, &wrong)
        .await
        .is_err());
    assert!(resolver
        .resolve_with_dependencies(
            "foreign",
            &config.definition.graph_id,
            spec,
            recipe,
            &dependencies
        )
        .await
        .is_err());
    provider.0.stop().await?;
    for providers in [
        json!([]),
        json!(["pg-bootstrap", "pg-bootstrap"]),
        json!(["bootstrap"]),
    ] {
        let mut invalid = config.clone();
        invalid
            .definition
            .resource_configurations
            .get_mut(&resource("bootstrap"))
            .context("recipe")?["providers"] = providers;
        assert!(validate_definition(&invalid).is_err());
    }
    let mut invalid = config;
    invalid
        .definition
        .resource_dependencies
        .remove(&resource("bootstrap"));
    assert!(validate_definition(&invalid).is_err());
    Ok(())
}
