use std::process::Command;

fn main() {
    let rustc_version = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string());

    println!(
        "cargo:rustc-env=DRASI_RUSTC_VERSION={}",
        rustc_version.trim()
    );

    // Emit the plugin-sdk version by reading it from the SDK crate's Cargo.toml
    let sdk_version = read_dep_version("drasi-plugin-sdk").unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DRASI_PLUGIN_SDK_VERSION={sdk_version}");

    // Emit the Rust sysroot native lib directory so the server can add it to
    // LD_LIBRARY_PATH at runtime when loading dylib plugins that depend on libstd.
    if let Some(sysroot_lib) = rust_sysroot_native_lib_dir() {
        println!("cargo:rustc-env=DRASI_RUST_LIB_DIR={sysroot_lib}");
    }
}

fn read_dep_version(crate_name: &str) -> Option<String> {
    // Parse the lock file to find the exact resolved version
    let lock_contents = std::fs::read_to_string("Cargo.lock").ok()?;
    let mut found = false;
    for line in lock_contents.lines() {
        if line.starts_with("name = ") && line.contains(crate_name) {
            found = true;
            continue;
        }
        if found && line.starts_with("version = ") {
            return Some(
                line.trim_start_matches("version = ")
                    .trim_matches('"')
                    .to_string(),
            );
        }
        if found && line.trim().is_empty() {
            break;
        }
    }
    None
}

/// Returns the path to the Rust sysroot's native library directory for the
/// current target (e.g. `<sysroot>/lib/rustlib/<target>/lib`).
///
/// This directory contains `libstd-*.so` which dylib plugins depend on.
fn rust_sysroot_native_lib_dir() -> Option<String> {
    let sysroot = Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())?;

    let target = std::env::var("TARGET").ok()?;
    let lib_dir = format!("{}/lib/rustlib/{}/lib", sysroot.trim(), target);

    if std::path::Path::new(&lib_dir).exists() {
        Some(lib_dir)
    } else {
        None
    }
}
