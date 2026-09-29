MATCH (n:FloorObject)
WHERE n.active = true
RETURN collect(n.payload) AS objects
