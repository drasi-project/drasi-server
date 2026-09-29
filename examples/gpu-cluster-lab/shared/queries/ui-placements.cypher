MATCH (p:gpu_placements)
OPTIONAL MATCH (a:AppliedPlan)
OPTIONAL MATCH (c:SchedulingContext)
OPTIONAL MATCH (r:DemoReadiness)
OPTIONAL MATCH (e:PolicyEnforcement)
OPTIONAL MATCH (e)-[:EXECUTION_WORKLOAD]->(w:workload_requirements)
OPTIONAL MATCH (e)-[:EXECUTION_GPU]->(g:gpu_inventory)
OPTIONAL MATCH (e)-[:EXECUTION_SAMPLE]->(s:GpuSample)
OPTIONAL MATCH (e)-[:EXECUTION_POLICY]->(auth:PlacementEligibility)
WITH p, a, c, r, e, w, g, s, auth,
     CASE WHEN s IS NULL THEN true ELSE drasi.trueNowOrLater(
       datetime.realtime().epochMillis >= s.report_time_ms + 5000,
       datetime({epochMillis: s.report_time_ms + 5000})
     ) END AS expired
WITH p, a, c, r, e,
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
WITH p, a, c, r, e, epoch_current AND inputs_current AND plan_applied AND workload_matches
     AND policy_current AND report_current AND assignment_matches AS confirmed
WITH p, a, c, r, sum(CASE WHEN confirmed THEN 1 ELSE 0 END) AS confirmed_count,
     sum(CASE WHEN e IS NOT NULL AND e.state <> 'fenced' THEN 1 ELSE 0 END) AS active_count
WITH p, a, c, confirmed_count, active_count,
     CASE WHEN a IS NULL OR c IS NULL OR r IS NULL OR c.current <> true
       OR r.inputs_ready <> true OR a.source_ready <> true
       OR r.observation_epoch <> c.observation_epoch OR a.observation_epoch <> c.observation_epoch
       OR r.scheduling_signature <> c.scheduling_signature OR r.policy_signature <> c.policy_signature THEN 'unknown'
     WHEN a.application.error IS NOT NULL THEN 'blocked'
     WHEN a.application.applied_plan_version IS NULL OR a.application.applied_plan_version <> toString(p.plan_version)
       THEN 'awaiting-application'
     WHEN confirmed_count = size(p.assignments) AND active_count = size(p.assignments)
       AND c.required_replicas = size(p.assignments) AND a.config_fingerprint = c.config_fingerprint THEN 'confirmed'
     ELSE 'awaiting-measurements' END AS status
RETURN p.fleet_id AS fleet_id, coalesce(c.observation_epoch, a.observation_epoch, '') AS observation_epoch,
       coalesce(c.scheduling_signature, '') AS scheduling_signature,
       coalesce(c.policy_signature, '') AS policy_signature,
       p.decision_id AS decision_id, p.config_fingerprint AS config_fingerprint,
       CASE WHEN p.plan_version = 0 OR p.plan_version = '0' THEN null ELSE toString(p.plan_version) END AS desired_plan_version,
       a.application.applied_plan_version AS applied_plan_version,
       CASE WHEN status = 'confirmed' THEN toString(p.plan_version) ELSE null END AS confirmed_plan_version,
       [d IN p.assignments | {id: d.workload_id + '/' + toString(d.replica_index),
         workload_id: d.workload_id, replica_index: d.replica_index, gpu_id: d.gpu_id,
         profile_id: d.profile_id, data_profile_id: d.data_profile_id, purpose: d.purpose,
         memory_mib: d.memory_mib, compute_units: d.compute_units}] AS desired,
       coalesce(a.execution, []) AS actual, status AS status,
       CASE WHEN status = 'confirmed' THEN 'Current replicas match the saved plan, policy and fresh reports.'
         WHEN a.application.error IS NOT NULL THEN a.application.error
         WHEN status = 'awaiting-measurements' AND a.config_fingerprint <> c.config_fingerprint
           THEN 'Waiting for the simulator to observe the current configuration.'
         WHEN status = 'awaiting-measurements'
           THEN 'Waiting for fleet confirmation: ' + toString(confirmed_count) + ' confirmed, '
             + toString(active_count) + ' active, ' + toString(size(p.assignments)) + ' saved, '
             + toString(c.required_replicas) + ' required replicas.'
         ELSE 'Waiting for matching runtime, application and report evidence.' END AS reason
