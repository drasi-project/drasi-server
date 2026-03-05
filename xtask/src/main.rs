use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod vendor;

#[derive(Parser)]
#[command(name = "xtask", about = "Development tasks for drasi-server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage vendored native libraries in OCI registry
    Vendor {
        #[command(subcommand)]
        action: VendorAction,
    },
}

#[derive(Subcommand)]
enum VendorAction {
    /// Push vendored libs for a target to the OCI registry
    Push {
        /// Target triple (e.g. x86_64-pc-windows-msvc)
        target: String,
        /// Version tag (e.g. v1)
        #[arg(long, default_value = "latest")]
        tag: String,
        /// OCI registry prefix
        #[arg(long, default_value = "ghcr.io/drasi-project")]
        registry: String,
        /// Sign with cosign after pushing
        #[arg(long)]
        sign: bool,
    },
    /// Pull vendored libs for a target from the OCI registry
    Pull {
        /// Target triple (e.g. x86_64-pc-windows-msvc)
        target: String,
        /// Version tag (e.g. v1)
        #[arg(long, default_value = "latest")]
        tag: String,
        /// OCI registry prefix
        #[arg(long, default_value = "ghcr.io/drasi-project")]
        registry: String,
        /// Verify cosign signature before extracting
        #[arg(long)]
        verify: bool,
    },
    /// List available vendor targets in the local vendor/ directory
    List,
}

fn workspace_root() -> Result<PathBuf> {
    let output = std::process::Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .context("failed to run cargo locate-project")?;
    let path = Path::new(std::str::from_utf8(&output.stdout)?.trim());
    Ok(path
        .parent()
        .context("Cargo.toml has no parent")?
        .to_path_buf())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root()?;
    let vendor_dir = root.join("vendor");

    match cli.command {
        Command::Vendor { action } => match action {
            VendorAction::Push {
                target,
                tag,
                registry,
                sign,
            } => {
                let target_dir = vendor_dir.join(&target);
                if !target_dir.exists() {
                    bail!("vendor/{target} does not exist");
                }
                let image_ref = format!("{registry}/vendor/{target}:{tag}");
                println!("Pushing vendor/{target} -> {image_ref}");
                let digest_ref = vendor::push(&target_dir, &target, &image_ref)?;
                if sign {
                    println!("Signing {digest_ref} with cosign...");
                    vendor::cosign_sign(&digest_ref)?;
                }
                println!("Done ✓");
            }
            VendorAction::Pull {
                target,
                tag,
                registry,
                verify,
            } => {
                let target_dir = vendor_dir.join(&target);
                let image_ref = format!("{registry}/vendor/{target}:{tag}");
                if target_dir.exists() {
                    println!("vendor/{target} already exists, skipping download");
                    return Ok(());
                }
                if verify {
                    println!("Verifying cosign signature for {image_ref}...");
                    vendor::cosign_verify(&image_ref)?;
                }
                println!("Pulling {image_ref} -> vendor/{target}");
                vendor::pull(&image_ref, &target_dir)?;
                println!("Done ✓");
            }
            VendorAction::List => {
                if !vendor_dir.exists() {
                    println!("No vendor/ directory found");
                    return Ok(());
                }
                for entry in std::fs::read_dir(&vendor_dir)? {
                    let entry = entry?;
                    if entry.file_type()?.is_dir() {
                        let name = entry.file_name();
                        let lib_dir = entry.path().join("lib");
                        let file_count = if lib_dir.exists() {
                            std::fs::read_dir(&lib_dir)?.count()
                        } else {
                            0
                        };
                        println!("  {} ({} lib files)", name.to_string_lossy(), file_count);
                    }
                }
            }
        },
    }
    Ok(())
}
