MATCH (d:DecisionExplanation)
OPTIONAL MATCH (d)-[:DECISION_WRITE]->(w:PlanWriteOutcome)
RETURN d.decision_id AS decision_id, d.observation_epoch AS observation_epoch,
       d.scheduling_signature AS scheduling_signature, d.policy_signature AS policy_signature,
       d.config_fingerprint AS config_fingerprint, d.outcome AS outcome,
       CASE WHEN d.stage = 'candidate' AND w.observation_epoch = d.observation_epoch
                 AND w.outcome = 'rejected' THEN 'rejected' ELSE d.stage END AS stage,
       d.plan_version AS plan_version,
       CASE WHEN d.stage = 'candidate' AND w.observation_epoch = d.observation_epoch
                 AND w.outcome = 'rejected' THEN w.detail ELSE d.summary END AS summary,
       d.reason_codes AS reason_codes,
       d.moved_replicas AS moved_replicas, d.new_replicas AS new_replicas,
       d.retained_replicas AS retained_replicas, d.pre_plan_free_memory_mib AS pre_plan_free_memory_mib,
       d.pre_plan_largest_gap_mib AS pre_plan_largest_gap_mib, d.moves AS moves
