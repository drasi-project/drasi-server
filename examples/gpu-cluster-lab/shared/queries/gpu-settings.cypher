MATCH (g:gpu_inventory)-[:GPU_SETTINGS]->(t:gpu_telemetry)
RETURN g.gpu_id AS gpu_id, g AS inventory, t AS settings
