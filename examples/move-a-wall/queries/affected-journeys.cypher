MATCH (c:Cart)-[:CART_TASK]->(j:Journey)-[:TO_DESTINATION]->(d:Destination),
      (o:Obstruction)-[:BLOCKS]->(j),
      (o)-[:CAUSED_BY]->(b:Obstacle)
WHERE c.floor = j.floor AND d.floor = j.floor AND b.floor = j.floor
RETURN o.id AS id, c.id AS cart_id, c.name AS cart,
       j.id AS journey_id, j.name AS task,
       d.id AS destination_id, d.name AS destination,
       b.id AS obstacle_id, b.name AS obstacle,
       o.distance_m AS distance_m, o.required_m AS required_m
