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

//! Code generator: turns a [`ForgeSpec`] into a standalone, minimal Cargo
//! project (a thin wrapper over `drasi-lib` + the selected component crates).

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use super::catalog::Catalog;
use super::spec::ForgeSpec;

/// How generated projects reference Drasi crates.
#[derive(Debug, Clone)]
pub enum DepMode {
    /// Path dependencies into a local `drasi-core` checkout. Guarantees the
    /// component crates and `drasi-lib` are version-consistent; the mode used
    /// for locally verifiable builds.
    LocalPath(PathBuf),
    /// crates.io version dependencies (from the catalog + pinned lib/sdk).
    Crates {
        lib_version: String,
        sdk_version: String,
    },
}

#[derive(Debug, Clone)]
pub struct GenOptions {
    pub out_dir: PathBuf,
    pub sealed: bool,
    pub docker: bool,
    /// Emit a `.github/workflows/release.yml` for building release artifacts.
    pub ci: bool,
    pub dep: DepMode,
}

#[derive(Debug, Default)]
pub struct Generated {
    pub files: Vec<PathBuf>,
}

/// A resolved crate dependency line to emit into Cargo.toml.
struct Dep {
    crate_name: String,
    version: String,
    dir: String,
    local_only: bool,
}

pub fn generate(spec: &ForgeSpec, catalog: &Catalog, opts: &GenOptions) -> Result<Generated> {
    if opts.out_dir.exists() && opts.out_dir.read_dir()?.next().is_some() {
        bail!(
            "output directory {} is not empty; choose a new directory to avoid overwriting files",
            opts.out_dir.display()
        );
    }
    let deps = collect_deps(spec, catalog)?;
    for dep in &deps {
        match &opts.dep {
            DepMode::Crates { .. } if dep.local_only => bail!(
                "{} has no compatible published release for this catalog; use --local <drasi-core> instead of --crates",
                dep.crate_name
            ),
            DepMode::LocalPath(core) if !core.join(&dep.dir).join("Cargo.toml").is_file() => bail!(
                "local component {} is missing at {}", dep.crate_name, core.join(&dep.dir).display()
            ),
            _ => {}
        }
    }
    let main_rs = render_main_rs(spec, catalog, opts)?;
    let src_dir = opts.out_dir.join("src");
    fs::create_dir_all(&src_dir).with_context(|| format!("creating {}", src_dir.display()))?;

    let mut gen = Generated::default();

    write_file(
        &opts.out_dir.join("Cargo.toml"),
        &render_cargo_toml(spec, &deps, opts),
        &mut gen,
    )?;
    write_file(&src_dir.join("main.rs"), &main_rs, &mut gen)?;
    write_file(
        &opts.out_dir.join("README.md"),
        &render_readme(spec, opts),
        &mut gen,
    )?;
    write_file(&opts.out_dir.join("REUSE_GAPS.md"), REUSE_GAPS_MD, &mut gen)?;

    if !opts.sealed {
        write_file(
            &opts.out_dir.join("config.sample.yaml"),
            &render_sample_config(spec),
            &mut gen,
        )?;
    }

    if opts.docker {
        write_file(
            &opts.out_dir.join("Dockerfile"),
            &render_dockerfile(spec),
            &mut gen,
        )?;
        write_file(
            &opts.out_dir.join(".dockerignore"),
            "target/\ndata/\n.git/\n*.md\n",
            &mut gen,
        )?;
    }

    // A .gitignore is always useful for the generated code tree.
    write_file(
        &opts.out_dir.join(".gitignore"),
        "/target\n/data\n",
        &mut gen,
    )?;

    if opts.ci {
        let wf_dir = opts.out_dir.join(".github").join("workflows");
        fs::create_dir_all(&wf_dir).with_context(|| format!("creating {}", wf_dir.display()))?;
        write_file(
            &wf_dir.join("release.yml"),
            &render_ci_workflow(spec),
            &mut gen,
        )?;
    }

    Ok(gen)
}

fn write_file(path: &Path, contents: &str, gen: &mut Generated) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    gen.files.push(path.to_path_buf());
    Ok(())
}

fn collect_deps(spec: &ForgeSpec, catalog: &Catalog) -> Result<Vec<Dep>> {
    let mut map: BTreeMap<String, Dep> = BTreeMap::new();
    for s in &spec.sources {
        let e = catalog
            .source(&s.kind)
            .with_context(|| format!("source kind '{}' missing from catalog", s.kind))?;
        map.insert(
            e.crate_name.clone(),
            Dep {
                crate_name: e.crate_name.clone(),
                version: e.version.clone(),
                dir: e.dir.clone(),
                local_only: e.local_only,
            },
        );
    }
    for r in &spec.reactions {
        let e = catalog
            .reaction(&r.kind)
            .with_context(|| format!("reaction kind '{}' missing from catalog", r.kind))?;
        map.insert(
            e.crate_name.clone(),
            Dep {
                crate_name: e.crate_name.clone(),
                version: e.version.clone(),
                dir: e.dir.clone(),
                local_only: e.local_only,
            },
        );
    }
    if spec.persist_index {
        let e = catalog
            .index("rocksdb")
            .context("rocksdb index missing from catalog")?;
        map.insert(
            e.crate_name.clone(),
            Dep {
                crate_name: e.crate_name.clone(),
                version: e.version.clone(),
                dir: e.dir.clone(),
                local_only: e.local_only,
            },
        );
    }
    // Bootstrap providers attached to sources.
    for s in &spec.sources {
        if let Some(b) = &s.bootstrap {
            let e = catalog
                .bootstrapper(&b.kind)
                .with_context(|| format!("bootstrap kind '{}' missing from catalog", b.kind))?;
            map.insert(
                e.crate_name.clone(),
                Dep {
                    crate_name: e.crate_name.clone(),
                    version: e.version.clone(),
                    dir: e.dir.clone(),
                    local_only: e.local_only,
                },
            );
        }
    }
    // Plugin-backed identity providers (the built-in `password` kind needs no crate).
    for ip in &spec.identity_providers {
        if ip.builtin_password {
            continue;
        }
        let e = catalog.identity_provider(&ip.kind).with_context(|| {
            format!("identity provider kind '{}' missing from catalog", ip.kind)
        })?;
        map.insert(
            e.crate_name.clone(),
            Dep {
                crate_name: e.crate_name.clone(),
                version: e.version.clone(),
                dir: e.dir.clone(),
                local_only: e.local_only,
            },
        );
    }
    // Secret store provider.
    if let Some(ss) = &spec.secret_store {
        let e = catalog
            .secret_store(&ss.kind)
            .with_context(|| format!("secret store kind '{}' missing from catalog", ss.kind))?;
        map.insert(
            e.crate_name.clone(),
            Dep {
                crate_name: e.crate_name.clone(),
                version: e.version.clone(),
                dir: e.dir.clone(),
                local_only: e.local_only,
            },
        );
    }
    // Persistent state store (redb).
    if spec.state_store.is_some() {
        let e = catalog
            .state_store("redb")
            .context("redb state store missing from catalog")?;
        map.insert(
            e.crate_name.clone(),
            Dep {
                crate_name: e.crate_name.clone(),
                version: e.version.clone(),
                dir: e.dir.clone(),
                local_only: e.local_only,
            },
        );
    }
    if !spec.sources.is_empty() {
        let e = &catalog.wal;
        map.insert(
            e.crate_name.clone(),
            Dep {
                crate_name: e.crate_name.clone(),
                version: e.version.clone(),
                dir: e.dir.clone(),
                local_only: e.local_only,
            },
        );
    }
    Ok(map.into_values().collect())
}

fn dep_line(dep: &Dep, opts: &GenOptions) -> String {
    match &opts.dep {
        DepMode::LocalPath(core) => {
            let p = core.join(&dep.dir);
            format!(
                "{} = {{ path = {} }}",
                dep.crate_name,
                toml_string(&p.display().to_string())
            )
        }
        DepMode::Crates { .. } => format!("{} = \"={}\"", dep.crate_name, dep.version),
    }
}

fn render_cargo_toml(spec: &ForgeSpec, deps: &[Dep], opts: &GenOptions) -> String {
    let pkg = sanitize_pkg_name(&spec.name);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Auto-generated by `drasi-server forge`. Re-run forge to regenerate."
    );
    let _ = writeln!(out, "[package]");
    let _ = writeln!(out, "name = {pkg:?}");
    let _ = writeln!(out, "version = \"0.1.0\"");
    let _ = writeln!(out, "edition = \"2021\"");
    let _ = writeln!(out, "publish = false");
    out.push('\n');
    let _ = writeln!(
        out,
        "# Detach from any parent workspace so this project builds standalone."
    );
    let _ = writeln!(out, "[workspace]");
    out.push('\n');

    let (lib_dep, sdk_dep) = match &opts.dep {
        DepMode::LocalPath(core) => (
            format!(
                "drasi-lib = {{ path = {}{} }}",
                toml_string(&core.join("lib").display().to_string()),
                lib_features(spec)
            ),
            format!(
                "drasi-plugin-sdk = {{ path = {} }}",
                toml_string(&core.join("components/plugin-sdk").display().to_string())
            ),
        ),
        DepMode::Crates {
            lib_version,
            sdk_version,
        } => (
            format!(
                "drasi-lib = {{ version = {lib_version:?}{} }}",
                lib_features(spec)
            ),
            format!("drasi-plugin-sdk = {sdk_version:?}"),
        ),
    };

    let _ = writeln!(out, "[dependencies]");
    let _ = writeln!(out, "anyhow = \"1\"");
    let _ = writeln!(out, "serde_json = \"1\"");
    let _ = writeln!(
        out,
        "tokio = {{ version = \"1\", features = [\"rt-multi-thread\", \"macros\", \"signal\"] }}"
    );
    let _ = writeln!(
        out,
        "tracing-subscriber = {{ version = \"0.3\", features = [\"env-filter\"] }}"
    );
    if spec.secret_store.is_some() {
        let _ = writeln!(out, "async-trait = \"0.1\"");
    }
    if !opts.sealed {
        // Lightweight runtime override file support (component config by id).
        let _ = writeln!(
            out,
            "serde = {{ version = \"1\", features = [\"derive\"] }}"
        );
        let _ = writeln!(out, "serde_yaml = \"0.9\"");
    }
    let _ = writeln!(out, "{lib_dep}");
    let _ = writeln!(out, "{sdk_dep}");
    for d in deps {
        let _ = writeln!(out, "{}", dep_line(d, opts));
    }
    out.push('\n');
    let _ = writeln!(out, "[[bin]]");
    let _ = writeln!(out, "name = {pkg:?}");
    let _ = writeln!(out, "path = \"src/main.rs\"");
    out.push('\n');
    let _ = writeln!(out, "[profile.release]");
    let _ = writeln!(out, "opt-level = \"z\"   # optimize for size");
    let _ = writeln!(out, "lto = true");
    let _ = writeln!(out, "codegen-units = 1");
    let _ = writeln!(out, "panic = \"abort\"");
    let _ = writeln!(out, "strip = true");
    out
}

fn lib_features(spec: &ForgeSpec) -> String {
    if spec.needs_middleware_all {
        ", features = [\"middleware-all\"]".to_string()
    } else {
        String::new()
    }
}

fn render_main_rs(spec: &ForgeSpec, catalog: &Catalog, opts: &GenOptions) -> Result<String> {
    let mut o = String::new();
    let kinds = spec.component_kinds().join(", ");
    let _ = writeln!(
        o,
        "// AUTO-GENERATED by `drasi-server forge`. Do not edit by hand."
    );
    let _ = writeln!(o, "//");
    let _ = writeln!(o, "// Standalone Drasi binary {:?}.", spec.name);
    let _ = writeln!(o, "// Baked components: {kinds}.");
    let _ = writeln!(
        o,
        "// Regenerate by re-running the forge command that produced this project."
    );
    o.push('\n');

    // Conditional imports for hygiene.
    let _ = writeln!(o, "use drasi_lib::DrasiLib;");
    if !spec.queries.is_empty() {
        let _ = writeln!(o, "use drasi_lib::QueryConfig;");
    }
    if !spec.sources.is_empty() {
        let _ = writeln!(o, "use drasi_plugin_sdk::SourcePluginDescriptor;");
    }
    if !spec.reactions.is_empty() {
        let _ = writeln!(o, "use drasi_plugin_sdk::ReactionPluginDescriptor;");
    }
    if spec.sources.iter().any(|s| s.bootstrap.is_some()) {
        let _ = writeln!(o, "use drasi_plugin_sdk::BootstrapPluginDescriptor;");
    }
    if spec.identity_providers.iter().any(|i| !i.builtin_password) {
        let _ = writeln!(o, "use drasi_plugin_sdk::IdentityProviderPluginDescriptor;");
    }
    if spec.secret_store.is_some() {
        let _ = writeln!(
            o,
            "use drasi_plugin_sdk::descriptor::SecretStorePluginDescriptor;"
        );
    }
    o.push('\n');

    // tracing init
    o.push_str(TRACING_FN);
    o.push('\n');

    if spec.needs_config_resolver() {
        o.push_str(RESOLVE_CFG_STRING_FN);
        o.push('\n');
    }
    if spec.secret_store.is_some() {
        o.push_str(SECRET_RESOLVER);
        o.push('\n');
    }
    if spec.persist_index || !spec.sources.is_empty() {
        // Reuse Server's path encoding without a runtime Server dependency.
        o.push_str(
            include_str!("../instance_paths.rs")
                .split("#[cfg(test)]")
                .next()
                .expect("instance path helper"),
        );
        o.push('\n');
    }

    if !opts.sealed {
        o.push_str(&render_runtime_module(spec));
        o.push('\n');
    }

    let _ = writeln!(o, "#[tokio::main]");
    let _ = writeln!(o, "async fn main() -> anyhow::Result<()> {{");

    if opts.sealed {
        let _ = writeln!(o, "    init_tracing({:?})?;", spec.log_level);
        let _ = writeln!(
            o,
            "    let instance_id = {:?}.to_string();",
            spec.instance_id
        );
    } else {
        let _ = writeln!(o, "    let overrides = runtime::Overrides::load()?;");
        let _ = writeln!(
            o,
            "    let log_level = overrides.log_level({:?});",
            spec.log_level
        );
        let _ = writeln!(o, "    init_tracing(&log_level)?;");
        let _ = writeln!(
            o,
            "    let instance_id = overrides.instance_id({:?});",
            spec.instance_id
        );
    }
    o.push('\n');
    let _ = writeln!(
        o,
        "    let mut builder = DrasiLib::builder().with_id(&instance_id);"
    );
    if let Some(capacity) = spec.priority_queue_capacity {
        let _ = writeln!(
            o,
            "    builder = builder.with_priority_queue_capacity({capacity});"
        );
    }
    if let Some(capacity) = spec.dispatch_buffer_capacity {
        let _ = writeln!(
            o,
            "    builder = builder.with_dispatch_buffer_capacity({capacity});"
        );
    }
    if spec.persist_index || !spec.sources.is_empty() {
        let _ = writeln!(o, "    let data_dir = std::path::PathBuf::from(\"./data\").join(instance_storage_key(&instance_id));");
    }
    if !spec.sources.is_empty() {
        let wal = &catalog.wal;
        let _ = writeln!(o, "    let wal_path = data_dir.join(\"wal\");");
        let _ = writeln!(o, "    std::fs::create_dir_all(&wal_path)?;");
        let _ = writeln!(
            o,
            "    builder = builder.with_wal_provider(std::sync::Arc::new({}::{}::new(&wal_path)));",
            wal.module(),
            wal.provider
        );
    }

    if spec.persist_index {
        let idx = catalog.index("rocksdb").context("rocksdb missing")?;
        o.push('\n');
        let _ = writeln!(
            o,
            "    // Persistent index (RocksDB), used as the default backend so"
        );
        let _ = writeln!(o, "    // every query persists its state across restarts.");
        let _ = writeln!(o, "    let index_path = data_dir.join(\"index\");");
        let _ = writeln!(o, "    std::fs::create_dir_all(&index_path)?;");
        let _ = writeln!(
            o,
            "    let index = {}::{}::new(index_path, {}, false);",
            idx.module(),
            idx.provider,
            spec.enable_archive
        );
        if let Some(bytes) = spec.memory_budget_bytes {
            let _ = writeln!(
                o,
                "    let index = index.with_memory_budget_bytes({bytes})?;"
            );
        }
        let _ = writeln!(
            o,
            "    builder = builder.with_default_index_provider({:?}, std::sync::Arc::new(index));",
            crate::index_provider::PERSISTENT_INDEX_PROVIDER_NAME
        );
    }

    // Register before constructing any descriptors or resolving state/identity values.
    if let Some(ss) = &spec.secret_store {
        let e = catalog
            .secret_store(&ss.kind)
            .context("secret store kind missing")?;
        let _ = writeln!(o, "    {{");
        let _ = writeln!(
            o,
            "        let cfg: serde_json::Value = serde_json::from_str({:?})?;",
            ss.config_json
        );
        let _ = writeln!(
            o,
            "        let provider = {}::{}.create_secret_store(&cfg).await?;",
            e.module(),
            e.descriptor
        );
        let _ = writeln!(o, "        let provider: std::sync::Arc<dyn drasi_lib::secret_store::SecretStoreProvider> = std::sync::Arc::from(provider);");
        let _ = writeln!(o, "        drasi_plugin_sdk::resolver::register_secret_resolver(std::sync::Arc::new(SecretResolver(provider.clone())));");
        let _ = writeln!(
            o,
            "        builder = builder.with_secret_store_provider(provider);"
        );
        let _ = writeln!(o, "    }}");
    }

    // State store (redb) — resolved path at runtime.
    if let Some(ss) = &spec.state_store {
        let e = catalog
            .state_store("redb")
            .context("redb state store missing")?;
        o.push('\n');
        let _ = writeln!(o, "    // ---- persistent state store (redb) ----");
        let _ = writeln!(o, "    {{");
        let _ = writeln!(
            o,
            "        let path_cfg: drasi_plugin_sdk::ConfigValue<String> = serde_json::from_str({:?})?;",
            ss.path_config_json
        );
        let _ = writeln!(
            o,
            "        let path = resolve_cfg_string(&path_cfg).await?;"
        );
        let _ = writeln!(
            o,
            "        if let Some(parent) = std::path::Path::new(&path).parent() {{"
        );
        let _ = writeln!(o, "            if !parent.as_os_str().is_empty() {{");
        let _ = writeln!(o, "                std::fs::create_dir_all(parent)?;");
        let _ = writeln!(o, "            }}");
        let _ = writeln!(o, "        }}");
        let _ = writeln!(
            o,
            "        let provider = {}::{}::new(&path)?;",
            e.module(),
            e.provider
        );
        let _ = writeln!(
            o,
            "        builder = builder.with_state_store_provider(std::sync::Arc::new(provider));"
        );
        let _ = writeln!(o, "    }}");
    }

    // Identity providers — a `{id -> provider}` map referenced by sources/reactions.
    if !spec.identity_providers.is_empty() {
        o.push('\n');
        let _ = writeln!(o, "    // ---- identity providers ----");
        let _ = writeln!(
            o,
            "    let mut identity_providers: std::collections::HashMap<String, std::sync::Arc<dyn drasi_lib::identity::IdentityProvider>> = std::collections::HashMap::new();"
        );
        for ip in &spec.identity_providers {
            let _ = writeln!(o, "    {{");
            let _ = writeln!(
                o,
                "        let cfg: serde_json::Value = serde_json::from_str({:?})?;",
                ip.config_json
            );
            if ip.builtin_password {
                let _ = writeln!(
                    o,
                    "        let username_cv: drasi_plugin_sdk::ConfigValue<String> = serde_json::from_value("
                );
                let _ = writeln!(
                    o,
                    "            cfg.get(\"username\").cloned().ok_or_else(|| anyhow::anyhow!(\"identity provider {{:?}}: missing 'username'\", {:?}))?)?;",
                    ip.id
                );
                let _ = writeln!(
                    o,
                    "        let password_cv: drasi_plugin_sdk::ConfigValue<String> = serde_json::from_value("
                );
                let _ = writeln!(
                    o,
                    "            cfg.get(\"password\").cloned().ok_or_else(|| anyhow::anyhow!(\"identity provider {{:?}}: missing 'password'\", {:?}))?)?;",
                    ip.id
                );
                let _ = writeln!(
                    o,
                    "        let provider: std::sync::Arc<dyn drasi_lib::identity::IdentityProvider> = std::sync::Arc::new("
                );
                let _ = writeln!(
                    o,
                    "            drasi_lib::identity::PasswordIdentityProvider::new(resolve_cfg_string(&username_cv).await?, resolve_cfg_string(&password_cv).await?));"
                );
            } else {
                let e = catalog
                    .identity_provider(&ip.kind)
                    .context("identity provider kind missing")?;
                let _ = writeln!(
                    o,
                    "        let created = {}::{}.create_identity_provider(&cfg).await?;",
                    e.module(),
                    e.descriptor
                );
                let _ = writeln!(
                    o,
                    "        let provider: std::sync::Arc<dyn drasi_lib::identity::IdentityProvider> = std::sync::Arc::from(created);"
                );
            }
            let _ = writeln!(
                o,
                "        identity_providers.insert({:?}.to_string(), provider);",
                ip.id
            );
            let _ = writeln!(o, "    }}");
        }
    }

    // Sources
    if !spec.sources.is_empty() {
        o.push('\n');
        let _ = writeln!(o, "    // ---- sources ----");
    }
    for s in &spec.sources {
        let e = catalog.source(&s.kind).context("source kind missing")?;
        let _ = writeln!(o, "    {{");
        let _ = writeln!(
            o,
            "        let cfg: serde_json::Value = serde_json::from_str({:?})?;",
            s.config_json
        );
        if !opts.sealed {
            let _ = writeln!(
                o,
                "        let cfg = overrides.source_config({:?}, cfg);",
                s.id
            );
        }
        let _ = writeln!(
            o,
            "        let component = {}::descriptor::{}",
            e.module(),
            e.descriptor
        );
        let _ = writeln!(
            o,
            "            .create_source({:?}, &cfg, {})",
            s.id, s.auto_start
        );
        let _ = writeln!(o, "            .await?;");
        if let Some(b) = &s.bootstrap {
            let be = catalog
                .bootstrapper(&b.kind)
                .context("bootstrap kind missing")?;
            let _ = writeln!(
                o,
                "        let bootstrap_cfg: serde_json::Value = serde_json::from_str({:?})?;",
                b.config_json
            );
            let _ = writeln!(
                o,
                "        let bootstrap = {}::descriptor::{}",
                be.module(),
                be.descriptor
            );
            let _ = writeln!(
                o,
                "            .create_bootstrap_provider(&bootstrap_cfg, &cfg)"
            );
            let _ = writeln!(o, "            .await?;");
            let _ = writeln!(
                o,
                "        component.set_bootstrap_provider(bootstrap).await;"
            );
        }
        if let Some(idref) = &s.identity_ref {
            let _ = writeln!(
                o,
                "        let idp = identity_providers.get({idref:?}).cloned()"
            );
            let _ = writeln!(
                o,
                "            .ok_or_else(|| anyhow::anyhow!(\"source references unknown identityProvider {{:?}}\", {idref:?}))?;"
            );
            let _ = writeln!(o, "        component.set_identity_provider(idp).await;");
        }
        let _ = writeln!(o, "        builder = builder.with_source(component);");
        let _ = writeln!(o, "    }}");
    }

    // Queries
    if !spec.queries.is_empty() {
        o.push('\n');
        let _ = writeln!(o, "    // ---- queries ----");
    }
    for q in &spec.queries {
        let _ = writeln!(o, "    {{");
        let _ = writeln!(
            o,
            "        let query: QueryConfig = serde_json::from_str({:?})?;",
            q.config_json
        );
        let _ = writeln!(o, "        builder = builder.with_query(query);");
        let _ = writeln!(o, "    }}");
    }

    // Reactions
    if !spec.reactions.is_empty() {
        o.push('\n');
        let _ = writeln!(o, "    // ---- reactions ----");
    }
    for r in &spec.reactions {
        let e = catalog.reaction(&r.kind).context("reaction kind missing")?;
        let ids = r
            .query_ids
            .iter()
            .map(|q| format!("{q:?}.to_string()"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(o, "    {{");
        let _ = writeln!(
            o,
            "        let cfg: serde_json::Value = serde_json::from_str({:?})?;",
            r.config_json
        );
        if !opts.sealed {
            let _ = writeln!(
                o,
                "        let cfg = overrides.reaction_config({:?}, cfg);",
                r.id
            );
        }
        let _ = writeln!(
            o,
            "        let component = {}::descriptor::{}",
            e.module(),
            e.descriptor
        );
        let _ = writeln!(
            o,
            "            .create_reaction({:?}, vec![{}], &cfg, {})",
            r.id, ids, r.auto_start
        );
        let _ = writeln!(o, "            .await?;");
        if let Some(idref) = &r.identity_ref {
            let _ = writeln!(
                o,
                "        let idp = identity_providers.get({idref:?}).cloned()"
            );
            let _ = writeln!(
                o,
                "            .ok_or_else(|| anyhow::anyhow!(\"reaction references unknown identityProvider {{:?}}\", {idref:?}))?;"
            );
            let _ = writeln!(o, "        component.set_identity_provider(idp).await;");
        }
        let _ = writeln!(o, "        builder = builder.with_reaction(component);");
        let _ = writeln!(o, "    }}");
    }

    o.push('\n');
    let _ = writeln!(o, "    let core = builder.build().await?;");
    let _ = writeln!(o, "    core.start().await?;");
    let _ = writeln!(
        o,
        "    eprintln!(\"drasi standalone binary {{}} (instance '{{}}') started; press Ctrl-C to stop.\", {:?}, instance_id);",
        spec.name
    );
    o.push_str(r#"    #[cfg(unix)]
    {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
"#);
    let _ = writeln!(o, "    core.stop().await?;");
    let _ = writeln!(o, "    Ok(())");
    let _ = writeln!(o, "}}");

    Ok(o)
}

const TRACING_FN: &str = r#"fn init_tracing(level: &str) -> anyhow::Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_new(level)?;
    fmt().with_env_filter(filter).try_init().map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}
"#;

const RESOLVE_CFG_STRING_FN: &str = r#"async fn resolve_cfg_string(
    v: &drasi_plugin_sdk::ConfigValue<String>,
) -> anyhow::Result<String> {
    Ok(drasi_plugin_sdk::mapper::DtoMapper::new().resolve_string(v).await?)
}
"#;

const SECRET_RESOLVER: &str = r#"struct SecretResolver(std::sync::Arc<dyn drasi_lib::secret_store::SecretStoreProvider>);

#[async_trait::async_trait]
impl drasi_plugin_sdk::resolver::ValueResolver for SecretResolver {
    async fn resolve_to_string(
        &self,
        value: &drasi_plugin_sdk::ConfigValue<String>,
    ) -> Result<String, drasi_plugin_sdk::resolver::ResolverError> {
        use drasi_plugin_sdk::{ConfigValue, resolver::ResolverError};
        match value {
            ConfigValue::Secret { name } => self.0.get_secret(name).await
                .map_err(|e| ResolverError::SecretResolutionFailed(e.to_string())),
            _ => Err(ResolverError::WrongResolverType),
        }
    }
}
"#;

/// The runtime override module baked into non-sealed binaries. Kept small and
/// dependency-light; see REUSE_GAPS.md for turning this into a reusable crate.
fn render_runtime_module(spec: &ForgeSpec) -> String {
    let source_ids: Vec<_> = spec.sources.iter().map(|s| s.id.as_str()).collect();
    let reaction_ids: Vec<_> = spec.reactions.iter().map(|r| r.id.as_str()).collect();
    let body = r##"mod runtime {
    //! Minimal runtime overrides: baked defaults, overridable via CLI flags,
    //! environment variables, and an optional YAML file. Precedence for scalar
    //! values is CLI > env > file > baked default. Component config blocks may
    //! be overridden wholesale by id via the file; individual fields inside a
    //! component config can additionally use `ConfigValue` env references, which
    //! the component resolves at runtime.
    use std::collections::HashMap;

    #[derive(Debug, Default, serde::Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    struct FileOverrides {
        id: Option<String>,
        log_level: Option<String>,
        sources: HashMap<String, serde_json::Value>,
        reactions: HashMap<String, serde_json::Value>,
    }

    pub struct Overrides {
        cli_id: Option<String>,
        cli_log_level: Option<String>,
        file: FileOverrides,
    }

    impl Overrides {
        pub fn load() -> anyhow::Result<Self> {
            let mut cli_id = None;
            let mut cli_log_level = None;
            let mut config_path: Option<String> = std::env::var("DRASI_CONFIG").ok();

            let mut args = std::env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--id" | "--log-level" | "--config" | "-c" => {
                        let value = args.next().filter(|v| !v.starts_with('-'))
                            .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                        match arg.as_str() {
                            "--id" => cli_id = Some(value),
                            "--log-level" => cli_log_level = Some(value),
                            _ => config_path = Some(value),
                        }
                    }
                    "-h" | "--help" => {
                        eprintln!(
                            "Usage: [--id <id>] [--log-level <level>] [--config <file.yaml>]\n\
                             Env: DRASI_INSTANCE_ID, DRASI_LOG_LEVEL/RUST_LOG, DRASI_CONFIG"
                        );
                        std::process::exit(0);
                    }
                    other => {
                        anyhow::bail!("unrecognized argument '{other}'");
                    }
                }
            }

            let file: FileOverrides = match config_path {
                Some(p) => {
                    let text = std::fs::read_to_string(&p)
                        .map_err(|e| anyhow::anyhow!("reading config '{p}': {e}"))?;
                    serde_yaml::from_str(&text)
                        .map_err(|e| anyhow::anyhow!("parsing config '{p}': {e}"))?
                }
                None => FileOverrides::default(),
            };
            let source_ids = super::SOURCE_IDS;
            let reaction_ids = super::REACTION_IDS;
            for (configs, ids) in [(&file.sources, source_ids), (&file.reactions, reaction_ids)] {
                for (id, cfg) in configs {
                    anyhow::ensure!(ids.contains(&id.as_str()), "unknown component override '{id}'");
                    anyhow::ensure!(cfg.is_object(), "config override for '{id}' must be an object");
                }
            }

            Ok(Self { cli_id, cli_log_level, file })
        }

        pub fn instance_id(&self, baked: &str) -> String {
            self.cli_id
                .clone()
                .or_else(|| std::env::var("DRASI_INSTANCE_ID").ok())
                .or_else(|| self.file.id.clone())
                .unwrap_or_else(|| baked.to_string())
        }

        pub fn log_level(&self, baked: &str) -> String {
            self.cli_log_level
                .clone()
                .or_else(|| std::env::var("DRASI_LOG_LEVEL").ok())
                .or_else(|| std::env::var("RUST_LOG").ok())
                .or_else(|| self.file.log_level.clone())
                .unwrap_or_else(|| baked.to_string())
        }

        pub fn source_config(&self, id: &str, baked: serde_json::Value) -> serde_json::Value {
            self.file.sources.get(id).cloned().unwrap_or(baked)
        }

        pub fn reaction_config(&self, id: &str, baked: serde_json::Value) -> serde_json::Value {
            self.file.reactions.get(id).cloned().unwrap_or(baked)
        }
    }
}
"##;
    format!("const SOURCE_IDS: &[&str] = &{source_ids:?};\nconst REACTION_IDS: &[&str] = &{reaction_ids:?};\n{body}")
}

fn render_readme(spec: &ForgeSpec, opts: &GenOptions) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# {}", spec.name);
    o.push('\n');
    let _ = writeln!(
        o,
        "A custom, minimal, statically-linked **Drasi** binary generated by `drasi-server forge`. \
         It embeds only the components below and drops the Drasi Server API, web UI, and dynamic \
         plugin loader for a smaller, faster, lower-attack-surface deployment."
    );
    o.push('\n');
    let _ = writeln!(o, "## Baked-in components\n");
    let _ = writeln!(o, "- Instance id (default): `{}`", spec.instance_id);
    let _ = writeln!(o, "- Log level (default): `{}`", spec.log_level);
    let _ = writeln!(
        o,
        "- Persistent index (RocksDB): {}",
        if spec.persist_index {
            "yes"
        } else {
            "no (in-memory)"
        }
    );
    if spec.state_store.is_some() {
        let _ = writeln!(o, "- Persistent state store: `redb`");
    }
    if let Some(ss) = &spec.secret_store {
        let _ = writeln!(o, "- Secret store: kind `{}`", ss.kind);
    }
    if !spec.identity_providers.is_empty() {
        let _ = writeln!(o, "- Identity providers:");
        for ip in &spec.identity_providers {
            let _ = writeln!(o, "  - `{}` (kind `{}`)", ip.id, ip.kind);
        }
    }
    if !spec.sources.is_empty() {
        let _ = writeln!(o, "- Sources:");
        for s in &spec.sources {
            let mut extra = String::new();
            if let Some(b) = &s.bootstrap {
                extra.push_str(&format!(", bootstrap `{}`", b.kind));
            }
            if let Some(idref) = &s.identity_ref {
                extra.push_str(&format!(", identity `{idref}`"));
            }
            let _ = writeln!(o, "  - `{}` (kind `{}`{extra})", s.id, s.kind);
        }
    }
    if !spec.queries.is_empty() {
        let _ = writeln!(o, "- Queries:");
        for q in &spec.queries {
            let _ = writeln!(o, "  - `{}`", q.id);
        }
    }
    if !spec.reactions.is_empty() {
        let _ = writeln!(o, "- Reactions:");
        for r in &spec.reactions {
            let extra = match &r.identity_ref {
                Some(idref) => format!(", identity `{idref}`"),
                None => String::new(),
            };
            let _ = writeln!(o, "  - `{}` (kind `{}`{extra})", r.id, r.kind);
        }
    }
    o.push('\n');
    let _ = writeln!(o, "## Build\n");
    let _ = writeln!(o, "```sh");
    let _ = writeln!(o, "cargo build --release");
    let _ = writeln!(o, "```\n");
    let _ = writeln!(
        o,
        "The optimized binary is written to `target/release/{}`.",
        sanitize_pkg_name(&spec.name)
    );
    let _ = writeln!(o, "\nUse Rust 1.95 or newer. Keep `Cargo.lock` after the first build for reproducibility. Components may require native libraries: Clang/CMake for RocksDB, libjq for jq middleware, protoc for gRPC, and system TLS/database libraries.");
    let _ = writeln!(o, "\nQueryConfig is embedded in full. Regenerate with a compatible catalog when upgrading Drasi dependencies. There is no Server REST/UI, application API, or dynamic plugin loader. Selected components can still expose their own network services.");
    let _ = writeln!(o, "\nSource WAL and persistent indexes use `./data/id-<hex-encoded-instance-id>/wal` and `/index`; the state-store path is configured separately. Changing the instance id selects a different WAL/index directory.");
    let _ = writeln!(o, "\n**Configuration is embedded in source and the executable.** Use environment/secret references instead of literal credentials; do not publish sensitive configs or generated output.");
    if let DepMode::LocalPath(_) = opts.dep {
        o.push('\n');
        let _ = writeln!(
            o,
            "> **Note:** this project was generated with **local path dependencies** into a \
             `drasi-core` checkout, so its `Cargo.toml` references absolute local paths and only \
             builds on the machine that generated it. Regenerate with `--crates` for a portable \
             project (see REUSE_GAPS.md)."
        );
    }
    o.push('\n');
    let _ = writeln!(o, "## Run\n");
    let _ = writeln!(o, "```sh");
    let _ = writeln!(o, "./target/release/{}", sanitize_pkg_name(&spec.name));
    let _ = writeln!(o, "```\n");

    if opts.sealed {
        let _ = writeln!(
            o,
            "This is a **sealed** build: the runtime override CLI, environment layer, and config \
             file loader are omitted. Logging uses the compiled-in level. Component ConfigValue \
             environment/secret references and external resources (such as the secrets file) \
             still resolve at runtime; sealed does not mean hermetic."
        );
    } else {
        let _ = writeln!(o, "### Configuration & overrides\n");
        let _ = writeln!(o, "Baked values are the defaults. Override at runtime (precedence: **CLI > env > file > baked**):\n");
        let _ = writeln!(
            o,
            "- CLI: `--id <id>`, `--log-level <level>`, `--config <file.yaml>`"
        );
        let _ = writeln!(
            o,
            "- Env: `DRASI_INSTANCE_ID`, `DRASI_LOG_LEVEL` (or `RUST_LOG`), `DRASI_CONFIG`"
        );
        let _ = writeln!(o, "- File: see `config.sample.yaml` — overrides the instance id, log level, and per-component config blocks by id.");
        o.push('\n');
        let _ = writeln!(
            o,
            "Individual fields inside a component's config can also reference environment variables \
             via Drasi `ConfigValue` syntax; those are resolved at runtime by the component itself."
        );
        o.push('\n');
        let _ = writeln!(o, "```sh");
        let _ = writeln!(
            o,
            "./target/release/{} --config config.sample.yaml --log-level debug",
            sanitize_pkg_name(&spec.name)
        );
        let _ = writeln!(o, "```");
    }
    o.push('\n');

    if opts.docker {
        let _ = writeln!(o, "## Docker\n");
        let _ = writeln!(o, "Requires portable `--crates` dependencies (local paths outside the build context cannot be copied). Mount external files required by components and a writable `/app/data` volume for persistence. Extend the image's native packages for component-specific requirements.\n");
        let _ = writeln!(o, "```sh");
        let _ = writeln!(o, "docker build -t {} .", sanitize_pkg_name(&spec.name));
        if opts.sealed {
            let _ = writeln!(o, "docker run --rm {}", sanitize_pkg_name(&spec.name));
        } else {
            let _ = writeln!(
                o,
                "docker run --rm -e DRASI_LOG_LEVEL=debug {}",
                sanitize_pkg_name(&spec.name)
            );
        }
        let _ = writeln!(o, "```\n");
    }

    if opts.ci {
        let _ = writeln!(o, "## Publish to GitHub\n");
        let _ = writeln!(o, "This project includes a GitHub Actions workflow at `.github/workflows/release.yml` that builds release artifacts for Linux and macOS when you push a `v*` tag.\n");
        let _ = writeln!(o, "```sh");
        let _ = writeln!(
            o,
            "git init && git add . && git commit -m \"Initial standalone Drasi project\""
        );
        let _ = writeln!(
            o,
            "gh repo create <owner>/{} --private --source . --push",
            sanitize_pkg_name(&spec.name)
        );
        let _ = writeln!(
            o,
            "git tag v0.1.0 && git push --tags   # triggers the release build"
        );
        let _ = writeln!(o, "```\n");
        if let DepMode::LocalPath(_) = opts.dep {
            let _ = writeln!(o, "> The workflow requires portable dependencies: regenerate with `--crates` (or vendor `drasi-core`) before pushing, or CI will fail to resolve the local path dependencies.\n");
        }
    }

    if !spec.warnings.is_empty() {
        let _ = writeln!(o, "## Notes from generation\n");
        for w in &spec.warnings {
            let _ = writeln!(o, "- {w}");
        }
        o.push('\n');
    }

    let _ = writeln!(o, "---\n");
    let _ = writeln!(o, "Generated by `drasi-server forge`. See `REUSE_GAPS.md` for known limitations and upstream improvement candidates.");
    o
}

fn render_sample_config(spec: &ForgeSpec) -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "# Sample runtime configuration for the generated binary {:?}.",
        spec.name
    );
    let _ = writeln!(o, "#");
    let _ = writeln!(
        o,
        "# Everything here is OPTIONAL: the binary already has these values baked in as"
    );
    let _ = writeln!(
        o,
        "# defaults. Provide this file with `--config <file>` (or the DRASI_CONFIG env"
    );
    let _ = writeln!(
        o,
        "# var) to override. Precedence: CLI flags > env vars > this file > baked defaults."
    );
    let _ = writeln!(o, "#");
    let _ = writeln!(
        o,
        "# Per-component `config` blocks are matched by id and override the baked config"
    );
    let _ = writeln!(
        o,
        "# for that component wholesale. Field-level docs for each component kind are"
    );
    let _ = writeln!(
        o,
        "# available via `drasi-server validate` and the Drasi component docs (offline"
    );
    let _ = writeln!(
        o,
        "# per-field schemas are a known gap — see REUSE_GAPS.md)."
    );
    o.push('\n');
    let _ = writeln!(o, "# Instance identity and logging");
    let _ = writeln!(o, "id: {}", yaml_scalar(&spec.instance_id));
    let _ = writeln!(
        o,
        "logLevel: {}   # trace | debug | info | warn | error",
        yaml_scalar(&spec.log_level)
    );
    o.push('\n');

    if !spec.sources.is_empty() {
        let _ = writeln!(
            o,
            "# Source component configs (baked values shown; edit to override)."
        );
        let _ = writeln!(
            o,
            "# Values are JSON (valid YAML); a whole block replaces the baked config for that id."
        );
        let _ = writeln!(o, "sources:");
        for s in &spec.sources {
            let _ = writeln!(o, "  # kind: {}", s.kind);
            let _ = writeln!(o, "  {}: {}", yaml_scalar(&s.id), s.config_json);
        }
        o.push('\n');
    }
    if !spec.reactions.is_empty() {
        let _ = writeln!(
            o,
            "# Reaction component configs (baked values shown; edit to override)."
        );
        let _ = writeln!(o, "reactions:");
        for r in &spec.reactions {
            let _ = writeln!(
                o,
                "  # kind: {} | subscribes to queries: {:?}",
                r.kind, r.query_ids
            );
            let _ = writeln!(o, "  {}: {}", yaml_scalar(&r.id), r.config_json);
        }
        o.push('\n');
    }
    if spec.state_store.is_some()
        || spec.secret_store.is_some()
        || !spec.identity_providers.is_empty()
    {
        let _ = writeln!(
            o,
            "# Note: this binary also bakes in the following, which are compiled in and"
        );
        let _ = writeln!(
            o,
            "# NOT overridable via this file (regenerate the binary to change them):"
        );
        if spec.state_store.is_some() {
            let _ = writeln!(o, "#   - state store: redb");
        }
        if let Some(ss) = &spec.secret_store {
            let _ = writeln!(o, "#   - secret store: {}", ss.kind);
        }
        for ip in &spec.identity_providers {
            let _ = writeln!(o, "#   - identity provider: {} (kind {})", ip.id, ip.kind);
        }
        let _ = writeln!(
            o,
            "# Fields inside baked configs may still read ${{ENV}} references at runtime."
        );
        o.push('\n');
    }
    o
}

fn render_dockerfile(spec: &ForgeSpec) -> String {
    let pkg = sanitize_pkg_name(&spec.name);
    format!(
        r#"# Auto-generated by `drasi-server forge`.
# Multi-stage build producing a small runtime image for the forged binary.
FROM rust:1.95-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends clang cmake pkg-config libssl-dev libjq-dev protobuf-compiler librdkafka-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release --bin {pkg}

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 libjq1 librdkafka1 && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 drasi && mkdir -p /app/data && chown -R drasi:drasi /app
WORKDIR /app
COPY --from=build /app/target/release/{pkg} /usr/local/bin/{pkg}
USER drasi
ENTRYPOINT ["/usr/local/bin/{pkg}"]
"#
    )
}

const REUSE_GAPS_MD: &str = include_str!("reuse_gaps.md");

fn render_ci_workflow(spec: &ForgeSpec) -> String {
    let pkg = sanitize_pkg_name(&spec.name);
    format!(
        r#"# Auto-generated by `drasi-server forge`.
# Builds release artifacts for the forged binary on tag pushes.
#
# NOTE: This workflow requires the project to use crates.io dependencies
# (generate with `--crates`) or to vendor `drasi-core`. A project generated
# with local path dependencies will NOT build on a GitHub runner, because the
# absolute local paths in Cargo.toml do not exist there.
name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: macos-latest
            target: aarch64-apple-darwin
    runs-on: ${{{{ matrix.os }}}}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.95.0
        with:
          targets: ${{{{ matrix.target }}}}
      - name: Native dependencies (Linux)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y clang cmake pkg-config libssl-dev libjq-dev protobuf-compiler librdkafka-dev
      - name: Native dependencies (macOS)
        if: runner.os == 'macOS'
        run: brew install cmake pkg-config openssl jq protobuf librdkafka
      - name: Build
        run: cargo build --release --target ${{{{ matrix.target }}}}
      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: {pkg}-${{{{ matrix.target }}}}
          path: target/${{{{ matrix.target }}}}/release/{pkg}
"#
    )
}

// ---- small formatting helpers ----
pub fn sanitize_pkg_name_pub(name: &str) -> String {
    sanitize_pkg_name(name)
}

fn sanitize_pkg_name(name: &str) -> String {
    let mut s: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-').to_string();
    match s.chars().next() {
        None => "drasi-forged".to_string(),
        Some(c) if c.is_ascii_digit() => format!("drasi-{s}"),
        Some(_) => s,
    }
}

fn yaml_scalar(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::spec::derive_spec_from_config;

    const YAML: &str = r#"
id: forge-test
persistIndex: true
enableArchive: false
memoryBudgetMiB: 64
bootstrapProviders: [{id: empty, kind: noop}]
sources: [{id: s, kind: mock, bootstrapProvider: empty, identityProvider: login}]
queries:
  - {id: q, query: "MATCH (n) RETURN n", sources: [{sourceId: s}], storageBackend: rocksdb}
reactions: [{id: r, kind: log, queries: [q], identityProvider: login}]
identityProviders: [{id: login, kind: password, username: "${USER:-test}", password: "${PASSWORD:-test}"}]
stateStore: {kind: redb, path: "./data/state.redb"}
secretStore: {kind: file, path: "./secrets.json"}
"#;

    fn options(out_dir: PathBuf, sealed: bool) -> GenOptions {
        GenOptions {
            out_dir,
            sealed,
            docker: true,
            ci: true,
            dep: DepMode::Crates {
                lib_version: super::super::CRATES_LIB_VERSION.into(),
                sdk_version: super::super::CRATES_SDK_VERSION.into(),
            },
        }
    }

    #[test]
    fn generates_current_wiring_and_only_selected_dependencies() {
        let cat = Catalog::load().unwrap();
        let spec =
            derive_spec_from_config(&serde_yaml::from_str(YAML).unwrap(), None, &cat).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let opts = options(dir.path().to_path_buf(), false);
        generate(&spec, &cat, &opts).unwrap();
        let main = fs::read_to_string(dir.path().join("src/main.rs")).unwrap();
        for text in [
            "mod runtime",
            "with_wal_provider",
            "with_default_index_provider(\"rocksdb\"",
            "new(index_path, false, false)",
            "with_memory_budget_bytes(67108864)",
            "NoOpBootstrapDescriptor",
            "PasswordIdentityProvider",
            "with_state_store_provider",
            "register_secret_resolver",
            "with_secret_store_provider",
        ] {
            assert!(main.contains(text), "missing {text}");
        }
        let manifest: toml::Value = fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        let deps = manifest["dependencies"].as_table().unwrap();
        for name in [
            "drasi-source-mock",
            "drasi-reaction-log",
            "drasi-bootstrap-noop",
            "drasi-index-rocksdb",
            "drasi-state-store-redb",
            "drasi-secret-store-file",
            "drasi-wal-redb",
        ] {
            assert!(deps.contains_key(name));
        }
        for name in [
            "drasi-server",
            "drasi-host-sdk",
            "drasi-source-postgres",
            "axum",
        ] {
            assert!(!deps.contains_key(name));
        }
        assert!(dir.path().join("config.sample.yaml").exists());
        assert!(dir.path().join("Dockerfile").exists());
        assert!(dir.path().join(".github/workflows/release.yml").exists());
    }

    #[test]
    fn sealed_omits_override_loader_and_sample_but_keeps_component_resolution() {
        let cat = Catalog::load().unwrap();
        let spec =
            derive_spec_from_config(&serde_yaml::from_str(YAML).unwrap(), None, &cat).unwrap();
        let dir = tempfile::tempdir().unwrap();
        generate(&spec, &cat, &options(dir.path().to_path_buf(), true)).unwrap();
        let main = fs::read_to_string(dir.path().join("src/main.rs")).unwrap();
        assert!(!main.contains("mod runtime"));
        assert!(!main.contains("Overrides"));
        assert!(!main.contains("RUST_LOG"));
        assert!(main.contains("resolve_cfg_string"));
        assert!(!dir.path().join("config.sample.yaml").exists());
        let manifest: toml::Value = fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(!manifest["dependencies"]
            .as_table()
            .unwrap()
            .contains_key("serde_yaml"));
    }

    #[test]
    fn refuses_overwrite_and_unpublished_components_before_writing() {
        let cat = Catalog::load().unwrap();
        let config = serde_yaml::from_str("sources: [{id: s, kind: here-traffic}]").unwrap();
        let spec = derive_spec_from_config(&config, None, &cat).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let opts = options(dir.path().to_path_buf(), false);
        assert!(generate(&spec, &cat, &opts)
            .unwrap_err()
            .to_string()
            .contains("--local"));
        assert_eq!(dir.path().read_dir().unwrap().count(), 0);
        fs::write(dir.path().join("keep.txt"), "user content").unwrap();
        assert!(generate(&spec, &cat, &opts)
            .unwrap_err()
            .to_string()
            .contains("not empty"));
        assert_eq!(
            fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
            "user content"
        );
    }

    #[test]
    fn sample_yaml_quotes_special_ids_and_values() {
        let cat = Catalog::load().unwrap();
        let config =
            serde_yaml::from_str("id: \"a\\u001b\"\nsources: [{id: 'true', kind: mock}]").unwrap();
        let spec = derive_spec_from_config(&config, None, &cat).unwrap();
        let sample: serde_json::Value = serde_yaml::from_str(&render_sample_config(&spec)).unwrap();
        assert_eq!(sample["id"], "a\u{001b}");
        assert_eq!(sample["sources"]["true"], serde_json::json!({}));
    }

    #[test]
    fn runtime_rejects_invalid_overrides_and_honors_logging_precedence() {
        let cat = Catalog::load().unwrap();
        let spec =
            derive_spec_from_config(&serde_yaml::from_str(YAML).unwrap(), None, &cat).unwrap();
        let code = render_runtime_module(&spec);
        assert!(code.contains("deny_unknown_fields"));
        assert!(code.contains("unknown component override"));
        assert!(code.contains("requires a value"));
        assert!(code.contains("unrecognized argument"));
        assert!(
            code.find("DRASI_LOG_LEVEL\").ok()").unwrap() < code.find("RUST_LOG\").ok()").unwrap()
        );
    }
}
