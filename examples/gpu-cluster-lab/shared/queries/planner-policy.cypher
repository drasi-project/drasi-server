MATCH (e:PlacementEligibility)
RETURN e.id AS id, e.workload_id AS workload_id, e.cluster_id AS cluster_id,
       e.context_key AS context_key, e.authorization AS authorization, e.reasons AS reasons,
       e.status AS status, e.error AS error, e.policy_hash AS policy_hash
