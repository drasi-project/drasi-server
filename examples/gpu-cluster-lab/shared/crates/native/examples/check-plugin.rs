use anyhow::{Context, Result};
fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .context("usage: check-plugin <native-library>")?;
    let plugin = drasi_host_sdk::computation::load(std::path::PathBuf::from(path))?;
    let mut names = plugin
        .factories()
        .iter()
        .map(|factory| factory.metadata().implementation.name.to_string())
        .collect::<Vec<_>>();
    names.sort();
    anyhow::ensure!(
        names
            == [
                "gpu.lab/configuration",
                "gpu.lab/placement-planner",
                "gpu.lab/plan-writer",
                "gpu.lab/runtime-status",
                "gpu.lab/telemetry-simulator",
            ],
        "native plugin factory catalog mismatch"
    );
    println!("Native host loaded all five GPU lab components through the public ABI.");
    Ok(())
}
