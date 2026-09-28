MATCH (f:FleetConfiguration)-[:POLICY_CONFIG]->(p:PolicyAssessment)
OPTIONAL MATCH (g:gpu_inventory)
OPTIONAL MATCH (g)-[:GPU_SAMPLE]->(s:GpuSample)
WITH f, p, g, s,
     CASE WHEN s IS NULL THEN false
          WHEN s.observation_epoch <> f.epoch THEN false
          ELSE NOT drasi.trueNowOrLater(
              datetime.realtime().epochMillis >= s.report_time_ms + 5000,
              datetime({epochMillis: s.report_time_ms + 5000})) END AS fresh
RETURN f.epoch AS epoch, f.configuration AS configuration, f.plan AS plan,
       p.assessment AS policy, p.epoch AS policy_epoch,
       collect(CASE WHEN g IS NULL THEN null ELSE {
           gpu_id:g.gpu_id, fresh:fresh,
           background_memory_mib:s.background_memory_requested_mib,
           background_compute_units:s.background_compute_units
       } END) AS capacities
