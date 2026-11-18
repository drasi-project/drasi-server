MATCH (w:workload_requirements), (c:regional_clusters), (f:FleetConfiguration)
OPTIONAL MATCH (w)-[:WORKLOAD_PROFILE]->(d:data_profiles)
OPTIONAL MATCH (d)-[:PROFILE_POLICY]->(p:placement_policies)
WITH w, c, f, d, p, w.workload_id + '/' + c.cluster_id AS id
RETURN id, w.workload_id AS workload_id, c.cluster_id AS cluster_id,
       c.region AS region, w.data_profile_id AS data_profile_id, w.purpose AS purpose,
       coalesce(d.policy_id, 'unresolved') AS policy_id,
       coalesce(p.authority_ref, '') AS authority_ref,
       p.revision AS policy_revision, coalesce(p.allowed_regions, []) AS allowed_regions,
       f.epoch AS observation_epoch,
       id + '@' + w.revision + ':' + w.data_profile_id + ':' + coalesce(d.revision, '-') + ':'
         + coalesce(d.policy_id, '-') + ':' + coalesce(p.revision, '-') + ':' + c.revision AS context_key,
       {workload_id: w.workload_id, data_profile_id: w.data_profile_id, purpose: w.purpose} AS workload,
       CASE WHEN d IS NULL THEN null ELSE {
         data_profile_id: d.data_profile_id, customer_id: d.customer_id,
         classification: d.classification, policy_id: d.policy_id,
         authority_ref: d.authority_ref, revision: d.revision} END AS data_profile,
       CASE WHEN p IS NULL THEN null ELSE {
         policy_id: p.policy_id, customer_id: p.customer_id, allowed_regions: p.allowed_regions,
         allowed_purposes: p.allowed_purposes, allowed_classifications: p.allowed_classifications,
         authority_ref: p.authority_ref, revision: p.revision} END AS policy,
       {cluster_id: c.cluster_id, name: c.name, region: c.region, revision: c.revision} AS cluster
