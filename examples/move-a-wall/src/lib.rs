pub mod geometry;
pub mod model;
mod plugin;
pub mod source;
pub mod transformer;
pub mod wire;
pub use plugin::plugin;
#[cfg(test)]
mod tests;

use drasi_lib::config::{QueryJoinConfig, QueryJoinKeyConfig};

pub const INSTANCE: &str = "move-a-wall";
pub const QUERIES: [&str; 5] = [
    "scene-inputs",
    "geometry-context",
    "obstructions",
    "affected-journeys",
    "geometry-status",
];

pub fn joins() -> Vec<QueryJoinConfig> {
    [
        ("CART_TASK", "Cart", "cart_id", "Journey", "cart_id"),
        (
            "TO_DESTINATION",
            "Journey",
            "destination_id",
            "Destination",
            "destination_id",
        ),
        (
            "BLOCKS",
            "Obstruction",
            "journey_id",
            "Journey",
            "journey_id",
        ),
        (
            "CAUSED_BY",
            "Obstruction",
            "obstacle_id",
            "Obstacle",
            "obstacle_id",
        ),
    ]
    .into_iter()
    .map(|(id, a, ap, b, bp)| QueryJoinConfig {
        id: id.into(),
        keys: vec![
            QueryJoinKeyConfig {
                label: a.into(),
                property: ap.into(),
            },
            QueryJoinKeyConfig {
                label: b.into(),
                property: bp.into(),
            },
        ],
    })
    .collect()
}

#[cfg(feature = "dynamic-plugin")]
drasi_computation_plugin_sdk::export_computation_plugin!(plugin());
