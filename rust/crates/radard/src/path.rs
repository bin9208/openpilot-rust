pub mod cache;
mod geometry;

use crate::{math::finite, Error};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Projection {
    pub path_s: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub tangent_x: f64,
    pub tangent_y: f64,
    pub d_path: f64,
}

#[derive(Clone, Debug)]
pub struct Path {
    key: Vec<[f64; 2]>,
}

impl Path {
    pub fn point_count(&self) -> usize {
        self.key.len()
    }

    pub fn new(input: &[[f64; 2]]) -> Result<Self, Error> {
        if input.is_empty() {
            return Err(Error::Contract(
                "model path is required for dPath prediction",
            ));
        }
        Ok(Self {
            key: input.to_vec(),
        })
    }

    pub fn project(&self, x: f64, y: f64) -> Projection {
        cache::project(&self.key, finite(x, 0.), finite(y, 0.))
    }

    pub fn at(&self, distance: f64, offset: f64) -> [f64; 2] {
        cache::at(&self.key, distance, offset)
    }
}
