use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Point = [f64; 2];
pub type Scene = BTreeMap<String, Entity>;
pub const EPSILON: f64 = 1e-7;
pub const WIDTH: f64 = 24.0;
pub const HEIGHT: f64 = 16.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shape {
    Cart {
        radius: f64,
        clearance: f64,
    },
    Journey {
        cart_id: String,
        destination_id: String,
        points: Vec<Point>,
    },
    Destination {
        point: Point,
    },
    Obstacle {
        vertices: Vec<Point>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub shape: Shape,
}

fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "IDs must be 1-64 ASCII letters, digits, hyphens or underscores"
    );
    Ok(())
}
fn point(p: Point) -> Result<()> {
    ensure!(
        p.iter().all(|n| n.is_finite())
            && (0.0..=WIDTH).contains(&p[0])
            && (0.0..=HEIGHT).contains(&p[1]),
        "coordinates must be finite and within the 24 x 16 scene"
    );
    Ok(())
}

impl Entity {
    pub fn validate(&self) -> Result<()> {
        identifier(&self.id)?;
        ensure!(self.id != "__clock", "__clock is reserved by the source");
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 100,
            "name must be 1-100 bytes"
        );
        match &self.shape {
            Shape::Cart { radius, clearance } => {
                ensure!(
                    radius.is_finite() && (0.05..=2.0).contains(radius),
                    "radius must be 0.05-2"
                );
                ensure!(
                    clearance.is_finite() && (0.0..=2.0).contains(clearance),
                    "clearance must be 0-2"
                );
            }
            Shape::Journey {
                cart_id,
                destination_id,
                points,
            } => {
                identifier(cart_id)?;
                identifier(destination_id)?;
                ensure!((2..=32).contains(&points.len()), "a path needs 2-32 points");
                for p in points {
                    point(*p)?;
                }
                ensure!(
                    points.windows(2).all(|p| distance(p[0], p[1]) > EPSILON),
                    "adjacent path points must differ"
                );
            }
            Shape::Destination { point: p } => point(*p)?,
            Shape::Obstacle { vertices } => {
                ensure!(
                    (3..=16).contains(&vertices.len()),
                    "an obstacle needs 3-16 vertices, without a closing duplicate"
                );
                for p in vertices {
                    point(*p)?;
                }
                // Every other vertex must lie strictly on the same side of each edge.
                // This rejects concave, self-crossing, duplicate and collinear rings.
                let mut orientation = 0.0_f64;
                for i in 0..vertices.len() {
                    let a = vertices[i];
                    let b = vertices[(i + 1) % vertices.len()];
                    ensure!(
                        distance(a, b) > EPSILON,
                        "polygon edges must have nonzero length"
                    );
                    for (j, c) in vertices.iter().enumerate() {
                        if j == i || j == (i + 1) % vertices.len() {
                            continue;
                        }
                        let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
                        ensure!(
                            cross.abs() > EPSILON,
                            "polygon must be strictly convex, with no collinear vertices"
                        );
                        if orientation == 0.0 {
                            orientation = cross.signum();
                        }
                        ensure!(
                            cross.signum() == orientation,
                            "only simple convex polygons are supported"
                        );
                    }
                }
            }
        }
        Ok(())
    }
    pub fn label(&self) -> &'static str {
        match self.shape {
            Shape::Cart { .. } => "Cart",
            Shape::Journey { .. } => "Journey",
            Shape::Destination { .. } => "Destination",
            Shape::Obstacle { .. } => "Obstacle",
        }
    }
}

pub fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

pub fn validate_scene(scene: &Scene) -> Result<()> {
    ensure!(scene.len() <= 64, "the demo supports at most 64 entities");
    for (id, entity) in scene {
        ensure!(id == &entity.id, "entity identity cannot change");
        entity.validate()?;
    }
    Ok(())
}

pub fn fixture() -> Scene {
    let mut scene = Scene::new();
    let mut add = |id: &str, name: &str, shape| {
        scene.insert(
            id.into(),
            Entity {
                id: id.into(),
                name: name.into(),
                active: true,
                shape,
            },
        );
    };
    add(
        "cart-ada",
        "Ada",
        Shape::Cart {
            radius: 0.45,
            clearance: 0.2,
        },
    );
    add(
        "cart-grace",
        "Grace",
        Shape::Cart {
            radius: 0.4,
            clearance: 0.2,
        },
    );
    add(
        "packing",
        "Packing station",
        Shape::Destination { point: [21.0, 5.0] },
    );
    add(
        "assembly",
        "Assembly bench",
        Shape::Destination {
            point: [21.0, 14.0],
        },
    );
    add(
        "journey-ada",
        "Deliver packaging",
        Shape::Journey {
            cart_id: "cart-ada".into(),
            destination_id: "packing".into(),
            points: vec![[2.0, 5.0], [21.0, 5.0]],
        },
    );
    add(
        "journey-grace",
        "Bring assembly parts",
        Shape::Journey {
            cart_id: "cart-grace".into(),
            destination_id: "assembly".into(),
            points: vec![
                [2.0, 14.0],
                [2.599, 13.321],
                [3.246, 12.689],
                [3.938, 12.105],
                [4.671, 11.575],
                [5.441, 11.099],
                [6.244, 10.682],
                [7.076, 10.325],
                [7.931, 10.03],
                [8.806, 9.799],
                [9.696, 9.633],
                [10.596, 9.533],
                [11.5, 9.5],
                [12.404, 9.533],
                [13.304, 9.633],
                [14.194, 9.799],
                [15.069, 10.03],
                [15.924, 10.325],
                [16.756, 10.682],
                [17.559, 11.099],
                [18.329, 11.575],
                [19.062, 12.105],
                [19.754, 12.689],
                [20.401, 13.321],
                [21.0, 14.0],
            ],
        },
    );
    add(
        "wall",
        "Movable wall",
        Shape::Obstacle {
            vertices: vec![[10.0, 7.0], [13.0, 7.0], [13.0, 8.0], [10.0, 8.0]],
        },
    );
    add(
        "pallet",
        "Stored pallet",
        Shape::Obstacle {
            vertices: vec![[5.0, 1.0], [7.0, 1.0], [7.0, 2.5], [5.0, 2.5]],
        },
    );
    add(
        "maintenance",
        "Maintenance zone",
        Shape::Obstacle {
            vertices: vec![[18.0, 8.0], [21.0, 8.0], [22.0, 10.0], [19.0, 10.0]],
        },
    );
    scene
}
