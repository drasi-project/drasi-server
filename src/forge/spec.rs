// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Normalize one Server instance using the current Server config mappings.

use anyhow::{bail, Context, Result};
use drasi_lib::indexes::{StorageBackendRef, StorageBackendSpec};
use std::collections::HashSet;
use std::path::Path;

use crate::api::mappings::DtoMapper;
use crate::api::models::{StateStoreConfig, BUILTIN_PASSWORD_KIND};
use crate::config::{load_config_file, DrasiServerConfig};
use crate::factories::{build_bootstrap_provider_config_map, resolve_source_bootstrap_provider};
use crate::index_provider::PERSISTENT_INDEX_PROVIDER_NAME;

use super::catalog::Catalog;

#[derive(Debug, Clone)]
pub struct BootstrapInst {
    pub kind: String,
    pub config_json: String,
}

#[derive(Debug, Clone)]
pub struct SourceInst {
    pub kind: String,
    pub id: String,
    pub config_json: String,
    pub auto_start: bool,
    pub bootstrap: Option<BootstrapInst>,
    pub identity_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReactionInst {
    pub kind: String,
    pub id: String,
    pub query_ids: Vec<String>,
    pub config_json: String,
    pub auto_start: bool,
    pub identity_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IdentityProviderInst {
    pub id: String,
    pub kind: String,
    pub config_json: String,
    pub builtin_password: bool,
}

#[derive(Debug, Clone)]
pub struct SecretStoreInst {
    pub kind: String,
    pub config_json: String,
}

#[derive(Debug, Clone)]
pub struct StateStoreInst {
    pub path_config_json: String,
}

#[derive(Debug, Clone)]
pub struct QueryInst {
    pub id: String,
    /// Preserve every mapped QueryConfig field, not just Query builder defaults.
    pub config_json: String,
}

#[derive(Debug, Clone)]
pub struct ForgeSpec {
    pub name: String,
    pub instance_id: String,
    pub log_level: String,
    pub persist_index: bool,
    pub enable_archive: bool,
    pub memory_budget_bytes: Option<usize>,
    pub priority_queue_capacity: Option<usize>,
    pub dispatch_buffer_capacity: Option<usize>,
    pub sources: Vec<SourceInst>,
    pub reactions: Vec<ReactionInst>,
    pub queries: Vec<QueryInst>,
    pub identity_providers: Vec<IdentityProviderInst>,
    pub secret_store: Option<SecretStoreInst>,
    pub state_store: Option<StateStoreInst>,
    pub needs_middleware_all: bool,
    pub warnings: Vec<String>,
}

impl ForgeSpec {
    pub fn component_kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self
            .sources
            .iter()
            .map(|s| format!("source:{}", s.kind))
            .chain(
                self.reactions
                    .iter()
                    .map(|r| format!("reaction:{}", r.kind)),
            )
            .chain(self.sources.iter().filter_map(|s| {
                s.bootstrap
                    .as_ref()
                    .map(|b| format!("bootstrap:{}", b.kind))
            }))
            .chain(
                self.identity_providers
                    .iter()
                    .map(|i| format!("identity:{}", i.kind)),
            )
            .chain(
                self.secret_store
                    .iter()
                    .map(|s| format!("secretStore:{}", s.kind)),
            )
            .chain(
                self.state_store
                    .iter()
                    .map(|_| "stateStore:redb".to_string()),
            )
            .collect();
        if self.persist_index {
            kinds.push("index:rocksdb".to_string());
        }
        if !self.sources.is_empty() {
            kinds.push("wal:redb".to_string());
        }
        kinds.sort();
        kinds.dedup();
        kinds
    }

    pub fn needs_config_resolver(&self) -> bool {
        self.state_store.is_some() || self.identity_providers.iter().any(|i| i.builtin_password)
    }
}

pub fn derive_spec(config_path: &Path, name: Option<&str>, catalog: &Catalog) -> Result<ForgeSpec> {
    let config = load_config_file(config_path)
        .with_context(|| format!("loading config {}", config_path.display()))?;
    derive_spec_from_config(&config, name, catalog)
}

pub fn derive_spec_from_config(
    config: &DrasiServerConfig,
    name: Option<&str>,
    catalog: &Catalog,
) -> Result<ForgeSpec> {
    config.validate()?;
    let mapper = DtoMapper::new();
    let resolved = config.resolved_instances(&mapper)?;
    if resolved.len() != 1 {
        bail!("forge requires exactly one instance; split multi-instance configuration into separate projects");
    }
    let instance = &resolved[0];
    let bootstrap_map = build_bootstrap_provider_config_map(&instance.bootstrap_providers)?;
    let mut unsupported = Vec::new();

    let identity_ids = unique_ids(
        "identityProvider",
        instance.identity_providers.iter().map(|i| i.id()),
    )?;
    let source_ids = unique_ids("source", instance.sources.iter().map(|s| s.id()))?;
    let query_ids = unique_ids("query", instance.queries.iter().map(|q| q.id.as_str()))?;
    unique_ids("reaction", instance.reactions.iter().map(|r| r.id()))?;

    let mut identity_providers = Vec::new();
    for ip in &instance.identity_providers {
        let builtin_password = ip.kind() == BUILTIN_PASSWORD_KIND;
        if !builtin_password && catalog.identity_provider(ip.kind()).is_none() {
            unsupported.push(format!(
                "identityProvider kind '{}' is not in the forge catalog",
                ip.kind()
            ));
        }
        identity_providers.push(IdentityProviderInst {
            id: ip.id().to_string(),
            kind: ip.kind().to_string(),
            config_json: serde_json::to_string(&ip.config)?,
            builtin_password,
        });
    }

    let mut sources = Vec::new();
    for sc in &instance.sources {
        let sc = resolve_source_bootstrap_provider(sc.clone(), &bootstrap_map)?;
        if catalog.source(sc.kind()).is_none() {
            unsupported.push(format!(
                "source kind '{}' is not in the forge catalog",
                sc.kind()
            ));
        }
        let bootstrap = sc
            .bootstrap_provider
            .as_ref()
            .and_then(|b| b.as_inline())
            .map(|bp| {
                if catalog.bootstrapper(bp.kind()).is_none() {
                    unsupported.push(format!(
                        "bootstrapProvider kind '{}' is not in the forge catalog",
                        bp.kind()
                    ));
                }
                Ok::<_, anyhow::Error>(BootstrapInst {
                    kind: bp.kind().to_string(),
                    config_json: serde_json::to_string(&bp.config)?,
                })
            })
            .transpose()?;
        check_identity(sc.identity_provider.as_deref(), &identity_ids, sc.id())?;
        sources.push(SourceInst {
            kind: sc.kind().to_string(),
            id: sc.id().to_string(),
            config_json: serde_json::to_string(&sc.config)?,
            auto_start: sc.auto_start(),
            bootstrap,
            identity_ref: sc.identity_provider.clone(),
        });
    }

    let mut reactions = Vec::new();
    for rc in &instance.reactions {
        if catalog.reaction(rc.kind()).is_none() {
            unsupported.push(format!(
                "reaction kind '{}' is not in the forge catalog",
                rc.kind()
            ));
        }
        check_identity(rc.identity_provider.as_deref(), &identity_ids, rc.id())?;
        for id in rc.queries() {
            if !query_ids.contains(id.as_str()) {
                bail!("reaction '{}' references unknown query '{id}'", rc.id());
            }
        }
        reactions.push(ReactionInst {
            kind: rc.kind().to_string(),
            id: rc.id().to_string(),
            query_ids: rc.queries().to_vec(),
            config_json: serde_json::to_string(&rc.config)?,
            auto_start: rc.auto_start(),
            identity_ref: rc.identity_provider.clone(),
        });
    }

    let secret_store = instance
        .secret_store
        .as_ref()
        .map(|ss| {
            if catalog.secret_store(&ss.kind).is_none() {
                unsupported.push(format!(
                    "secretStore kind '{}' is not in the forge catalog",
                    ss.kind
                ));
            }
            Ok::<_, anyhow::Error>(SecretStoreInst {
                kind: ss.kind.clone(),
                config_json: serde_json::to_string(&ss.config)?,
            })
        })
        .transpose()?;
    let state_store = match &instance.state_store {
        Some(StateStoreConfig::Redb { path }) => Some(StateStoreInst {
            path_config_json: serde_json::to_string(path)?,
        }),
        None => None,
    };

    let mut queries = Vec::new();
    let mut needs_middleware_all = false;
    for q in &instance.queries {
        for source in &q.sources {
            if !source_ids.contains(source.source_id.as_str()) {
                bail!(
                    "query '{}' references unknown source '{}'",
                    q.id,
                    source.source_id
                );
            }
        }
        match &q.storage_backend {
            None | Some(StorageBackendRef::Inline(StorageBackendSpec::Memory { .. })) => {}
            Some(StorageBackendRef::Named(kind))
                if kind == PERSISTENT_INDEX_PROVIDER_NAME && instance.persist_index => {}
            Some(backend) => unsupported.push(format!(
                "query '{}' storageBackend {backend:?} has no configured provider; use inline memory or the named 'rocksdb' backend with persistIndex: true (inline plugins are unsupported by drasi-lib)",
                q.id
            )),
        }
        needs_middleware_all |= !q.middleware.is_empty();
        queries.push(QueryInst {
            id: q.id.clone(),
            config_json: serde_json::to_string(q)?,
        });
    }
    if !unsupported.is_empty() {
        unsupported.sort();
        unsupported.dedup();
        bail!("configuration is not supported by forge:\n  - {}\nUse the full drasi-server or a supported catalog component.", unsupported.join("\n  - "));
    }
    if sources.is_empty() && reactions.is_empty() {
        bail!("configuration has no forgeable sources or reactions");
    }
    let mut warnings = Vec::new();
    if !config.plugins.is_empty() {
        warnings.push("Plugin registry references/pins are not copied: generated projects use the static forge catalog versions.".into());
    }
    Ok(ForgeSpec {
        name: name
            .map(str::to_string)
            .unwrap_or_else(|| format!("drasi-{}", instance.id)),
        instance_id: instance.id.clone(),
        log_level: mapper.resolve_string(&config.log_level)?,
        persist_index: instance.persist_index,
        enable_archive: instance.enable_archive,
        memory_budget_bytes: instance.memory_budget_bytes,
        priority_queue_capacity: instance.default_priority_queue_capacity,
        dispatch_buffer_capacity: instance.default_dispatch_buffer_capacity,
        sources,
        reactions,
        queries,
        identity_providers,
        secret_store,
        state_store,
        needs_middleware_all,
        warnings,
    })
}

fn unique_ids<'a>(kind: &str, ids: impl Iterator<Item = &'a str>) -> Result<HashSet<&'a str>> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() || !seen.insert(id) {
            bail!("empty or duplicate {kind} id '{id}'");
        }
    }
    Ok(seen)
}

fn check_identity(id: Option<&str>, declared: &HashSet<&str>, component: &str) -> Result<()> {
    if let Some(id) = id {
        if !declared.contains(id) {
            bail!("component '{component}' references unknown identityProvider '{id}'");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(yaml: &str) -> Result<ForgeSpec> {
        derive_spec_from_config(&serde_yaml::from_str(yaml)?, None, &Catalog::load()?)
    }

    #[test]
    fn resolves_shared_and_inline_bootstrap_without_losing_source_config() {
        let s = spec(
            r#"
id: test
bootstrapProviders:
  - id: shared
    kind: noop
sources:
  - {id: one, kind: mock, bootstrapProvider: shared, intervalMs: 250}
  - {id: two, kind: mock, bootstrapProvider: {kind: noop}}
"#,
        )
        .unwrap();
        assert_eq!(s.sources[0].bootstrap.as_ref().unwrap().kind, "noop");
        assert_eq!(s.sources[1].bootstrap.as_ref().unwrap().kind, "noop");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&s.sources[0].config_json).unwrap()
                ["intervalMs"],
            250
        );
    }

    #[test]
    fn preserves_query_options_index_and_capacity_settings() {
        let s = spec(
            r#"
id: test
persistIndex: true
enableArchive: true
memoryBudgetMiB: 64
defaultPriorityQueueCapacity: 111
defaultDispatchBufferCapacity: 222
sources: [{id: s, kind: mock}]
queries:
  - id: q
    query: "MATCH (n) RETURN n"
    sources: [{sourceId: s, nodes: [Sensor]}]
    storageBackend: rocksdb
    enableBootstrap: false
    bootstrapBufferSize: 123
    bootstrapTimeoutSecs: 42
    outboxCapacity: 17
    priorityQueueCapacity: 333
    dispatchBufferCapacity: 444
"#,
        )
        .unwrap();
        assert!(s.enable_archive);
        assert_eq!(s.memory_budget_bytes, Some(64 * 1024 * 1024));
        assert_eq!(s.priority_queue_capacity, Some(111));
        assert_eq!(s.dispatch_buffer_capacity, Some(222));
        let q: drasi_lib::QueryConfig = serde_json::from_str(&s.queries[0].config_json).unwrap();
        assert_eq!(q.bootstrap_buffer_size, 123);
        assert_eq!(q.bootstrap_timeout_secs, 42);
        assert_eq!(q.outbox_capacity, 17);
        assert_eq!(q.priority_queue_capacity, Some(333));
        assert_eq!(q.dispatch_buffer_capacity, Some(444));
        assert!(!q.enable_bootstrap);
        assert_eq!(q.sources[0].nodes, ["Sensor"]);
        assert!(
            matches!(q.storage_backend, Some(StorageBackendRef::Named(ref n)) if n == "rocksdb")
        );
    }

    #[test]
    fn supports_single_explicit_instance_and_inline_memory_archive() {
        let s = spec(
            r#"
instances:
  - id: single
    sources: [{id: s, kind: mock}]
    queries:
      - id: q
        query: "MATCH (n) RETURN n"
        sources: [{sourceId: s}]
        storageBackend: {kind: memory, enableArchive: true}
"#,
        )
        .unwrap();
        assert_eq!(s.instance_id, "single");
        let q: drasi_lib::QueryConfig = serde_json::from_str(&s.queries[0].config_json).unwrap();
        assert!(matches!(
            q.storage_backend,
            Some(StorageBackendRef::Inline(StorageBackendSpec::Memory {
                enable_archive: true
            }))
        ));
    }

    #[test]
    fn rejects_unsupported_or_dangling_configuration() {
        for (yaml, expected) in [
            ("instances: [{id: one}, {id: two}]", "exactly one"),
            ("sources: [{id: s, kind: missing}]", "not in the forge catalog"),
            ("sources: [{id: s, kind: mock, bootstrapProvider: missing}]", "unknown bootstrapProvider"),
            ("sources: [{id: s, kind: mock, identityProvider: missing}]", "unknown identityProvider"),
            ("sources: [{id: s, kind: mock}, {id: s, kind: mock}]", "duplicate source"),
            ("sources: [{id: s, kind: mock}]\nidentityProviders: [{id: p, kind: password}, {id: p, kind: password}]", "duplicate identityProvider"),
            ("sources: [{id: s, kind: mock}]\nreactions: [{id: r, kind: log, queries: [missing]}]", "unknown query"),
            ("sources: [{id: s, kind: mock}]\nqueries: [{id: q, query: 'MATCH (n) RETURN n', sources: [{sourceId: missing}]}]", "unknown source"),
            ("sources: [{id: s, kind: mock}]\nqueries: [{id: q, query: 'MATCH (n) RETURN n', storageBackend: redis}]", "no configured provider"),
            ("sources: [{id: s, kind: mock}]\nqueries: [{id: q, query: 'MATCH (n) RETURN n', storageBackend: rocksdb}]", "persistIndex"),
            ("persistIndex: true\nsources: [{id: s, kind: mock}]\nqueries: [{id: q, query: 'MATCH (n) RETURN n', storageBackend: {kind: rocksdb}}]", "inline plugins"),
        ] {
            let error = spec(yaml).unwrap_err().to_string();
            assert!(error.contains(expected), "{yaml}: {error}");
        }
    }
}
