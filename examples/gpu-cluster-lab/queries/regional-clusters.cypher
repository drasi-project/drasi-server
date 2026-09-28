MATCH (c:regional_clusters)
RETURN c.cluster_id AS cluster_id, c.name AS name, c.region AS region, c.revision AS revision
