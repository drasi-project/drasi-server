MATCH (n:FloorObject)
RETURN collect(n.payload) AS objects
