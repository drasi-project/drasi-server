MATCH (c:SchedulingContext)
OPTIONAL MATCH (s:PlacementStatus)
OPTIONAL MATCH (k:CapacityStatus)
OPTIONAL MATCH (r:PlacementScenario)
WITH c, s, k, r,
     c.current = true AND s.provenance.config_fingerprint = c.config_fingerprint
     AND s.provenance.epoch = c.observation_epoch AS current,
     r.provenance.config_fingerprint = c.config_fingerprint
     AND r.provenance.epoch = c.observation_epoch AS scenario_current
WITH c, s, k, current, k.status = 'optimal' AS capacity_ok,
     collect(CASE WHEN scenario_current AND r.scenario = 'worker'
       THEN {excluded: r.value, feasible: r.feasible, detail: r.detail} ELSE null END) AS workers,
     collect(CASE WHEN scenario_current AND r.scenario = 'region'
       THEN {excluded: r.value, feasible: r.feasible, detail: r.detail} ELSE null END) AS regions
RETURN c.fleet_id AS fleet_id, c.observation_epoch AS observation_epoch,
       c.scheduling_signature AS scheduling_signature, c.policy_signature AS policy_signature,
       CASE WHEN current THEN 'current' ELSE 'checking' END AS status,
       CASE WHEN current THEN s.status ELSE null END AS placement_status,
       [x IN workers WHERE x IS NOT NULL] AS workers,
       [x IN regions WHERE x IS NOT NULL] AS regions,
       CASE WHEN current AND s.status = 'infeasible' THEN capacity_ok ELSE null END AS capacity_only_feasible,
       CASE WHEN NOT current THEN 'No capacity-only result for these inputs.'
            WHEN s.status <> 'infeasible' THEN 'The policy-constrained placement is feasible.'
            WHEN k.status = 'optimal' THEN 'Capacity alone could place every replica; data policy blocks the plan.'
            ELSE coalesce(k.detail, 'Capacity alone cannot place every replica.') END AS capacity_only_detail
