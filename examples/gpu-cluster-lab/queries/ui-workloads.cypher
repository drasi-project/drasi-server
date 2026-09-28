MATCH (w:workload_requirements)
OPTIONAL MATCH (a:AppliedPlan)
OPTIONAL MATCH (p:gpu_placements)
OPTIONAL MATCH (c:SchedulingContext)
OPTIONAL MATCH (r:DemoReadiness)
OPTIONAL MATCH (w)-[:EXECUTION_WORKLOAD]->(e:PolicyEnforcement)
OPTIONAL MATCH (e)-[:EXECUTION_GPU]->(g:gpu_inventory)
OPTIONAL MATCH (e)-[:EXECUTION_SAMPLE]->(s:GpuSample)
OPTIONAL MATCH (e)-[:EXECUTION_POLICY]->(auth:PlacementEligibility)
WITH w, a, p, c, r, e, g, s, auth,
     CASE WHEN s IS NULL THEN true ELSE drasi.trueNowOrLater(
       datetime.realtime().epochMillis >= s.report_time_ms + 5000,
       datetime({epochMillis: s.report_time_ms + 5000})
     ) END AS expired
WITH w, a, r, e,
     r.observation_epoch = c.observation_epoch AND a.observation_epoch = c.observation_epoch
     AND e.observation_epoch = c.observation_epoch AND s.observation_epoch = c.observation_epoch
     AND auth.observation_epoch = c.observation_epoch AS epoch_current,
     r.inputs_ready = true AND c.current = true AND a.source_ready = true
     AND r.scheduling_signature = c.scheduling_signature AND r.policy_signature = c.policy_signature
     AND a.config_fingerprint = c.config_fingerprint AS inputs_current,
     a.application.applied_plan_version = toString(p.plan_version)
     AND e.applied_plan_version = toString(p.plan_version) AS plan_applied,
     e.state = 'running' AND e.replica_index < w.replicas AND g.scheduling_enabled = true
     AND e.profile_id = w.profile_id AND e.data_profile_id = w.data_profile_id AND e.purpose = w.purpose
     AND e.memory_mib = w.memory_mib_per_replica AND e.compute_units = w.compute_units_per_replica AS workload_matches,
     auth.current = true AND auth.authorization = 'allow' AND auth.policy_signature = c.policy_signature
     AND auth.workload_id = w.workload_id AND auth.cluster_id = g.cluster_id
     AND auth.data_profile_id = w.data_profile_id AND auth.purpose = w.purpose AS policy_current,
     s.applied_plan_version = toString(p.plan_version) AND NOT expired
     AND s.report_time_ms >= e.acknowledged_at_ms AS report_current,
     size([d IN p.assignments WHERE d = e.assignment]) = 1 AS assignment_matches
WITH w, a, r, e, epoch_current AND inputs_current AND plan_applied AND workload_matches
     AND policy_current AND report_current AND assignment_matches AS confirmed
WITH w, a IS NOT NULL AS execution_known, r IS NOT NULL AND r.inputs_ready = true AS readiness_known,
     sum(CASE WHEN e.state = 'running' THEN 1 ELSE 0 END) AS running_count,
     sum(CASE WHEN confirmed THEN 1 ELSE 0 END) AS confirmed_count,
     sum(CASE WHEN e.state = 'suspended' THEN 1 ELSE 0 END) AS suspended_count,
     sum(CASE WHEN e.state = 'fenced' THEN 1 ELSE 0 END) AS fenced_count
RETURN w.workload_id AS workload_id, w.name AS name, w.profile_id AS profile_id,
       w.data_profile_id AS data_profile_id, w.purpose AS purpose, toString(w.revision) AS revision,
       w.replicas AS replicas, w.memory_mib_per_replica AS memory_mib_per_replica,
       w.compute_units_per_replica AS compute_units_per_replica,
       w.spread_across_domains AS spread_across_domains,
       CASE WHEN execution_known THEN running_count ELSE null END AS running_replicas,
       CASE WHEN execution_known AND readiness_known THEN confirmed_count ELSE null END AS ready_replicas,
       CASE WHEN execution_known THEN suspended_count ELSE null END AS suspended_replicas,
       CASE WHEN execution_known THEN fenced_count ELSE null END AS fenced_replicas,
       CASE WHEN execution_known THEN 0 ELSE null END AS fencing_pending_replicas
