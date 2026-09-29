MATCH (g:gpu_inventory)-[:GPU_CLUSTER]->(c:regional_clusters)
RETURN g.gpu_id AS gpu_id, g AS inventory, c.cluster_id AS cluster_id, c.region AS region
