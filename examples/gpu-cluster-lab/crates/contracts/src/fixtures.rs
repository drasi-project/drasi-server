use super::*;

pub const NAMES: [&str; 3] = ["baseline", "fragmentation", "regional-boundary"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub name: String,
    pub configuration: Configuration,
    pub settings: BTreeMap<Uuid, Settings>,
    pub assignments: Vec<Assignment>,
}

pub fn load(name: &str) -> Result<Fixture> {
    ensure!(NAMES.contains(&name), "unknown fixture {name}");
    let regional = name == "regional-boundary";
    let mut config = Configuration::default();
    let (customer, data, policy, purpose, class, regions, authority) = if regional {
        (
            "customer-eu",
            "customer-eu-documents",
            "customer-eu-processing",
            "customer-support",
            "restricted",
            vec!["westeurope", "northeurope"],
            "customer-eu-contract-v1",
        )
    } else {
        (
            "demo",
            "demo-open",
            "demo-permissive",
            "demo",
            "synthetic",
            vec!["*"],
            "demo-fixture",
        )
    };
    config.policies.insert(
        policy.into(),
        Policy {
            policy_id: policy.into(),
            name: policy.into(),
            customer_id: customer.into(),
            allowed_regions: regions.into_iter().map(str::to_owned).collect(),
            allowed_purposes: BTreeSet::from([purpose.into()]),
            allowed_classifications: BTreeSet::from([class.into()]),
            authority_ref: authority.into(),
            revision: 1,
        },
    );
    config.data_profiles.insert(
        data.into(),
        DataProfile {
            data_profile_id: data.into(),
            customer_id: customer.into(),
            classification: class.into(),
            policy_id: policy.into(),
            authority_ref: authority.into(),
            revision: 1,
        },
    );
    let mut topology = vec![(
        "eu-primary",
        "westeurope",
        vec!["inference-a", "inference-b", "inference-c"],
    )];
    if regional {
        topology.extend([
            (
                "eu-recovery",
                "northeurope",
                vec!["recovery-a", "recovery-b"],
            ),
            ("us-spare", "eastus", vec!["us-a", "us-b"]),
        ]);
    }
    let mut settings = BTreeMap::new();
    let mut slots = BTreeMap::new();
    for (cluster, region, hosts) in topology {
        config.clusters.insert(
            cluster.into(),
            Cluster {
                cluster_id: cluster.into(),
                name: cluster.into(),
                region: region.into(),
                revision: 1,
            },
        );
        for host in hosts {
            for slot in 0..2 {
                let gpu = Gpu::new(host, cluster, slot);
                slots.insert((host, slot), gpu.gpu_id);
                settings.insert(gpu.gpu_id, Settings::baseline(gpu.gpu_id));
                config.gpus.insert(gpu.gpu_id, gpu);
            }
        }
    }
    let specs = if name == "fragmentation" {
        vec![
            ("chat-alpha", "chat-v1"),
            ("chat-bravo", "chat-v1"),
            ("chat-charlie", "chat-v1"),
        ]
    } else {
        vec![
            ("assistant", "assistant-v1"),
            ("chat", "chat-v1"),
            ("embeddings", "embeddings-v1"),
            ("reranker", "reranker-v1"),
        ]
    };
    let mut ids = BTreeMap::new();
    for (name, profile) in specs {
        let w = Workload::from_profile(name, profile, 2, data, purpose)?;
        ids.insert(name, w.workload_id);
        config.workloads.insert(w.workload_id, w);
    }
    let layout = if name == "fragmentation" {
        vec![
            ("chat-alpha", 0, "inference-a", 0),
            ("chat-bravo", 0, "inference-a", 1),
            ("chat-alpha", 1, "inference-b", 0),
            ("chat-charlie", 0, "inference-b", 1),
            ("chat-bravo", 1, "inference-c", 0),
            ("chat-charlie", 1, "inference-c", 1),
        ]
    } else {
        vec![
            ("assistant", 0, "inference-a", 0),
            ("chat", 0, "inference-a", 1),
            ("assistant", 1, "inference-b", 0),
            ("embeddings", 0, "inference-b", 1),
            ("reranker", 0, "inference-b", 1),
            ("chat", 1, "inference-c", 0),
            ("reranker", 1, "inference-c", 0),
            ("embeddings", 1, "inference-c", 1),
        ]
    };
    let mut assignments: Vec<_> = layout
        .into_iter()
        .map(|(name, index, host, slot)| {
            Assignment::new(&config.workloads[&ids[name]], index, slots[&(host, slot)])
        })
        .collect();
    assignments.sort();
    config.validate()?;
    validate_settings(&config, &settings)?;
    validate_assignments(&config, &baseline_capacities(&config), &assignments)?;
    Ok(Fixture {
        name: name.into(),
        configuration: config,
        settings,
        assignments,
    })
}

pub fn baseline_capacities(config: &Configuration) -> BTreeMap<Uuid, Capacity> {
    config
        .gpus
        .values()
        .map(|g| (g.gpu_id, Capacity::from_observation(g, true, 0, 10)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixtures_have_expected_resources_and_new_identities() -> Result<()> {
        let a = load("baseline")?;
        let b = load("baseline")?;
        assert_ne!(
            a.configuration.fingerprint()?,
            b.configuration.fingerprint()?
        );
        assert_eq!(
            a.assignments.iter().map(|a| a.memory_mib).sum::<u32>(),
            216 * 1024
        );
        assert_eq!(
            a.assignments.iter().map(|a| a.compute_units).sum::<u32>(),
            260
        );
        assert_eq!(load("regional-boundary")?.configuration.gpus.len(), 14);
        let f = load("fragmentation")?;
        assert_eq!(
            6 * MEMORY_MIB - f.assignments.iter().map(|a| a.memory_mib).sum::<u32>(),
            336 * 1024
        );
        Ok(())
    }
    #[test]
    fn fingerprint_is_order_independent_and_revision_sensitive() -> Result<()> {
        let mut a = load("baseline")?.configuration;
        let old = a.fingerprint()?;
        let b: Configuration = serde_json::from_value(serde_json::to_value(&a)?)?;
        assert_eq!(old, b.fingerprint()?);
        a.clusters.get_mut("eu-primary").unwrap().revision += 1;
        assert_ne!(old, a.fingerprint()?);
        assert_eq!(
            hash("test", &serde_json::json!({"a":1}))?,
            format!("{:x}", Sha256::digest(b"test\0{\"a\":1}"))
        );
        Ok(())
    }
    #[test]
    fn plans_cannot_be_partial_or_change_reservations() -> Result<()> {
        let f = load("baseline")?;
        let cap = baseline_capacities(&f.configuration);
        assert!(validate_assignments(&f.configuration, &cap, &f.assignments[..7]).is_err());
        validate_execution_subset(&f.configuration, &cap, &f.assignments[..7])?;
        let mut unrequested = f.assignments[0].clone();
        unrequested.replica_index = 100;
        assert!(validate_execution_subset(&f.configuration, &cap, &[unrequested]).is_err());
        let mut changed = f.assignments.clone();
        changed[0].compute_units = 1;
        assert!(validate_assignments(&f.configuration, &cap, &changed).is_err());
        Ok(())
    }
}
