//! Configurations of the drasi-core `drasi.standard/policy` and
//! `drasi.standard/optimization` instances the lab runs.
use serde_json::{json, Value};

pub const POLICY_IMPLEMENTATION: &str = "drasi.standard/policy";
pub const OPTIMIZATION_IMPLEMENTATION: &str = "drasi.standard/optimization";

/// Processing relationships between the configuration processor, the
/// processing queries, the plugin instances and the lab transformers.
pub const EDGES: [(&str, &str); 26] = [
    ("configuration", "simulation-inputs"),
    ("configuration", "placement-pairs"),
    ("configuration", "placement-gpus"),
    ("configuration", "placement-candidates"),
    ("configuration", "solver"),
    ("configuration", "capacity-check"),
    ("simulator", "placement-gpus"),
    ("placement-pairs", "policy"),
    ("policy", "planner-policy"),
    ("policy", "solver"),
    ("placement-gpus", "solver"),
    ("placement-gpus", "capacity-check"),
    ("placement-candidates", "solver"),
    ("placement-candidates", "capacity-check"),
    ("solver", "planner-solution"),
    ("solver", "planner-status"),
    ("simulation-inputs", "simulator"),
    ("simulation-inputs", "placement"),
    ("planner-policy", "simulator"),
    ("planner-policy", "placement"),
    ("placement-gpus", "placement"),
    ("planner-solution", "placement"),
    ("planner-status", "placement"),
    ("placement", "plan-output"),
    ("plan-output", "plan-writer"),
    ("placement", "runtime-context"),
];

/// Evaluates `eligibility.rego` once per `PlacementPair` row.
pub fn policy() -> Value {
    let mut config = gpu_contracts::policy::policy_bundle();
    let extra = json!({
        "stream": "policy/out",
        "labels": ["PlacementPair"],
        "input": {
            "workload": {
                "workload_id": "$properties.workload_id",
                "data_profile_id": "$properties.data_profile_id",
                "purpose": "$properties.purpose"
            },
            "data_profile": "$properties.data_profile",
            "policy": "$properties.policy",
            "cluster": "$properties.cluster"
        },
        "output_label": "PlacementEligibility",
        "copy_input_properties": true,
        "output_properties": {
            "input_fingerprint": "$element.properties.context_key",
            "authorization": "$decision.authorization",
            "reasons": "$decision.reasons",
            "status": "$status",
            "error": "$error"
        },
        "provenance": true,
        "on_error": "emit",
        "timeout_ms": 50
    });
    config
        .as_object_mut()
        .expect("bundle is an object")
        .extend(extra.as_object().expect("constant object").clone());
    config
}

fn sets(policy: bool) -> Vec<Value> {
    let mut pair_filters = vec![json!({"inSet": "gpus", "match": {"gpu_id": "gpu_id"}})];
    let mut sets = vec![
        json!({"name": "gpus", "label": "PlacementGpu", "key": ["gpu_id"], "where": [
            {"property": "eligible", "op": "==", "value": true}
        ]}),
        json!({"name": "workloads", "label": "workload_requirements", "key": ["workload_id"]}),
        json!({"name": "replicas", "from": "workloads",
               "expand": {"count": "replicas", "index": "replica_index"}}),
        json!({"name": "context", "label": "FleetConfiguration", "key": ["id"]}),
    ];
    if policy {
        sets.push(json!({"name": "previous", "label": "PlanAssignment",
                         "key": ["workload_id", "replica_index"]}));
        sets.push(json!({"name": "allowed", "label": "PlacementEligibility",
                         "key": ["workload_id", "cluster_id"], "where": [
            {"property": "authorization", "op": "==", "value": "allow"},
            {"property": "status", "op": "==", "value": "decided"}
        ]}));
        pair_filters.push(json!({"inSet": "allowed",
                                 "match": {"workload_id": "workload_id", "cluster_id": "cluster_id"}}));
    }
    sets.push(json!({"name": "pairs", "label": "PlacementCandidate",
                     "key": ["workload_id", "gpu_id"], "where": pair_filters}));
    sets.push(json!({"name": "edges", "from": "pairs",
                     "expand": {"count": "replicas", "index": "replica_index"}}));
    sets
}

fn constraints() -> Value {
    json!([
        {"name": "one-gpu-per-replica", "forEach": "replicas",
         "terms": [{"variable": "assign",
                    "match": {"workload_id": "workload_id", "replica_index": "replica_index"}}],
         "op": "==", "rhs": 1},
        {"name": "observed-memory", "forEach": "gpus",
         "terms": [{"variable": "assign", "match": {"gpu_id": "gpu_id"},
                    "coefficient": {"property": "memory_mib", "scale": 0.0009765625}}],
         "op": "<=", "rhs": {"property": "memory_mib", "scale": 0.0009765625}},
        {"name": "installed-memory", "forEach": "gpus",
         "terms": [{"variable": "assign", "match": {"gpu_id": "gpu_id"},
                    "coefficient": {"property": "memory_mib", "scale": 0.0009765625}}],
         "op": "<=", "rhs": {"property": "installed_memory_mib", "scale": 0.0009765625}},
        {"name": "observed-compute", "forEach": "gpus",
         "terms": [{"variable": "assign", "match": {"gpu_id": "gpu_id"},
                    "coefficient": {"property": "compute_units"}}],
         "op": "<=", "rhs": {"property": "compute_units"}},
        {"name": "planning-compute", "forEach": "gpus",
         "terms": [{"variable": "assign", "match": {"gpu_id": "gpu_id"},
                    "coefficient": {"property": "compute_units"}}],
         "op": "<=", "rhs": 85},
        {"name": "peak-utilization", "forEach": "gpus",
         "where": [{"property": "compute_units", "op": ">", "value": 0}],
         "terms": [
            {"variable": "assign", "match": {"gpu_id": "gpu_id"},
             "coefficient": {"property": "compute_units"}},
            {"variable": "peak",
             "coefficient": {"property": "compute_units", "of": "group", "scale": -1}}
         ],
         "op": "<=", "rhs": 0},
        {"name": "spread-failure-domains", "groupBy": ["workload_id", "failure_domain"],
         "terms": [{"variable": "assign",
                    "where": [{"property": "spread", "op": "==", "value": true}]}],
         "op": "<=", "rhs": 1}
    ])
}

/// The policy-filtered placement model: minimum movement from the saved plan,
/// then minimum peak utilisation, plus worker and region loss scenarios.
pub fn solver() -> Value {
    json!({
        "stream": "solver/out",
        "debounceMs": 250,
        "maxDelayMs": 2000,
        "timeLimitMs": 2000,
        "sets": sets(true),
        "variables": [
            {"name": "assign", "type": "binary", "over": "edges",
             "output": {"label": "PlacementAssignment",
                        "properties": ["workload_id", "replica_index", "gpu_id"]}},
            {"name": "peak", "type": "continuous", "lower": 0, "output": {"emit": false}}
        ],
        "constraints": constraints(),
        "objectives": [
            {"name": "moves", "sense": "minimize", "terms": [{"variable": "assign", "where": [
                {"inSet": "previous", "match": {"workload_id": "workload_id", "replica_index": "replica_index"}},
                {"notInSet": "previous", "match": {"workload_id": "workload_id", "replica_index": "replica_index", "gpu_id": "gpu_id"}}
            ]}]},
            {"name": "peak", "sense": "minimize", "terms": [{"variable": "peak"}]}
        ],
        "scenarios": [
            {"name": "worker", "set": "gpus", "property": "host_id", "removeFrom": ["gpus", "pairs"]},
            {"name": "region", "set": "gpus", "property": "region", "removeFrom": ["gpus", "pairs"]}
        ],
        "statusLabel": "PlacementStatus",
        "scenarioLabel": "PlacementScenario",
        "provenance": {"set": "context",
                       "properties": ["config_fingerprint", "plan_version", "epoch"]}
    })
}

/// The same resource model without policy: a diagnostic only.
pub fn capacity_check() -> Value {
    json!({
        "stream": "capacity-check/out",
        "debounceMs": 250,
        "maxDelayMs": 2000,
        "timeLimitMs": 2000,
        "sets": sets(false),
        "variables": [
            {"name": "assign", "type": "binary", "over": "edges", "output": {"emit": false}},
            {"name": "peak", "type": "continuous", "lower": 0, "output": {"emit": false}}
        ],
        "constraints": constraints(),
        "statusLabel": "CapacityStatus",
        "provenance": {"set": "context",
                       "properties": ["config_fingerprint", "plan_version", "epoch"]}
    })
}
