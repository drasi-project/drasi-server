MATCH (f:FleetConfiguration), (g:gpu_inventory)-[:GPU_CLUSTER]->(c:regional_clusters)
OPTIONAL MATCH (g)-[:GPU_SAMPLE]->(s:GpuSample)
WITH f, g, c, s,
     CASE WHEN s IS NULL THEN false
          WHEN s.observation_epoch <> f.epoch THEN false
          ELSE NOT drasi.trueNowOrLater(
              datetime.realtime().epochMillis >= s.report_time_ms + 5000,
              datetime({epochMillis: s.report_time_ms + 5000})) END AS fresh
WITH g, c, fresh,
     CASE WHEN fresh THEN g.memory_mib - s.background_memory_requested_mib ELSE 0 END AS memory,
     CASE WHEN fresh THEN 85 - s.background_compute_units ELSE 0 END AS compute
RETURN g.gpu_id AS gpu_id, g.host_id AS host_id, c.cluster_id AS cluster_id, c.region AS region,
       g.memory_mib AS installed_memory_mib,
       CASE WHEN memory > 0 THEN memory ELSE 0 END AS memory_mib,
       CASE WHEN compute > 0 THEN compute ELSE 0 END AS compute_units,
       g.scheduling_enabled = true AND fresh AS eligible
