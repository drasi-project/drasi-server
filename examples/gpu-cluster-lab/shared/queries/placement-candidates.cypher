MATCH (w:workload_requirements), (g:gpu_inventory)-[:GPU_CLUSTER]->(c:regional_clusters)
WHERE '*' IN w.allowed_gpu_models OR g.model IN w.allowed_gpu_models
RETURN w.workload_id AS workload_id, g.gpu_id AS gpu_id, c.cluster_id AS cluster_id,
       g.host_id AS host_id, c.region AS region, g.failure_domain AS failure_domain,
       w.replicas AS replicas, w.spread_across_domains = true AS spread,
       w.memory_mib_per_replica AS memory_mib, w.compute_units_per_replica AS compute_units
