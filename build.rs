use std::process::Command;

/// Vendor OCI registry and default tag for pre-built native libraries.
const VENDOR_REGISTRY: &str = "ghcr.io";
const VENDOR_REPO_PREFIX: &str = "drasi-project/vendor";
const VENDOR_TAG: &str = "v1";

/// Targets that require vendored native libraries.
const VENDORED_TARGETS: &[&str] = &["x86_64-pc-windows-msvc"];

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

    // Auto-download vendored native libraries for targets that need them.
    if let (Ok(target), Ok(manifest_dir)) =
        (std::env::var("TARGET"), std::env::var("CARGO_MANIFEST_DIR"))
    {
        let vendor_dir = std::path::PathBuf::from(&manifest_dir)
            .join("vendor")
            .join(&target);
        let vendor_lib_dir = vendor_dir.join("lib");

        if VENDORED_TARGETS.contains(&target.as_str()) && !vendor_lib_dir.exists() {
            eprintln!(
                "cargo:warning=Vendored libs for {target} not found, downloading from OCI registry..."
            );
            match download_vendor(&target, &vendor_dir) {
                Ok(()) => eprintln!("cargo:warning=Vendored libs for {target} downloaded ✓"),
                Err(e) => {
                    panic!("Failed to download vendored libs for {target}: {e}");
                }
            }
        }

        if vendor_lib_dir.exists() {
            println!(
                "cargo:rustc-link-search=native={}",
                vendor_lib_dir.display()
            );
        }
    }

    // Emit the plugin-sdk version by reading it from the SDK crate's Cargo.toml
    let sdk_version = read_dep_version("drasi-plugin-sdk").unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DRASI_PLUGIN_SDK_VERSION={sdk_version}");

    // Emit dependency versions for host version info (used by plugin auto-install)
    let core_version = read_dep_version("drasi-core").unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DRASI_CORE_VERSION={core_version}");

    let lib_version = read_dep_version("drasi-lib").unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DRASI_LIB_VERSION={lib_version}");

    // Emit the target triple for runtime platform detection
    if let Ok(target) = std::env::var("TARGET") {
        println!("cargo:rustc-env=TARGET_TRIPLE={target}");
    }

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

/// Download vendored native libraries from the OCI registry.
///
/// Fetches a tar.gz artifact from `ghcr.io/drasi-project/vendor/{target}:{tag}`
/// and extracts it to `vendor/{target}/`.
fn download_vendor(target: &str, vendor_dir: &std::path::Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use sha2::{Digest, Sha256};
    use tar::Archive;

    let repo = format!("{VENDOR_REPO_PREFIX}/{target}");
    let base_url = format!("https://{VENDOR_REGISTRY}/v2/{repo}");

    // Get anonymous pull token (vendor packages are public)
    let token_url = format!(
        "https://{VENDOR_REGISTRY}/token?scope=repository:{repo}:pull&service={VENDOR_REGISTRY}"
    );
    let token_resp: serde_json::Value = ureq::get(&token_url)
        .call()
        .map_err(|e| format!("token request failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("token parse failed: {e}"))?;
    let token = token_resp
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or("no token in response")?;

    // Fetch manifest
    let manifest: serde_json::Value = ureq::get(&format!("{base_url}/manifests/{VENDOR_TAG}"))
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "application/vnd.oci.image.manifest.v1+json")
        .call()
        .map_err(|e| format!("manifest fetch failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("manifest parse failed: {e}"))?;

    let layer = manifest
        .get("layers")
        .and_then(|l| l.as_array())
        .and_then(|a| a.first())
        .ok_or("no layers in manifest")?;
    let digest = layer
        .get("digest")
        .and_then(|d| d.as_str())
        .ok_or("no digest in layer")?;

    // Download blob (raise body limit — vendor tarballs are ~25MB+)
    let blob = ureq::get(&format!("{base_url}/blobs/{digest}"))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| format!("blob download failed: {e}"))?
        .into_body()
        .with_config()
        .limit(100 * 1024 * 1024) // 100 MB
        .read_to_vec()
        .map_err(|e| format!("blob read failed: {e}"))?;

    // Verify digest
    let actual_digest = format!("sha256:{:x}", Sha256::digest(&blob));
    if actual_digest != digest {
        return Err(format!(
            "digest mismatch: expected {digest}, got {actual_digest}"
        ));
    }

    // Extract tarball into parent of vendor_dir (tar contains the target name as root)
    let parent = vendor_dir.parent().ok_or("vendor_dir has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("mkdir failed: {e}"))?;

    let decoder = GzDecoder::new(&blob[..]);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(parent)
        .map_err(|e| format!("extract failed: {e}"))?;

    if !vendor_dir.join("lib").exists() {
        return Err(format!(
            "extraction succeeded but {}/lib not found",
            vendor_dir.display()
        ));
    }

    Ok(())
}
