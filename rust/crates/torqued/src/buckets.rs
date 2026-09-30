use crate::{random::RandomState, Error};
use std::collections::VecDeque;
const EDGES: [f64; 9] = [-0.5, -0.3, -0.2, -0.1, 0., 0.1, 0.2, 0.3, 0.5];
const MINIMUM: [usize; 8] = [100, 300, 500, 500, 500, 500, 300, 100];

pub struct Buckets {
    points: [VecDeque<[f64; 3]>; 8],
    decimated: bool,
}
impl Buckets {
    pub fn new(decimated: bool) -> Self {
        Self {
            points: std::array::from_fn(|_| VecDeque::new()),
            decimated,
        }
    }
    pub fn add(&mut self, x: f64, y: f64) {
        for (index, edges) in EDGES.windows(2).enumerate() {
            if x >= edges[0] && x < edges[1] {
                let bucket = &mut self.points[index];
                if bucket.len() == 1500 {
                    bucket.pop_front();
                }
                bucket.push_back([x, 1., y]);
                break;
            }
        }
    }
    pub fn counts(&self) -> [usize; 8] {
        std::array::from_fn(|i| self.points[i].len())
    }
    pub fn len(&self) -> usize {
        self.points.iter().map(VecDeque::len).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn calculable(&self) -> bool {
        self.points.iter().all(|bucket| !bucket.is_empty())
    }
    fn minimum(&self, i: usize) -> usize {
        MINIMUM[i] / if self.decimated { 10 } else { 1 }
    }
    fn total_minimum(&self) -> usize {
        if self.decimated {
            600
        } else {
            4000
        }
    }
    pub fn valid(&self) -> bool {
        self.points
            .iter()
            .enumerate()
            .all(|(i, bucket)| bucket.len() >= self.minimum(i))
            && self.len() >= self.total_minimum()
    }
    pub fn percent(&self) -> i8 {
        // All values are bounded nonnegative bucket counts, and the result is in 0..=100.
        let total = (self.len() as f64 / self.total_minimum() as f64 * 100.).min(100.);
        let individual = self
            .points
            .iter()
            .enumerate()
            .map(|(i, b)| b.len() as f64 / self.minimum(i) as f64 * 100.)
            .fold(100., f64::min);
        ((total + individual) / 2.) as i8
    }
    pub fn points(&self) -> Vec<[f64; 3]> {
        self.points.iter().flat_map(|b| b.iter().copied()).collect()
    }
    pub fn sample(&self, random: &mut RandomState) -> Result<Vec<[f64; 3]>, Error> {
        let points = self.points();
        Ok(random
            .sample_indices(points.len(), if self.decimated { 600 } else { 2000 })?
            .into_iter()
            .map(|i| points[i])
            .collect())
    }
}
