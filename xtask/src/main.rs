use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<Package>,
    target_directory: PathBuf,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
    features: std::collections::HashMap<String, Vec<String>>,
}

struct DiscoveryResult {
    plugins: Vec<Package>,
    target_directory: PathBuf,
}

fn discover_dynamic_plugins() -> DiscoveryResult {
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

    let plugins = metadata
        .packages
        .into_iter()
        .filter(|p| p.features.contains_key("dynamic-plugin"))
        .collect();

    DiscoveryResult {
        plugins,
        target_directory: metadata.target_directory,
    }
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
            eprintln!("  build-plugins [--release] [--jobs N]  Build all dynamic plugins as cdylib shared libraries");
            eprintln!("  list-plugins                          List all discovered dynamic plugin crates");
            std::process::exit(1);
        }
    }
}

fn list_plugins() {
    let result = discover_dynamic_plugins();
    if result.plugins.is_empty() {
        println!("No dynamic plugins found.");
        return;
    }
    println!("Dynamic plugins ({}):", result.plugins.len());
    for p in &result.plugins {
        println!("  {} ({})", p.name, p.manifest_path.display());
    }
}

fn parse_jobs(args: &[String]) -> usize {
    for (i, arg) in args.iter().enumerate() {
        if arg == "--jobs" || arg == "-j" {
            if let Some(n) = args.get(i + 1) {
                return n.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid --jobs value: {}", n);
                    std::process::exit(1);
                });
            }
        }
    }
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn build_plugins(args: &[String]) {
    let release = args.iter().any(|a| a == "--release");
    let jobs = parse_jobs(args);
    let result = discover_dynamic_plugins();

    if result.plugins.is_empty() {
        println!("No dynamic plugins found.");
        return;
    }

    let mode = if release { "release" } else { "debug" };
    let target_dir = result.target_directory;
    println!(
        "=== Building {} cdylib plugins ({}, {} parallel jobs) ===",
        result.plugins.len(),
        mode,
        jobs
    );

    let failed = Arc::new(AtomicBool::new(false));
    let target_dir = Arc::new(target_dir);
    let plugins: Vec<_> = result
        .plugins
        .into_iter()
        .map(|p| (p.name, p.manifest_path))
        .collect();

    // Process plugins in chunks of `jobs` size
    for chunk in plugins.chunks(jobs) {
        if failed.load(Ordering::Relaxed) {
            break;
        }

        let handles: Vec<_> = chunk
            .iter()
            .map(|(name, manifest)| {
                let name = name.clone();
                let manifest = manifest.clone();
                let failed = Arc::clone(&failed);
                let target_dir = Arc::clone(&target_dir);

                thread::spawn(move || {
                    println!("  Building {}...", name);

                    let mut cmd = Command::new("cargo");
                    cmd.args([
                        "build",
                        "--lib",
                        "--manifest-path",
                        manifest.to_str().expect("invalid manifest path"),
                        "--target-dir",
                        target_dir.to_str().expect("invalid target dir"),
                        "--features",
                        "dynamic-plugin",
                    ]);

                    if release {
                        cmd.arg("--release");
                    }

                    let status = cmd.status().expect("failed to run cargo build");
                    if !status.success() {
                        eprintln!("Failed to build {}", name);
                        failed.store(true, Ordering::Relaxed);
                    } else {
                        println!("  Built {}", name);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("build thread panicked");
        }
    }

    if failed.load(Ordering::Relaxed) {
        eprintln!("=== Plugin build failed ===");
        std::process::exit(1);
    }

    println!("=== cdylib plugins built ===");
}
