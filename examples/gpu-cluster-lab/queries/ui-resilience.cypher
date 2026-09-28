MATCH (r:ResilienceAssessment)
OPTIONAL MATCH (r)-[:RESILIENCE_DIAGNOSTIC]->(d:CapacityDiagnostic)
RETURN r.fleet_id AS fleet_id, r.observation_epoch AS observation_epoch,
       r.scheduling_signature AS scheduling_signature, r.policy_signature AS policy_signature,
       r.status AS status, r.workers AS workers, r.regions AS regions,
       CASE WHEN d.observation_epoch = r.observation_epoch AND d.policy_signature = r.policy_signature
            THEN d.diagnostic.feasible ELSE null END AS capacity_only_feasible,
       CASE WHEN d.observation_epoch = r.observation_epoch AND d.policy_signature = r.policy_signature
            THEN d.diagnostic.detail ELSE 'No capacity-only result for these inputs.' END AS capacity_only_detail
