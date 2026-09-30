MATCH (n:SceneObject)
RETURN collect(n.payload) AS objects
