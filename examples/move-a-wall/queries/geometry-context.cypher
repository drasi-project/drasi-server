MATCH (n:SceneObject)
WHERE n.active = true
RETURN collect(n.payload) AS objects
