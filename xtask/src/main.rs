use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
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
            eprintln!("  build-plugins [--release] [--jobs N] [--target TRIPLE]  Build all dynamic plugins as cdylib shared libraries");
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

fn plugin_lib_name(crate_name: &str, target: Option<&str>) -> String {
    let base = crate_name.replace('-', "_");
    let is_windows = target
        .map(|t| t.contains("windows"))
        .unwrap_or(cfg!(target_os = "windows"));
    if is_windows {
        base
    } else {
        format!("lib{base}")
    }
}

fn plugin_lib_ext(target: Option<&str>) -> &'static str {
    let triple = target.unwrap_or("");
    if triple.contains("windows") {
        "dll"
    } else if triple.contains("apple") || triple.contains("darwin") {
        "dylib"
    } else if !triple.is_empty() {
        "so"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

fn parse_target(args: &[String]) -> Option<String> {
    for (i, arg) in args.iter().enumerate() {
        if arg == "--target" {
            if let Some(t) = args.get(i + 1) {
                return Some(t.clone());
            }
        }
    }
    None
}

fn build_plugins(args: &[String]) {
    let release = args.iter().any(|a| a == "--release");
    let jobs = parse_jobs(args);
    let target = parse_target(args);
    let result = discover_dynamic_plugins();

    if result.plugins.is_empty() {
        println!("No dynamic plugins found.");
        return;
    }

    let mode = if release { "release" } else { "debug" };
    let target_dir = result.target_directory;
    let build_dir = match &target {
        Some(t) => target_dir.join(t).join(mode),
        None => target_dir.join(mode),
    };
    let plugins_dir = build_dir.join("plugins");
    let build_cmd = if target.is_some() { "cross" } else { "cargo" };

    println!(
        "=== Building {} cdylib plugins ({}{}, {} parallel jobs) ===",
        result.plugins.len(),
        mode,
        target.as_ref().map(|t| format!(", {t}")).unwrap_or_default(),
        jobs
    );

    let failed = Arc::new(AtomicBool::new(false));
    let target_dir = Arc::new(target_dir);
    let target = Arc::new(target);
    let build_cmd: Arc<str> = Arc::from(build_cmd);
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
                let target = Arc::clone(&target);
                let build_cmd = Arc::clone(&build_cmd);

                thread::spawn(move || {
                    println!("  Building {}...", name);

                    let mut cmd = Command::new(build_cmd.as_ref());
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

                    if let Some(t) = target.as_ref() {
                        cmd.args(["--target", t]);
                    }

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

    // Move plugin shared libraries to plugins/ subdirectory
    fs::create_dir_all(&plugins_dir).expect("failed to create plugins directory");

    let lib_ext = plugin_lib_ext(target.as_deref());

    for (name, _) in &plugins {
        let lib_name = plugin_lib_name(name, target.as_deref());
        let src = build_dir.join(format!("{lib_name}.{lib_ext}"));
        let dst = plugins_dir.join(format!("{lib_name}.{lib_ext}"));

        if src.exists() {
            fs::rename(&src, &dst).unwrap_or_else(|e| {
                eprintln!("Failed to move {} to plugins/: {}", lib_name, e);
            });
        }

        // Clean up .rlib and .d files from the build directory
        clean_build_artifacts(&build_dir, &lib_name);
    }

    println!("=== cdylib plugins output to {} ===", plugins_dir.display());
}

fn clean_build_artifacts(build_dir: &Path, lib_name: &str) {
    // Clean .rlib files
    let rlib = build_dir.join(format!("{lib_name}.rlib"));
    if rlib.exists() {
        let _ = fs::remove_file(&rlib);
    }

    // Clean .d files
    let d_file = build_dir.join(format!("{lib_name}.d"));
    if d_file.exists() {
        let _ = fs::remove_file(&d_file);
    }

    // Clean deps/ directory artifacts
    let deps_dir = build_dir.join("deps");
    if deps_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&deps_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let fname = fname.to_string_lossy();
                if fname.starts_with(lib_name) && (fname.ends_with(".rlib") || fname.ends_with(".d"))
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}
