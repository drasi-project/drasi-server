MATCH (p:PlacementEligibility)
RETURN p.id AS id, p.workload_id AS workload_id, p.cluster_id AS cluster_id,
       p.region AS region, p.data_profile_id AS data_profile_id, p.purpose AS purpose,
       p.policy_id AS policy_id, p.authority_ref AS authority_ref,
       p.policy_revision AS policy_revision, p.allowed_regions AS allowed_regions,
       p.observation_epoch AS observation_epoch, p.input_fingerprint AS input_fingerprint,
       p.policy_signature AS policy_signature, p.authorization AS authorization,
       p.reasons AS reasons, p.current AS current, p.error AS error
