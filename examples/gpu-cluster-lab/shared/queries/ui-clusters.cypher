MATCH (c:regional_clusters)
OPTIONAL MATCH (c)-[:GPU_CLUSTER]->(g:gpu_inventory)
OPTIONAL MATCH (g)-[:GPU_SAMPLE]->(s:GpuSample)
WITH c, g, s,
     CASE WHEN s IS NULL THEN false ELSE drasi.trueNowOrLater(
       datetime.realtime().epochMillis >= s.report_time_ms + 5000,
       datetime({epochMillis: s.report_time_ms + 5000})
     ) END AS expired
WITH c.cluster_id AS cluster_id, c.name AS name, c.region AS region,
     collect(g.host_id) AS hosts, count(g.gpu_id) AS gpu_count, count(s.gpu_id) AS sample_count,
     sum(CASE WHEN s IS NULL OR expired THEN 0 ELSE 1 END) AS healthy_count
RETURN cluster_id, name, region, size(coll.distinct(hosts)) AS registered_workers,
       CASE WHEN gpu_count > sample_count THEN null ELSE healthy_count END AS healthy_gpus
