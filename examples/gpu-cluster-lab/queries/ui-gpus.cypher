MATCH (g:gpu_inventory)-[:GPU_CLUSTER]->(c:regional_clusters)
MATCH (g)-[:GPU_SETTINGS]->(t:gpu_telemetry)
OPTIONAL MATCH (g)-[:GPU_SAMPLE]->(s:GpuSample)
WITH g, c, t, s, datetime.realtime().epochMillis AS now,
     CASE WHEN s IS NULL THEN false ELSE drasi.trueNowOrLater(
       datetime.realtime().epochMillis >= s.report_time_ms + 5000,
       datetime({epochMillis: s.report_time_ms + 5000})
     ) END AS expired
RETURN g.gpu_id AS gpu_id, g.name AS name, g.host_id AS host_id,
       g.cluster_id AS cluster_id, c.region AS region, g.gpu_index AS slot,
       g.memory_mib AS memory_mib, toString(g.revision) AS inventory_revision,
       toString(t.revision) AS telemetry_revision,
       g.scheduling_enabled AS scheduling_enabled,
       t.powered_on AS powered_on, t.reporting_enabled AS reporting_enabled,
       t.interval_ms AS interval_ms, t.background_compute_units AS background_compute_units,
       t.background_memory_mib AS background_memory_mib,
       coalesce(s.observation_epoch, '') AS observation_epoch,
       CASE WHEN s IS NULL THEN 'unknown' WHEN expired THEN 'unreachable' ELSE 'healthy' END AS health,
       CASE WHEN s IS NULL THEN null ELSE now - s.report_time_ms END AS sample_age_ms,
       s.report_time_ms AS report_time_ms, s.report_sequence AS report_sequence,
       s.applied_plan_version AS sample_plan_version,
       s.inventory_revision AS sample_inventory_revision,
       s.telemetry_revision AS sample_telemetry_revision,
       s.background_compute_units AS reported_background_compute_units,
       s.managed_compute_units AS managed_compute_units,
       s.total_compute_units AS total_compute_units, s.busy_percent AS busy_percent,
       s.managed_memory_mib AS managed_memory_mib,
       s.background_memory_requested_mib AS reported_background_memory_requested_mib,
       s.background_memory_allocated_mib AS background_memory_allocated_mib,
       s.modeled_memory_used_mib AS modeled_memory_used_mib
