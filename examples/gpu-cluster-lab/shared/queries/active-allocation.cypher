MATCH (p:gpu_placements)
RETURN p.fleet_id AS fleet_id, p AS plan
