pub mod geometry;
pub mod model;
mod plugin;
pub mod source;
pub mod transformer;
pub mod wire;
pub use plugin::plugin;
#[cfg(test)]
mod tests;

pub const INSTANCE: &str = "move-a-wall";
pub const QUERIES: [&str; 5] = [
    "scene-inputs",
    "geometry-context",
    "obstructions",
    "affected-journeys",
    "geometry-status",
];

#[cfg(feature = "dynamic-plugin")]
drasi_computation_plugin_sdk::export_computation_plugin!(plugin());
