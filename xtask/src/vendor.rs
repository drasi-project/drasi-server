//! OCI registry push/pull for vendored native libraries.
//!
//! Pushes a vendor/{target} directory as a single-layer OCI artifact (tar.gz)
//! and pulls it back, using the OCI Distribution API directly.

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;
use tar::{Archive, Builder};

const MEDIA_TYPE_LAYER: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
const MEDIA_TYPE_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
const MEDIA_TYPE_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";

/// Parse "ghcr.io/org/repo/path:tag" into (registry_host, repository, tag).
fn parse_image_ref(image_ref: &str) -> Result<(&str, &str, &str)> {
    let (repo_with_host, tag) = image_ref
        .rsplit_once(':')
        .context("image ref must contain a :tag")?;
    let slash_pos = repo_with_host
        .find('/')
        .context("image ref must contain registry host")?;
    let host = &repo_with_host[..slash_pos];
    let repo = &repo_with_host[slash_pos + 1..];
    Ok((host, repo, tag))
}

/// Get an auth token for GHCR using docker credential helpers, gh CLI, or env vars.
fn get_auth_token(registry: &str, repo: &str) -> Result<String> {
    // Try GH_TOKEN / GITHUB_TOKEN env var first
    if let Ok(pat) = std::env::var("GH_TOKEN").or_else(|_| std::env::var("GITHUB_TOKEN")) {
        return exchange_or_basic(registry, repo, "token", &pat);
    }

    // Try docker credential helper
    if let Some(token) = try_docker_credential_helper(registry) {
        return exchange_or_basic(registry, repo, &token.0, &token.1);
    }

    // Try `gh auth token` CLI
    if let Ok(output) = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
    {
        if output.status.success() {
            let pat = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !pat.is_empty() {
                return exchange_or_basic(registry, repo, "token", &pat);
            }
        }
    }

    bail!("No auth found. Set GH_TOKEN, run `cosign login ghcr.io`, or `gh auth login`")
}

fn exchange_or_basic(registry: &str, repo: &str, user: &str, pass: &str) -> Result<String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!(
            "https://{registry}/token?scope=repository:{repo}:pull,push&service={registry}"
        ))
        .basic_auth(user, Some(pass))
        .send();

    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>() {
                if let Some(token) = body.get("token").and_then(|t| t.as_str()) {
                    return Ok(token.to_string());
                }
            }
        }
    }

    Ok(format!(
        "Basic {}",
        b64_encode(format!("{user}:{pass}").as_bytes())
    ))
}

/// Try to get credentials from the docker credential helper.
fn try_docker_credential_helper(registry: &str) -> Option<(String, String)> {
    let config_path = dirs_docker_config().join("config.json");
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).ok()?).ok()?;

    let creds_store = config.get("credsStore").and_then(|s| s.as_str())?;
    let helper = format!("docker-credential-{creds_store}");

    let mut child = std::process::Command::new(&helper)
        .arg("get")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    use std::io::Write;
    child.stdin.take()?.write_all(registry.as_bytes()).ok()?;

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    let cred: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let username = cred.get("Username").and_then(|u| u.as_str())?.to_string();
    let secret = cred.get("Secret").and_then(|s| s.as_str())?.to_string();
    Some((username, secret))
}

fn dirs_docker_config() -> std::path::PathBuf {
    std::env::var("DOCKER_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".docker")
        })
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(std::path::PathBuf::from)
}

fn b64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn format_auth_header(token: &str) -> String {
    if token.starts_with("Basic ") || token.starts_with("Bearer ") {
        token.to_string()
    } else {
        format!("Bearer {token}")
    }
}

/// Create a tar.gz of the target directory.
fn create_tarball(dir: &Path, target_name: &str) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut tar = Builder::new(&mut enc);
        tar.append_dir_all(target_name, dir)
            .context("failed to create tarball")?;
        tar.finish()?;
    }
    enc.finish().context("gzip finish failed")
}

/// Push a vendor directory as an OCI artifact. Returns the digest reference for signing.
pub fn push(dir: &Path, target_name: &str, image_ref: &str) -> Result<String> {
    let (registry, repo, tag) = parse_image_ref(image_ref)?;
    let token = get_auth_token(registry, repo)?;
    let auth = format_auth_header(&token);
    let client = reqwest::blocking::Client::new();
    let base_url = format!("https://{registry}/v2/{repo}");

    // 1. Create tarball
    println!("  Creating tarball...");
    let blob = create_tarball(dir, target_name)?;
    let blob_digest = format!("sha256:{:x}", Sha256::digest(&blob));
    let blob_size = blob.len();
    println!(
        "  Tarball: {} bytes, digest: {}",
        blob_size,
        &blob_digest[..19]
    );

    // 2. Upload blob
    println!("  Uploading blob...");
    // Start upload
    let resp = client
        .post(format!("{base_url}/blobs/uploads/"))
        .header("Authorization", &auth)
        .send()
        .context("failed to start blob upload")?;

    if !resp.status().is_success() {
        bail!(
            "failed to start upload: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }

    let upload_url = resp
        .headers()
        .get("Location")
        .context("no Location header in upload response")?
        .to_str()?
        .to_string();

    // Complete upload with single PUT
    let put_url = if upload_url.contains('?') {
        format!("{upload_url}&digest={blob_digest}")
    } else {
        format!("{upload_url}?digest={blob_digest}")
    };

    // Make URL absolute if relative
    let put_url = if put_url.starts_with('/') {
        format!("https://{registry}{put_url}")
    } else {
        put_url
    };

    let resp = client
        .put(&put_url)
        .header("Authorization", &auth)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", blob_size.to_string())
        .body(blob)
        .send()
        .context("failed to upload blob")?;

    if !resp.status().is_success() {
        bail!(
            "blob upload failed: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }
    println!("  Blob uploaded ✓");

    // 3. Create and upload config blob (empty JSON object)
    let config_bytes = b"{}";
    let config_digest = format!("sha256:{:x}", Sha256::digest(config_bytes));
    let config_size = config_bytes.len();

    let resp = client
        .post(format!("{base_url}/blobs/uploads/"))
        .header("Authorization", &auth)
        .send()
        .context("failed to start config upload")?;

    if !resp.status().is_success() {
        bail!("failed to start config upload: {}", resp.status());
    }

    let upload_url = resp
        .headers()
        .get("Location")
        .context("no Location header")?
        .to_str()?
        .to_string();

    let put_url = if upload_url.contains('?') {
        format!("{upload_url}&digest={config_digest}")
    } else {
        format!("{upload_url}?digest={config_digest}")
    };
    let put_url = if put_url.starts_with('/') {
        format!("https://{registry}{put_url}")
    } else {
        put_url
    };

    let resp = client
        .put(&put_url)
        .header("Authorization", &auth)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", config_size.to_string())
        .body(config_bytes.to_vec())
        .send()
        .context("failed to upload config")?;

    if !resp.status().is_success() {
        bail!("config upload failed: {}", resp.status());
    }

    // 4. Create and push manifest
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": MEDIA_TYPE_MANIFEST,
        "config": {
            "mediaType": MEDIA_TYPE_CONFIG,
            "digest": config_digest,
            "size": config_size,
        },
        "layers": [{
            "mediaType": MEDIA_TYPE_LAYER,
            "digest": blob_digest,
            "size": blob_size,
            "annotations": {
                "org.opencontainers.image.title": format!("{target_name}.tar.gz"),
            }
        }]
    });

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));

    println!("  Pushing manifest...");
    let resp = client
        .put(format!("{base_url}/manifests/{tag}"))
        .header("Authorization", &auth)
        .header("Content-Type", MEDIA_TYPE_MANIFEST)
        .body(manifest_bytes)
        .send()
        .context("failed to push manifest")?;

    if !resp.status().is_success() {
        bail!(
            "manifest push failed: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }
    println!("  Manifest pushed ✓");

    // Return the repo reference with digest for cosign signing
    let digest_ref = format!("{registry}/{repo}@{manifest_digest}");
    Ok(digest_ref)
}

/// Pull a vendor artifact from OCI and extract to target_dir.
pub fn pull(image_ref: &str, target_dir: &Path) -> Result<()> {
    let (registry, repo, tag) = parse_image_ref(image_ref)?;
    let token = get_pull_token(registry, repo)?;
    let auth = format_auth_header(&token);
    let client = reqwest::blocking::Client::new();
    let base_url = format!("https://{registry}/v2/{repo}");

    // 1. Fetch manifest
    println!("  Fetching manifest...");
    let resp = client
        .get(format!("{base_url}/manifests/{tag}"))
        .header("Authorization", &auth)
        .header("Accept", MEDIA_TYPE_MANIFEST)
        .send()
        .context("failed to fetch manifest")?;

    if !resp.status().is_success() {
        bail!(
            "manifest fetch failed: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }

    let manifest: serde_json::Value = resp.json()?;
    let layer = manifest
        .get("layers")
        .and_then(|l| l.as_array())
        .and_then(|a| a.first())
        .context("no layers in manifest")?;
    let digest = layer
        .get("digest")
        .and_then(|d| d.as_str())
        .context("no digest in layer")?;

    // 2. Download blob
    println!("  Downloading blob ({digest})...");
    let resp = client
        .get(format!("{base_url}/blobs/{digest}"))
        .header("Authorization", &auth)
        .send()
        .context("failed to download blob")?;

    if !resp.status().is_success() {
        bail!("blob download failed: {}", resp.status());
    }

    let blob = resp.bytes()?;

    // Verify digest
    let actual_digest = format!("sha256:{:x}", Sha256::digest(&blob));
    if actual_digest != digest {
        bail!("digest mismatch: expected {digest}, got {actual_digest}");
    }

    // 3. Extract tarball
    println!("  Extracting to {}...", target_dir.display());
    let parent = target_dir.parent().context("target_dir has no parent")?;
    std::fs::create_dir_all(parent)?;

    let decoder = GzDecoder::new(&blob[..]);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(parent)
        .context("failed to extract tarball")?;

    if !target_dir.exists() {
        bail!(
            "extraction succeeded but {} not found — check tarball structure",
            target_dir.display()
        );
    }

    println!("  Extracted ✓");
    Ok(())
}

/// Get a pull-only token (doesn't require auth for public packages).
fn get_pull_token(registry: &str, repo: &str) -> Result<String> {
    let client = reqwest::blocking::Client::new();

    // For GHCR, anonymous pull tokens are obtained differently:
    // The www-authenticate challenge from a 401 tells us where to get a token.
    // Try the standard token endpoint first with anonymous credentials.
    let token_url =
        format!("https://{registry}/token?scope=repository:{repo}:pull&service=ghcr.io");

    // Try anonymous token request
    let resp = client.get(&token_url).send();
    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>() {
                if let Some(token) = body.get("token").and_then(|t| t.as_str()) {
                    return Ok(token.to_string());
                }
            }
        }
    }

    // Fall back to authenticated token
    if let Ok(token) = get_auth_token(registry, repo) {
        return Ok(token);
    }

    bail!("could not obtain pull token for {registry}/{repo}")
}

/// Sign an OCI image reference with cosign (keyless / OIDC).
pub fn cosign_sign(image_ref: &str) -> Result<()> {
    let status = Command::new("cosign")
        .args(["sign", "--yes", image_ref])
        .status()
        .context("failed to run cosign sign")?;

    if !status.success() {
        bail!("cosign sign failed with exit code {:?}", status.code());
    }
    Ok(())
}

/// Verify an OCI image's cosign signature.
pub fn cosign_verify(image_ref: &str) -> Result<()> {
    let status = Command::new("cosign")
        .args([
            "verify",
            "--certificate-oidc-issuer",
            "https://token.actions.githubusercontent.com",
            "--certificate-identity-regexp",
            "https://github.com/drasi-project/*",
            image_ref,
        ])
        .status()
        .context("failed to run cosign verify")?;

    if !status.success() {
        bail!("cosign verification failed for {image_ref}");
    }
    Ok(())
}
