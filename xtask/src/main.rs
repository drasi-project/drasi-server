//! Build orchestration tasks for Drasi Server.
//!
//! This is a [cargo xtask](https://github.com/matklad/cargo-xtask) binary.
//!
//! Usage:
//!   cargo xtask build-plugins     Build plugin shared libraries into ./plugins/
//!   cargo xtask build-dynamic     Build server (no static plugins) + plugin shared libraries
//!   cargo xtask clean-plugins     Remove the ./plugins/ directory

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

/// Plugin crates that can be built as shared libraries.
///
/// These must match the optional dependencies gated behind `builtin-plugins`
/// in the root Cargo.toml. Core plugins (noop, application) are always
/// statically linked and are NOT in this list.
const PLUGIN_CRATES: &[&str] = &[
    "drasi-source-mock",
    "drasi-source-http",
    "drasi-source-grpc",
    "drasi-source-postgres",
    "drasi-source-mssql",
    "drasi-bootstrap-postgres",
    "drasi-bootstrap-scriptfile",
    "drasi-bootstrap-mssql",
    "drasi-reaction-log",
    "drasi-reaction-http",
    "drasi-reaction-http-adaptive",
    "drasi-reaction-grpc",
    "drasi-reaction-grpc-adaptive",
    "drasi-reaction-sse",
    "drasi-reaction-profiler",
    "drasi-reaction-storedproc-postgres",
    "drasi-reaction-storedproc-mysql",
    "drasi-reaction-storedproc-mssql",
];

fn main() {
    let args: Vec<String> = env::args().collect();
    let task = args.get(1).map(String::as_str);

    match task {
        Some("build-plugins") => build_plugins(is_release(&args)),
        Some("build-dynamic") => build_dynamic(is_release(&args)),
        Some("clean-plugins") => clean_plugins(),
        _ => print_help(),
    }
}

fn is_release(args: &[String]) -> bool {
    args.iter().any(|a| a == "--release")
}

fn print_help() {
    eprintln!(
        "\
Drasi Server build tasks

Usage: cargo xtask <TASK> [OPTIONS]

Tasks:
  build-plugins     Build plugin shared libraries into ./plugins/
  build-dynamic     Build server (dynamic only) + plugin shared libraries
  clean-plugins     Remove the ./plugins/ directory

Options:
  --release         Build in release mode (default: debug)"
    );
    process::exit(1);
}

/// Build all plugin crates as shared libraries and copy them to ./plugins/.
fn build_plugins(release: bool) {
    let project_root = project_root();
    let plugins_dir = project_root.join("plugins");

    fs::create_dir_all(&plugins_dir).expect("Failed to create plugins/ directory");

    // Build all plugin crates via the workspace dependency graph.
    // The `builtin-plugins` feature must be enabled so the optional deps are resolvable.
    println!("Building plugin shared libraries...");
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&project_root);
    cmd.args(["build", "--features", "builtin-plugins"]);

    if release {
        cmd.arg("--release");
    }

    for crate_name in PLUGIN_CRATES {
        cmd.args(["-p", crate_name]);
    }

    let status = cmd.status().expect("Failed to run cargo build");
    if !status.success() {
        eprintln!("cargo build failed");
        process::exit(1);
    }

    // Copy .so/.dylib/.dll files to ./plugins/
    let target_dir = project_root
        .join("target")
        .join(if release { "release" } else { "debug" });

    let extensions: &[&str] = if cfg!(target_os = "macos") {
        &["dylib"]
    } else if cfg!(target_os = "windows") {
        &["dll"]
    } else {
        &["so"]
    };

    println!("Packaging plugins into ./plugins/...");
    let mut count = 0;
    for crate_name in PLUGIN_CRATES {
        let lib_name = crate_name.replace('-', "_");
        for ext in extensions {
            let filename = format!("lib{lib_name}.{ext}");
            let src = target_dir.join(&filename);
            if src.exists() {
                let dst = plugins_dir.join(&filename);
                fs::copy(&src, &dst).unwrap_or_else(|e| {
                    panic!("Failed to copy {} to {}: {}", src.display(), dst.display(), e)
                });
                println!("  {filename}");
                count += 1;
            }
        }
    }

    println!("Done. {count} plugins in {}", plugins_dir.display());
}

/// Build the server without static plugins, then build and package dynamic plugins.
fn build_dynamic(release: bool) {
    build_plugins(release);

    println!("\nBuilding drasi-server (dynamic plugins only)...");
    let project_root = project_root();
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&project_root);
    cmd.args(["build", "--no-default-features"]);

    if release {
        cmd.arg("--release");
    }

    let status = cmd.status().expect("Failed to run cargo build");
    if !status.success() {
        eprintln!("cargo build failed");
        process::exit(1);
    }

    let binary = if release {
        "target/release/drasi-server"
    } else {
        "target/debug/drasi-server"
    };

    println!("\nDone. Run with:");
    println!("  ./{binary} --config config/server.yaml --plugins-dir ./plugins");
}

/// Remove the ./plugins/ directory.
fn clean_plugins() {
    let plugins_dir = project_root().join("plugins");
    if plugins_dir.exists() {
        fs::remove_dir_all(&plugins_dir).expect("Failed to remove plugins/ directory");
        println!("Removed {}", plugins_dir.display());
    } else {
        println!("Nothing to clean — plugins/ does not exist");
    }
}

/// Find the workspace root (the directory containing the top-level Cargo.toml).
fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be in a subdirectory of the project root")
        .to_path_buf()
}
