use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
    features: std::collections::HashMap<String, Vec<String>>,
}

fn discover_dynamic_plugins() -> Vec<Package> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .expect("failed to run cargo metadata");

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).expect("failed to parse cargo metadata");

    metadata
        .packages
        .into_iter()
        .filter(|p| p.features.contains_key("dynamic-plugin"))
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let subcommand = args.get(1).map(|s| s.as_str());

    match subcommand {
        Some("build-plugins") => build_plugins(&args[2..]),
        Some("list-plugins") => list_plugins(),
        _ => {
            eprintln!("Usage: cargo xtask <command>");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  build-plugins [--release]   Build all dynamic plugins as cdylib shared libraries");
            eprintln!("  list-plugins                List all discovered dynamic plugin crates");
            std::process::exit(1);
        }
    }
}

fn list_plugins() {
    let plugins = discover_dynamic_plugins();
    if plugins.is_empty() {
        println!("No dynamic plugins found.");
        return;
    }
    println!("Dynamic plugins ({}):", plugins.len());
    for p in &plugins {
        println!("  {} ({})", p.name, p.manifest_path.display());
    }
}

fn build_plugins(args: &[String]) {
    let release = args.iter().any(|a| a == "--release");
    let plugins = discover_dynamic_plugins();

    if plugins.is_empty() {
        println!("No dynamic plugins found.");
        return;
    }

    let mode = if release { "release" } else { "debug" };
    println!("=== Building {} cdylib plugins ({}) ===", plugins.len(), mode);

    for plugin in &plugins {
        println!("  Building {}...", plugin.name);

        let mut cmd = Command::new("cargo");
        cmd.args([
            "build",
            "--lib",
            "--manifest-path",
            plugin.manifest_path.to_str().expect("invalid manifest path"),
            "--features",
            "dynamic-plugin",
        ]);

        if release {
            cmd.arg("--release");
        }

        let status = cmd.status().expect("failed to run cargo build");
        if !status.success() {
            eprintln!("Failed to build {}", plugin.name);
            std::process::exit(1);
        }
    }

    println!("=== cdylib plugins built ===");
}
