fn main() {
    // Allow the plugin version to be set via environment variable.
    // Default to "1.0.0" if not provided.
    let version = std::env::var("PLUGIN_VERSION").unwrap_or_else(|_| "1.0.0".to_string());
    println!("cargo:rustc-env=PLUGIN_VERSION={version}");
    println!("cargo:rerun-if-env-changed=PLUGIN_VERSION");
}
