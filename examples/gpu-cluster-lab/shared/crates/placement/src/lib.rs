use anyhow::{ensure, Result};
use good_lp::{
    constraint, microlp, variable, Expression, ProblemVariables, ResolutionError, Solution,
    SolutionStatus, SolverModel, WithTimeLimit,
};
use gpu_contracts::*;
use gpu_policy::Assessment;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Outcome {
    Feasible {
        assignments: Vec<Assignment>,
        moved_replicas: usize,
        new_replicas: usize,
    },
    Infeasible {
        reason: String,
    },
    Unknown {
        error: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicaMove {
    pub replica_id: String,
    pub from_gpu_id: Option<Uuid>,
    pub to_gpu_id: Uuid,
    pub reason: String,
}

pub fn signature(
    config: &Configuration,
    policy: &Assessment,
    capacities: &BTreeMap<Uuid, Capacity>,
) -> Result<String> {
    hash(
        "gpu-scheduling-v1",
        &(config.fingerprint()?, &policy.policy_signature, capacities),
    )
}

pub fn solve(
    config: &Configuration,
    policy: &Assessment,
    capacities: &BTreeMap<Uuid, Capacity>,
    previous: &[Assignment],
    budget: Duration,
) -> Result<Outcome> {
    config.validate()?;
    policy.validate_current(config)?;
    if validate_assignments(config, capacities, previous).is_ok()
        && policy.validate_plan(config, previous).is_ok()
    {
        return Ok(Outcome::Feasible {
            assignments: previous.to_vec(),
            moved_replicas: 0,
            new_replicas: 0,
        });
    }
    solve_model(config, Some(policy), capacities, previous, budget, true)
}

fn solve_model(
    config: &Configuration,
    policy: Option<&Assessment>,
    capacities: &BTreeMap<Uuid, Capacity>,
    previous: &[Assignment],
    budget: Duration,
    optimize: bool,
) -> Result<Outcome> {
    let deadline = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| anyhow::anyhow!("invalid solve budget"))?;
    let replicas: Vec<_> = config
        .workloads
        .values()
        .flat_map(|w| (0..w.replicas).map(move |i| (w, i)))
        .collect();
    let previous: BTreeMap<_, _> = previous.iter().map(|a| (a.key(), a.gpu_id)).collect();
    let mut optimal_moves = None;
    let mut result = Outcome::Unknown {
        error: "solver did not run".into(),
    };
    for phase in 0..if optimize { 2 } else { 1 } {
        if Instant::now() >= deadline {
            return Ok(Outcome::Unknown {
                error: "solve time budget exhausted".into(),
            });
        }
        let mut vars = ProblemVariables::new();
        let peak = vars.add(variable().min(0));
        let mut edges = Vec::new();
        let mut moves = Expression::from(0);
        for (w, i) in &replicas {
            for g in config.gpus.values() {
                let Some(c) = capacities.get(&g.gpu_id) else {
                    continue;
                };
                if !c.eligible
                    || !g.scheduling_enabled
                    || c.memory_mib < w.memory_mib_per_replica
                    || c.compute_units < w.compute_units_per_replica
                    || (!w.allowed_gpu_models.contains("*")
                        && !w.allowed_gpu_models.contains(&g.model))
                    || policy.is_some_and(|p| !p.allows(w.workload_id, &g.cluster_id))
                {
                    continue;
                }
                let x = vars.add(variable().binary());
                if previous
                    .get(&(w.workload_id, *i))
                    .is_some_and(|old| *old != g.gpu_id)
                {
                    moves += x;
                }
                edges.push((*w, *i, g, x));
            }
        }
        let objective = if phase == 0 {
            moves.clone()
        } else {
            Expression::from(peak)
        };
        let mut model = vars.minimise(objective).using(microlp).with_time_limit(
            deadline
                .saturating_duration_since(Instant::now())
                .as_secs_f64(),
        );
        for (w, i) in &replicas {
            let assigned: Expression = edges
                .iter()
                .filter(|(v, j, _, _)| v.workload_id == w.workload_id && j == i)
                .map(|(_, _, _, x)| *x)
                .sum();
            model.add_constraint(constraint!(assigned == 1));
        }
        for g in config.gpus.values() {
            let Some(c) = capacities.get(&g.gpu_id) else {
                continue;
            };
            // GiB keeps MILP coefficients near the demand units; validation still uses exact MiB.
            let memory: Expression = edges
                .iter()
                .filter(|(_, _, v, _)| v.gpu_id == g.gpu_id)
                .map(|(w, _, _, x)| *x * (f64::from(w.memory_mib_per_replica) / 1024.0))
                .sum();
            let demand: Expression = edges
                .iter()
                .filter(|(_, _, v, _)| v.gpu_id == g.gpu_id)
                .map(|(w, _, _, x)| *x * w.compute_units_per_replica)
                .sum();
            model.add_constraint(constraint!(
                memory <= f64::from(c.memory_mib.min(g.memory_mib)) / 1024.0
            ));
            model.add_constraint(constraint!(
                demand.clone() <= c.compute_units.min(PLANNING_UNITS)
            ));
            if c.compute_units > 0 {
                model.add_constraint(constraint!(demand <= peak * c.compute_units));
            }
        }
        let domains: BTreeSet<_> = config.gpus.values().map(|g| &g.failure_domain).collect();
        for w in config
            .workloads
            .values()
            .filter(|w| w.spread_across_domains)
        {
            for domain in &domains {
                let count: Expression = edges
                    .iter()
                    .filter(|(v, _, g, _)| {
                        v.workload_id == w.workload_id && &g.failure_domain == *domain
                    })
                    .map(|(_, _, _, x)| *x)
                    .sum();
                model.add_constraint(constraint!(count <= 1));
            }
        }
        if let Some(minimum) = optimal_moves {
            model.add_constraint(constraint!(moves.clone() == minimum));
        }
        let solution = match model.solve() {
            Ok(s) if matches!(s.status(), SolutionStatus::Optimal) => s,
            Ok(_) => return Ok(Outcome::Unknown { error: "solve timed out before proving optimum".into() }),
            Err(ResolutionError::Infeasible) if phase > 0 => return Ok(Outcome::Unknown {
                error: "Peak-demand optimization rejected the validated minimum-movement solution".into(),
            }),
            Err(ResolutionError::Infeasible) => return Ok(Outcome::Infeasible {
                reason: "No complete assignment satisfies resources, compatibility, policy and worker separation".into(),
            }),
            Err(e) => return Ok(Outcome::Unknown { error: e.to_string() }),
        };
        let mut assignments: Vec<_> = edges
            .iter()
            .filter(|(_, _, _, x)| solution.value(*x) > 0.5)
            .map(|(w, i, g, _)| Assignment::new(w, *i, g.gpu_id))
            .collect();
        assignments.sort();
        validate_assignments(config, capacities, &assignments)?;
        if let Some(p) = policy {
            p.validate_plan(config, &assignments)?;
        }
        let moved = assignments
            .iter()
            .filter(|a| previous.get(&a.key()).is_some_and(|old| old != &a.gpu_id))
            .count();
        let new = assignments
            .iter()
            .filter(|a| !previous.contains_key(&a.key()))
            .count();
        optimal_moves = Some(moved as f64);
        result = Outcome::Feasible {
            assignments,
            moved_replicas: moved,
            new_replicas: new,
        };
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub excluded: String,
    pub feasible: Option<bool>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resilience {
    pub scheduling_signature: String,
    pub policy_signature: String,
    pub workers: Vec<Scenario>,
    pub regions: Vec<Scenario>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LossKind {
    Worker,
    Region,
}

pub fn assess_loss(
    config: &Configuration,
    policy: &Assessment,
    capacities: &BTreeMap<Uuid, Capacity>,
    kind: LossKind,
    excluded: &str,
    budget: Duration,
) -> Result<Scenario> {
    config.validate()?;
    policy.validate_current(config)?;
    let mut hypothetical = capacities.clone();
    for g in config.gpus.values() {
        let removed = match kind {
            LossKind::Worker => g.host_id == excluded,
            LossKind::Region => config.clusters[&g.cluster_id].region == excluded,
        };
        if removed {
            if let Some(c) = hypothetical.get_mut(&g.gpu_id) {
                c.eligible = false;
            }
        }
    }
    let outcome = solve_model(config, Some(policy), &hypothetical, &[], budget, false)?;
    let (feasible, detail) = match outcome {
        Outcome::Feasible { .. } => (
            Some(true),
            "Complete recovery is feasible within policy".into(),
        ),
        Outcome::Infeasible { reason } => (Some(false), reason),
        Outcome::Unknown { error } => (None, error),
    };
    Ok(Scenario {
        excluded: excluded.into(),
        feasible,
        detail,
    })
}

pub fn assess(
    config: &Configuration,
    policy: &Assessment,
    capacities: &BTreeMap<Uuid, Capacity>,
    budget: Duration,
) -> Result<Resilience> {
    config.validate()?;
    policy.validate_current(config)?;
    let deadline = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| anyhow::anyhow!("invalid assessment budget"))?;
    let mut workers = BTreeSet::new();
    let mut regions = BTreeSet::new();
    for g in config
        .gpus
        .values()
        .filter(|g| capacities.get(&g.gpu_id).is_some_and(|c| c.eligible))
    {
        workers.insert(g.host_id.clone());
        regions.insert(config.clusters[&g.cluster_id].region.clone());
    }
    let mut output = Resilience {
        scheduling_signature: signature(config, policy, capacities)?,
        policy_signature: policy.policy_signature.clone(),
        workers: vec![],
        regions: vec![],
    };
    for (kind, excluded) in workers
        .into_iter()
        .map(|s| ("worker", s))
        .chain(regions.into_iter().map(|s| ("region", s)))
    {
        let scenario = assess_loss(
            config,
            policy,
            capacities,
            if kind == "worker" {
                LossKind::Worker
            } else {
                LossKind::Region
            },
            &excluded,
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(2)),
        )?;
        if kind == "worker" {
            output.workers.push(scenario);
        } else {
            output.regions.push(scenario);
        }
    }
    Ok(output)
}

pub fn capacity_only(
    config: &Configuration,
    capacities: &BTreeMap<Uuid, Capacity>,
    budget: Duration,
) -> Result<Scenario> {
    config.validate()?;
    let outcome = solve_model(config, None, capacities, &[], budget, false)?;
    let (feasible, detail) = match outcome {
        Outcome::Feasible { .. } => (
            Some(true),
            "Capacity available; this diagnostic does not authorize processing".into(),
        ),
        Outcome::Infeasible { reason } => (Some(false), reason),
        Outcome::Unknown { error } => (None, error),
    };
    Ok(Scenario {
        excluded: "policy-filter-only".into(),
        feasible,
        detail,
    })
}

pub fn candidate(
    config: &Configuration,
    policy: &Assessment,
    capacities: &BTreeMap<Uuid, Capacity>,
    previous: &Plan,
    outcome: Outcome,
    reasons: Vec<String>,
) -> Result<Candidate> {
    let Outcome::Feasible {
        assignments,
        moved_replicas,
        new_replicas,
    } = outcome
    else {
        anyhow::bail!("only a complete feasible outcome can become a candidate");
    };
    validate_assignments(config, capacities, &assignments)?;
    policy.validate_plan(config, &assignments)?;
    let free: Vec<_> = capacities
        .values()
        .filter(|c| c.eligible)
        .map(|c| {
            let used: u32 = previous
                .assignments
                .iter()
                .filter(|a| a.gpu_id == c.gpu_id)
                .map(|a| a.memory_mib)
                .sum();
            c.memory_mib.saturating_sub(used)
        })
        .collect();
    let moves: Vec<_> = assignments
        .iter()
        .filter_map(|assignment| {
            let previous = previous
                .assignments
                .iter()
                .find(|old| old.key() == assignment.key());
            if previous.is_some_and(|old| old.gpu_id == assignment.gpu_id) {
                return None;
            }
            Some(ReplicaMove {
                replica_id: format!("{}/{}", assignment.workload_id, assignment.replica_index),
                from_gpu_id: previous.map(|old| old.gpu_id),
                to_gpu_id: assignment.gpu_id,
                reason: if previous.is_some() {
                    "moved-for-complete-feasible-plan"
                } else {
                    "new-replica"
                }
                .into(),
            })
        })
        .collect();
    ensure!(
        moves.iter().filter(|m| m.from_gpu_id.is_some()).count() == moved_replicas
            && moves.iter().filter(|m| m.from_gpu_id.is_none()).count() == new_replicas
            && moved_replicas + new_replicas <= assignments.len(),
        "solver movement counts disagree with assignment evidence"
    );
    let details = serde_json::json!({
        "schema_version":1, "reason_codes":reasons, "moved_replicas":moved_replicas,
        "new_replicas":new_replicas, "retained_replicas":assignments.len()-moved_replicas-new_replicas,
        "pre_plan_free_memory_mib":free.iter().sum::<u32>(),
        "pre_plan_largest_gap_mib":free.iter().max().copied().unwrap_or(0),
        "moves":moves, "scheduling_signature":signature(config, policy, capacities)?,
        "capacities":capacities.values().collect::<Vec<_>>(), "policy":policy,
    });
    ensure!(
        serde_json::to_vec(&details)?.len() <= 128 * 1024,
        "decision evidence exceeds limit"
    );
    Ok(Candidate {
        decision_id: Uuid::new_v4(),
        expected_plan_version: previous.plan_version,
        config_fingerprint: config.fingerprint()?,
        policy_signature: policy.policy_signature.clone(),
        policy_bundle_hash: policy.policy_bundle_hash.clone(),
        scheduling_signature: signature(config, policy, capacities)?,
        assignments,
        decision_details: details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu_policy::Evaluator;
    #[test]
    fn increased_background_demand_requires_only_one_existing_move() -> Result<()> {
        for order in 0..48 {
            let mut f = fixtures::load("baseline")?;
            let gpu_ids: BTreeMap<_, _> = f
                .configuration
                .gpus
                .values()
                .map(|g| {
                    let index = match (g.host_id.as_str(), g.gpu_index) {
                        ("inference-a", 0) => 4,
                        ("inference-a", 1) => 0,
                        ("inference-b", 0) => 2,
                        ("inference-b", 1) => 1,
                        ("inference-c", 0) => 3,
                        ("inference-c", 1) => 5,
                        _ => unreachable!(),
                    };
                    let index = if order >= 24 { 5 - index } else { index };
                    (g.gpu_id, Uuid::from_u128(100 + (index + order) % 6))
                })
                .collect();
            let workload_ids: BTreeMap<_, _> = f
                .configuration
                .workloads
                .values()
                .map(|w| {
                    let index = match w.profile_id.as_str() {
                        "chat-v1" => 0,
                        "reranker-v1" => 1,
                        "assistant-v1" => 2,
                        "embeddings-v1" => 3,
                        _ => unreachable!(),
                    };
                    (
                        w.workload_id,
                        Uuid::from_u128(200 + (index + order / 6) % 4),
                    )
                })
                .collect();
            f.configuration.gpus = f
                .configuration
                .gpus
                .into_values()
                .map(|mut g| {
                    g.gpu_id = gpu_ids[&g.gpu_id];
                    (g.gpu_id, g)
                })
                .collect();
            f.configuration.workloads = f
                .configuration
                .workloads
                .into_values()
                .map(|mut w| {
                    w.workload_id = workload_ids[&w.workload_id];
                    (w.workload_id, w)
                })
                .collect();
            for assignment in &mut f.assignments {
                assignment.gpu_id = gpu_ids[&assignment.gpu_id];
                assignment.workload_id = workload_ids[&assignment.workload_id];
            }
            let policy = Evaluator::new()?.evaluate(&f.configuration)?;
            let mut capacities = fixtures::baseline_capacities(&f.configuration);
            let source = f
                .configuration
                .gpus
                .values()
                .find(|g| g.host_id == "inference-a" && g.gpu_index == 0)
                .unwrap();
            let destination = f
                .configuration
                .gpus
                .values()
                .find(|g| g.host_id == "inference-c" && g.gpu_index == 1)
                .unwrap();
            capacities.insert(
                source.gpu_id,
                Capacity::from_observation(source, true, 0, 35),
            );
            let mut witness = f.assignments.clone();
            witness
                .iter_mut()
                .find(|a| a.gpu_id == source.gpu_id)
                .unwrap()
                .gpu_id = destination.gpu_id;
            validate_assignments(&f.configuration, &capacities, &witness)?;
            policy.validate_plan(&f.configuration, &witness)?;
            let feasible = solve_model(
                &f.configuration,
                Some(&policy),
                &capacities,
                &f.assignments,
                Duration::from_secs(2),
                false,
            )?;
            assert!(
                matches!(feasible, Outcome::Feasible { .. }),
                "feasibility pass: {feasible:?}"
            );
            let outcome = solve(
                &f.configuration,
                &policy,
                &capacities,
                &f.assignments,
                Duration::from_secs(2),
            )?;
            let Outcome::Feasible {
                assignments,
                moved_replicas,
                new_replicas,
            } = outcome
            else {
                panic!("validated one-move witness exists, but solver returned {outcome:?}");
            };
            assert_eq!((assignments.len(), moved_replicas, new_replicas), (8, 1, 0));
            let restored = solve(
                &f.configuration,
                &policy,
                &fixtures::baseline_capacities(&f.configuration),
                &assignments,
                Duration::from_secs(2),
            )?;
            let Outcome::Feasible {
                assignments: after,
                moved_replicas,
                ..
            } = restored
            else {
                panic!("{restored:?}");
            };
            assert_eq!(moved_replicas, 0);
            assert_eq!(after, assignments);
        }
        Ok(())
    }

    #[test]
    fn fragmentation_requires_exactly_one_existing_move() -> Result<()> {
        let mut f = fixtures::load("fragmentation")?;
        let w =
            Workload::from_profile("document-assistant", "assistant-v1", 1, "demo-open", "demo")?;
        f.configuration.workloads.insert(w.workload_id, w);
        let policy = Evaluator::new()?.evaluate(&f.configuration)?;
        let result = solve(
            &f.configuration,
            &policy,
            &fixtures::baseline_capacities(&f.configuration),
            &f.assignments,
            Duration::from_secs(2),
        )?;
        let Outcome::Feasible {
            assignments,
            moved_replicas,
            new_replicas,
        } = result
        else {
            panic!("{result:?}")
        };
        assert_eq!((assignments.len(), moved_replicas, new_replicas), (7, 1, 1));
        Ok(())
    }
    #[test]
    fn baseline_and_added_load_have_distinct_recovery() -> Result<()> {
        let mut f = fixtures::load("baseline")?;
        let evaluator = Evaluator::new()?;
        let baseline = assess(
            &f.configuration,
            &evaluator.evaluate(&f.configuration)?,
            &fixtures::baseline_capacities(&f.configuration),
            Duration::from_secs(5),
        )?;
        assert!(baseline.workers.iter().all(|s| s.feasible == Some(true)));
        assert_eq!(baseline.regions[0].feasible, Some(false));
        let w = Workload::from_profile("tenant-chat", "chat-v1", 2, "demo-open", "demo")?;
        f.configuration.workloads.insert(w.workload_id, w);
        let p = evaluator.evaluate(&f.configuration)?;
        let caps = fixtures::baseline_capacities(&f.configuration);
        assert!(matches!(
            solve(
                &f.configuration,
                &p,
                &caps,
                &f.assignments,
                Duration::from_secs(2)
            )?,
            Outcome::Feasible { .. }
        ));
        assert!(assess(&f.configuration, &p, &caps, Duration::from_secs(5))?
            .workers
            .iter()
            .all(|s| s.feasible == Some(false)));
        for slot in 0..2 {
            let g = Gpu::new("inference-d", "eu-primary", slot);
            f.configuration.gpus.insert(g.gpu_id, g);
        }
        let p = evaluator.evaluate(&f.configuration)?;
        assert!(assess(
            &f.configuration,
            &p,
            &fixtures::baseline_capacities(&f.configuration),
            Duration::from_secs(5)
        )?
        .workers
        .iter()
        .all(|s| s.feasible == Some(true)));
        Ok(())
    }
    #[test]
    fn healthy_us_capacity_never_overrides_policy() -> Result<()> {
        let f = fixtures::load("regional-boundary")?;
        let p = Evaluator::new()?.evaluate(&f.configuration)?;
        let mut caps = fixtures::baseline_capacities(&f.configuration);
        for g in f.configuration.gpus.values() {
            if g.cluster_id == "eu-primary" {
                caps.get_mut(&g.gpu_id).unwrap().eligible = false;
            }
        }
        let result = solve(
            &f.configuration,
            &p,
            &caps,
            &f.assignments,
            Duration::from_secs(2),
        )?;
        let Outcome::Feasible { assignments, .. } = result else {
            panic!("{result:?}")
        };
        assert!(assignments
            .iter()
            .all(|a| f.configuration.gpus[&a.gpu_id].cluster_id == "eu-recovery"));
        for g in f.configuration.gpus.values() {
            if g.host_id == "recovery-a" {
                caps.get_mut(&g.gpu_id).unwrap().eligible = false;
            }
        }
        assert!(matches!(
            solve(
                &f.configuration,
                &p,
                &caps,
                &assignments,
                Duration::from_secs(2)
            )?,
            Outcome::Infeasible { .. }
        ));
        assert_eq!(
            capacity_only(&f.configuration, &caps, Duration::from_secs(2))?.feasible,
            Some(true)
        );
        Ok(())
    }
}
