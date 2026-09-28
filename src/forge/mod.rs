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

//! `drasi-server forge` — generate a custom, minimal, statically-linked Drasi
//! binary from a Drasi Server configuration file.
//!
//! The forged project is a thin wrapper: it embeds `drasi-lib` plus only the
//! component crates referenced by the config, wiring them through the same
//! component descriptors the server uses. It drops the REST API, web UI, and
//! dynamic plugin loader for a smaller, faster, lower-attack-surface binary.

#![allow(clippy::print_stdout)]

pub mod catalog;
pub mod generate;
pub mod spec;

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use catalog::Catalog;
use generate::{DepMode, GenOptions};

/// Options for a forge run (populated from the CLI).
#[derive(Debug, Clone)]
pub struct ForgeArgs {
    pub config: PathBuf,
    pub out: PathBuf,
    pub name: Option<String>,
    pub sealed: bool,
    pub docker: bool,
    /// Emit a GitHub Actions release workflow + .gitignore for repo output.
    pub ci: bool,
    pub build: bool,
    /// Explicit path to a local `drasi-core` checkout for path dependencies.
    pub local: Option<PathBuf>,
    /// Force crates.io version dependencies instead of local path deps.
    pub crates: bool,
}

/// Published versions compatible with the catalog and current Server query schema.
const CRATES_LIB_VERSION: &str = "=0.9.3";
const CRATES_SDK_VERSION: &str = "=0.11.3";

pub fn run_forge(args: ForgeArgs) -> Result<()> {
    let catalog = Catalog::load()?;

    println!("→ Reading config: {}", args.config.display());
    let spec = spec::derive_spec(&args.config, args.name.as_deref(), &catalog)?;

    let dep = resolve_dep_mode(&args)?;
    describe_dep_mode(&dep);

    for w in &spec.warnings {
        println!("  ⚠ {w}");
    }

    let opts = GenOptions {
        out_dir: args.out.clone(),
        sealed: args.sealed,
        docker: args.docker,
        ci: args.ci,
        dep,
    };

    if args.ci || args.docker {
        if let DepMode::LocalPath(_) = &opts.dep {
            println!(
                "  ⚠ This project uses local path dependencies, which are not available in \
                 Docker build contexts or GitHub runners. Regenerate with --crates for a \
                 portable project (see README / REUSE_GAPS.md)."
            );
        }
    }

    println!(
        "→ Generating project '{}' into {}",
        spec.name,
        args.out.display()
    );
    let generated = generate::generate(&spec, &catalog, &opts)?;
    for f in &generated.files {
        println!("    + {}", display_relative(&args.out, f));
    }

    println!(
        "\nGenerated '{}' with {} source(s), {} query(ies), {} reaction(s){}.",
        spec.name,
        spec.sources.len(),
        spec.queries.len(),
        spec.reactions.len(),
        if opts.sealed { " [sealed]" } else { "" }
    );

    if args.build {
        build_project(&args.out, &spec.name)?;
    } else {
        println!("\nNext:");
        println!("  cd {} && cargo build --release", args.out.display());
    }

    Ok(())
}

fn resolve_dep_mode(args: &ForgeArgs) -> Result<DepMode> {
    if args.crates && args.local.is_some() {
        bail!("--crates and --local cannot be used together");
    }
    if args.crates {
        return Ok(DepMode::Crates {
            lib_version: CRATES_LIB_VERSION.to_string(),
            sdk_version: CRATES_SDK_VERSION.to_string(),
        });
    }
    if let Some(p) = &args.local {
        validate_drasi_core(p)?;
        return Ok(DepMode::LocalPath(p.canonicalize()?));
    }
    // Auto-detect a sibling drasi-core checkout.
    if let Some(p) = autodetect_drasi_core(&args.config) {
        validate_drasi_core(&p)?;
        return Ok(DepMode::LocalPath(p));
    }
    Ok(DepMode::Crates {
        lib_version: CRATES_LIB_VERSION.to_string(),
        sdk_version: CRATES_SDK_VERSION.to_string(),
    })
}

fn validate_drasi_core(p: &Path) -> Result<()> {
    for relative in ["lib/Cargo.toml", "components/plugin-sdk/Cargo.toml"] {
        let manifest = p.join(relative);
        if !manifest.is_file() {
            bail!("--local path {:?} is missing {}", p, manifest.display());
        }
    }
    Ok(())
}

fn autodetect_drasi_core(config: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("drasi-core"));
        candidates.push(cwd.join("../drasi-core"));
    }
    if let Some(dir) = config.parent() {
        candidates.push(dir.join("../drasi-core"));
        candidates.push(dir.join("../../drasi-core"));
    }
    for c in candidates {
        if c.join("lib").join("Cargo.toml").exists() {
            if let Ok(path) = c.canonicalize() {
                return Some(path);
            }
        }
    }
    None
}

fn describe_dep_mode(dep: &DepMode) {
    match dep {
        DepMode::LocalPath(p) => println!("→ Dependency mode: local path deps ({})", p.display()),
        DepMode::Crates {
            lib_version,
            sdk_version,
        } => println!(
            "→ Dependency mode: crates.io (drasi-lib {lib_version}, drasi-plugin-sdk {sdk_version})"
        ),
    }
}

fn build_project(out_dir: &Path, name: &str) -> Result<()> {
    println!("\n→ Building (cargo build --release) …");
    let pkg = generate::sanitize_pkg_name_pub(name);
    let output = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--bin",
            &pkg,
            "--message-format=json-render-diagnostics",
        ])
        .current_dir(out_dir)
        .stderr(Stdio::inherit())
        .output()
        .context("failed to invoke cargo (is it on PATH?)")?;
    if !output.status.success() {
        bail!(
            "cargo build failed for generated project at {}",
            out_dir.display()
        );
    }
    let mut executable = None;
    for line in String::from_utf8(output.stdout)?.lines() {
        let message: serde_json::Value =
            serde_json::from_str(line).context("parsing cargo build output")?;
        if message["reason"] == "compiler-artifact" && message["target"]["name"] == pkg {
            if let Some(path) = message["executable"].as_str() {
                executable = Some(PathBuf::from(path));
            }
        }
    }
    let bin =
        executable.context("cargo build succeeded without reporting the generated executable")?;
    let meta = std::fs::metadata(&bin).with_context(|| format!("reading {}", bin.display()))?;
    println!(
        "✔ Built {} ({:.1} MB)",
        bin.display(),
        meta.len() as f64 / (1024.0 * 1024.0)
    );
    Ok(())
}

fn display_relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> ForgeArgs {
        ForgeArgs {
            config: PathBuf::from("config.yaml"),
            out: PathBuf::from("generated"),
            name: None,
            sealed: false,
            docker: false,
            ci: false,
            build: false,
            local: None,
            crates: true,
        }
    }

    #[test]
    fn crates_mode_is_explicit_and_version_locked() {
        assert!(matches!(resolve_dep_mode(&args()).unwrap(),
            DepMode::Crates { lib_version, sdk_version }
                if lib_version == "=0.9.3" && sdk_version == "=0.11.3"));
    }

    #[test]
    fn rejects_conflicting_modes_and_incomplete_local_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = args();
        args.local = Some(dir.path().to_path_buf());
        assert!(resolve_dep_mode(&args)
            .unwrap_err()
            .to_string()
            .contains("cannot be used together"));
        args.crates = false;
        assert!(resolve_dep_mode(&args)
            .unwrap_err()
            .to_string()
            .contains("lib/Cargo.toml"));
        std::fs::create_dir(dir.path().join("lib")).unwrap();
        std::fs::write(dir.path().join("lib/Cargo.toml"), "").unwrap();
        assert!(resolve_dep_mode(&args)
            .unwrap_err()
            .to_string()
            .contains("plugin-sdk/Cargo.toml"));
    }
}
