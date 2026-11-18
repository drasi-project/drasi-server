pub mod model;
mod plugin;
pub mod source;
pub mod wire;
pub use plugin::plugin;
#[cfg(test)]
mod tests;

pub const INSTANCE: &str = "move-a-wall";
/// The query outputs the UI reads through the native SSE sink.
pub const QUERIES: [&str; 6] = [
    "scene-inputs",
    "scene-status",
    "journey-footprints",
    "obstacle-footprints",
    "obstructions",
    "affected-journeys",
];

#[cfg(feature = "dynamic-plugin")]
drasi_computation_plugin_sdk::export_computation_plugin!(plugin());
