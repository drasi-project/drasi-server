use crate::model::{validate_scene, Scene, Shape, EPSILON};
use anyhow::Result;
use geo::{Distance, Euclidean, LineString, Polygon};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Obstruction {
    pub id: String,
    pub journey_id: String,
    pub cart_id: String,
    pub obstacle_id: String,
    pub floor: String,
    pub distance_m: f64,
    pub required_m: f64,
}

pub fn calculate(scene: &Scene) -> Result<BTreeMap<String, Obstruction>> {
    validate_scene(scene)?;
    let mut results = BTreeMap::new();
    for journey in scene.values().filter(|e| e.active) {
        let Shape::Journey {
            cart_id, points, ..
        } = &journey.shape
        else {
            continue;
        };
        let Some(cart) = scene
            .get(cart_id)
            .filter(|c| c.active && c.floor == journey.floor)
        else {
            continue;
        };
        let Shape::Cart { radius, clearance } = cart.shape else {
            continue;
        };
        let line = LineString::from(points.iter().map(|p| (p[0], p[1])).collect::<Vec<_>>());
        for obstacle in scene
            .values()
            .filter(|e| e.active && e.floor == journey.floor)
        {
            let Shape::Obstacle { vertices } = &obstacle.shape else {
                continue;
            };
            let polygon = Polygon::new(
                LineString::from(vertices.iter().map(|p| (p[0], p[1])).collect::<Vec<_>>()),
                vec![],
            );
            let distance = Euclidean.distance(&line, &polygon);
            if distance <= radius + clearance + EPSILON {
                let id = format!("{}/{}", journey.id, obstacle.id);
                results.insert(
                    id.clone(),
                    Obstruction {
                        id,
                        journey_id: journey.id.clone(),
                        cart_id: cart.id.clone(),
                        obstacle_id: obstacle.id.clone(),
                        floor: journey.floor.clone(),
                        distance_m: distance,
                        required_m: radius + clearance,
                    },
                );
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    fn wall(y: f64) -> Scene {
        let mut s = fixture();
        s.get_mut("wall").unwrap().shape = Shape::Obstacle {
            vertices: vec![[10.0, y], [13.0, y], [13.0, y + 1.0], [10.0, y + 1.0]],
        };
        s
    }
    #[test]
    fn crossing_near_miss_contact_and_clearance() -> Result<()> {
        assert!(calculate(&fixture())?.is_empty());
        assert!(calculate(&wall(4.5))?.contains_key("journey-ada/wall"));
        assert!(
            calculate(&wall(5.5))?.contains_key("journey-ada/wall"),
            "finite footprint, not just line intersection"
        );
        assert!(
            calculate(&wall(5.65))?.contains_key("journey-ada/wall"),
            "tangency counts"
        );
        assert!(calculate(&wall(5.65001))?.is_empty());
        Ok(())
    }
    #[test]
    fn turns_overlaps_and_separate_floors() -> Result<()> {
        let mut s = wall(11.5);
        assert!(calculate(&s)?.contains_key("journey-grace/wall"));
        let mut other = s["wall"].clone();
        other.id = "second".into();
        s.insert(other.id.clone(), other);
        assert_eq!(calculate(&s)?.len(), 2);
        s.get_mut("wall").unwrap().floor = "upstairs".into();
        assert_eq!(calculate(&s)?.len(), 1);
        let mut corner = fixture();
        corner.get_mut("wall").unwrap().shape = Shape::Obstacle {
            vertices: vec![[14.3, 10.2], [15.0, 10.2], [15.0, 10.7], [14.3, 10.7]],
        };
        assert!(
            calculate(&corner)?.contains_key("journey-grace/wall"),
            "the circular swept footprint covers the outside of a polyline turn"
        );
        Ok(())
    }
    #[test]
    fn malformed_values_and_unsupported_shapes_fail() {
        let mut s = fixture();
        s.get_mut("wall").unwrap().shape = Shape::Obstacle {
            vertices: vec![[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]],
        };
        assert!(calculate(&s).is_err());
        s.get_mut("wall").unwrap().shape = Shape::Obstacle {
            vertices: vec![[0.0, 0.0], [2.0, 0.0], [1.0, 0.0]],
        };
        assert!(calculate(&s).is_err());
        s = fixture();
        s.get_mut("cart-ada").unwrap().shape = Shape::Cart {
            radius: f64::NAN,
            clearance: 0.0,
        };
        assert!(calculate(&s).is_err());
        s = fixture();
        s.get_mut("journey-ada").unwrap().shape = Shape::Journey {
            cart_id: "cart-ada".into(),
            destination_id: "packing".into(),
            points: vec![[1., 1.], [1., 1.]],
        };
        assert!(calculate(&s).is_err());
        s = fixture();
        s.get_mut("packing").unwrap().shape = Shape::Destination { point: [-1., 0.] };
        assert!(calculate(&s).is_err());
        assert!(serde_json::from_str::<Shape>(r#"{"kind":"circle","radius":1}"#).is_err());
    }
}
