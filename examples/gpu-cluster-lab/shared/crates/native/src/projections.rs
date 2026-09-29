//! Query definitions over native evidence; registration and PostgreSQL input assembly are separate.

use drasi_lib::{
    computation::v1::QueryExecutionSettings,
    config::{QueryJoinConfig, QueryJoinKeyConfig},
};

pub const POLICY: &str = include_str!("../../../queries/ui-policy.cypher");
pub const DECISIONS: &str = include_str!("../../../queries/ui-decisions.cypher");
pub const TIMELINE: &str = include_str!("../../../queries/ui-timeline.cypher");
pub const RESILIENCE: &str = include_str!("../../../queries/ui-resilience.cypher");
pub const GPUS: &str = include_str!("../../../queries/ui-gpus.cypher");
pub const CLUSTERS: &str = include_str!("../../../queries/ui-clusters.cypher");
pub const WORKLOADS: &str = include_str!("../../../queries/ui-workloads.cypher");
pub const PLACEMENTS: &str = include_str!("../../../queries/ui-placements.cypher");
pub const STATUS: &str = include_str!("../../../queries/ui-status.cypher");

pub fn definitions() -> Vec<(&'static str, &'static str, QueryExecutionSettings)> {
    vec![
        ("ui-gpus", GPUS, gpu_settings()),
        ("ui-clusters", CLUSTERS, cluster_settings()),
        ("ui-workloads", WORKLOADS, execution_settings()),
        ("ui-placements", PLACEMENTS, execution_settings()),
        ("ui-status", STATUS, QueryExecutionSettings::default()),
        ("ui-policy", POLICY, QueryExecutionSettings::default()),
        ("ui-decisions", DECISIONS, decision_settings()),
        ("ui-timeline", TIMELINE, QueryExecutionSettings::default()),
        ("ui-resilience", RESILIENCE, resilience_settings()),
    ]
}

pub fn execution_settings() -> QueryExecutionSettings {
    QueryExecutionSettings {
        joins: vec![
            join(
                "EXECUTION_WORKLOAD",
                ["PolicyEnforcement", "workload_requirements"],
                "workload_id",
            ),
            join(
                "EXECUTION_GPU",
                ["PolicyEnforcement", "gpu_inventory"],
                "gpu_id",
            ),
            join(
                "EXECUTION_SAMPLE",
                ["PolicyEnforcement", "GpuSample"],
                "gpu_id",
            ),
            join(
                "EXECUTION_POLICY",
                ["PolicyEnforcement", "PlacementEligibility"],
                "input_fingerprint",
            ),
        ],
        ..Default::default()
    }
}

pub fn gpu_settings() -> QueryExecutionSettings {
    let mut settings = cluster_settings();
    settings.joins.push(join(
        "GPU_SETTINGS",
        ["gpu_inventory", "gpu_telemetry"],
        "gpu_id",
    ));
    settings
}

pub fn cluster_settings() -> QueryExecutionSettings {
    QueryExecutionSettings {
        joins: vec![
            join(
                "GPU_CLUSTER",
                ["gpu_inventory", "regional_clusters"],
                "cluster_id",
            ),
            join("GPU_SAMPLE", ["gpu_inventory", "GpuSample"], "gpu_id"),
        ],
        ..Default::default()
    }
}

pub fn decision_settings() -> QueryExecutionSettings {
    join_settings(
        "DECISION_WRITE",
        ["DecisionExplanation", "PlanWriteOutcome"],
        "decision_id",
    )
}

pub fn resilience_settings() -> QueryExecutionSettings {
    join_settings(
        "RESILIENCE_DIAGNOSTIC",
        ["ResilienceAssessment", "CapacityDiagnostic"],
        "scheduling_signature",
    )
}

fn join_settings(id: &str, labels: [&str; 2], property: &str) -> QueryExecutionSettings {
    QueryExecutionSettings {
        joins: vec![join(id, labels, property)],
        ..Default::default()
    }
}

fn join(id: &str, labels: [&str; 2], property: &str) -> QueryJoinConfig {
    QueryJoinConfig {
        id: id.into(),
        keys: labels
            .into_iter()
            .map(|label| QueryJoinKeyConfig {
                label: label.into(),
                property: property.into(),
            })
            .collect(),
    }
}
