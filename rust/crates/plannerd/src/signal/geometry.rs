//! Candidate road geometry; these intervals do not establish signal-to-lane identity.
use crate::model::Trajectory;
use openpilot_control_policy::math::interp;
use serde::Deserialize;
#[derive(Debug, Deserialize)]
pub struct Geometry {
    pub focal: f64,
    pub rpy: [f64; 3],
    pub edges: Vec<Trajectory>,
    pub edge_stds: Vec<f64>,
    pub lanes: Vec<Trajectory>,
    pub lane_probs: Vec<f64>,
    pub lane_stds: Vec<f64>,
}
#[derive(Debug)]
pub struct Corridor {
    pub left: Vec<f64>,
    pub right: Vec<f64>,
    pub z: Vec<f64>,
    pub valid: Vec<bool>,
    pub source: &'static str,
}
fn pair(
    ds: &[f64],
    curves: &[Trajectory],
    stds: &[f64],
    lanes: bool,
    inset: bool,
) -> Option<Corridor> {
    if curves.len() != 2
        || stds.len() != 2
        || !stds
            .iter()
            .all(|s| s.is_finite() && *s >= 0. && *s <= if inset && !lanes { 1. } else { 0.5 })
    {
        return None;
    }
    for c in curves {
        if c.x.len() < 2
            || c.y.len() != c.x.len()
            || c.z.len() != c.x.len()
            || !c.x.iter().chain(&c.y).chain(&c.z).all(|v| v.is_finite())
            || c.x.windows(2).any(|p| p[1] <= p[0])
        {
            return None;
        }
    }
    let mut result = Corridor {
        left: Vec::new(),
        right: Vec::new(),
        z: Vec::new(),
        valid: Vec::new(),
        source: "edges",
    };
    for &d in ds {
        let left = interp(d, &curves[0].x, &curves[0].y).ok()?;
        let right = interp(d, &curves[1].x, &curves[1].y).ok()?;
        result.left.push(left);
        result.right.push(right);
        result.z.push(
            (interp(d, &curves[0].x, &curves[0].z).ok()?
                + interp(d, &curves[1].x, &curves[1].z).ok()?)
                / 2.,
        );
        result.valid.push(
            curves
                .iter()
                .all(|c| d >= c.x[0] && d <= c.x[c.x.len() - 1])
                && right - left >= 2.
                && right - left <= if lanes { 5. } else { 20. },
        );
    }
    Some(result)
}
impl Geometry {
    pub fn corridor(
        &self,
        ds: &[f64],
        margin: f64,
        inset: bool,
        with_edges: bool,
    ) -> Option<Corridor> {
        if !margin.is_finite() || !(0. ..=5.).contains(&margin) {
            return None;
        }
        let mut road = if with_edges {
            pair(ds, &self.edges, &self.edge_stds, false, inset)
        } else {
            None
        };
        if let Some(r) = &mut road {
            for i in 0..ds.len() {
                r.left[i] += if inset {
                    0.3 + 2. * self.edge_stds[0]
                } else {
                    0.3
                };
                r.right[i] -= if inset {
                    0.3 + 2. * self.edge_stds[1]
                } else {
                    0.3
                };
                if inset {
                    r.valid[i] &= r.right[i] - r.left[i] >= 2.;
                }
            }
        }
        let lane = if self.lanes.len() == 4
            && self.lane_probs.len() == 4
            && self.lane_stds.len() == 4
            && [1, 2].iter().all(|i| {
                self.lane_probs[*i].is_finite() && (0.8..=1.).contains(&self.lane_probs[*i])
            }) {
            pair(ds, &self.lanes[1..3], &self.lane_stds[1..3], true, inset)
        } else {
            None
        };
        let Some(mut lane) = lane else {
            return road;
        };
        for i in 0..ds.len() {
            if let Some(r) = &road {
                lane.valid[i] &=
                    r.valid[i] && lane.left[i].max(r.left[i]) < lane.right[i].min(r.right[i]);
            }
            lane.left[i] -= margin;
            lane.right[i] += margin;
            if let Some(r) = &road {
                lane.left[i] = lane.left[i].max(r.left[i]);
                lane.right[i] = lane.right[i].min(r.right[i]);
            }
        }
        lane.source = if road.is_none() {
            "lanes"
        } else {
            "lanes+edges"
        };
        Some(lane)
    }
    fn valid_box(&self, b: [f64; 4]) -> bool {
        b.iter()
            .chain(&self.rpy)
            .chain(std::iter::once(&self.focal))
            .all(|v| v.is_finite())
            && self.focal > 0.
            && 0. <= b[0]
            && b[0] < b[2]
            && b[2] <= 1344.
            && 0. <= b[1]
            && b[1] < b[3]
            && b[3] <= 760.
    }
    fn ray(&self, x: f64, y: f64) -> Option<[f64; 3]> {
        let rotation = openpilot_calibrationd::orientation::rotation(self.rpy);
        let input = [1., (x - 672.) / self.focal, (y - 380.) / self.focal];
        let ray: [f64; 3] = std::array::from_fn(|i| {
            rotation[0][i] * input[0] + rotation[1][i] * input[1] + rotation[2][i] * input[2]
        });
        if ray[0] <= 0. {
            return None;
        }
        Some(ray.map(|v| v / ray[0]))
    }
    pub fn housing_range(&self, b: [f64; 4], margin: f64) -> Option<[f64; 2]> {
        let (w, h) = (b[2] - b[0], b[3] - b[1]);
        if !self.valid_box(b)
            || !(16. ..=160.).contains(&w)
            || !(4. ..=50.).contains(&h)
            || !(2.6..=6.8).contains(&(w / h))
        {
            return None;
        }
        let ray = self.ray(b[0] + 0.125 * w, (b[1] + b[3]) / 2.)?;
        let (lo, hi) = (self.focal * 0.7 / w, self.focal * 2.5 / w);
        let step = (hi - lo) / 64.;
        let ds: Vec<_> = (0..65)
            .map(|i| if i == 64 { hi } else { lo + (i as f64) * step })
            .collect();
        let c = self.corridor(&ds, margin, false, true)?;
        let possible: Vec<_> = ds
            .iter()
            .enumerate()
            .filter(|(i, d)| {
                c.valid[*i]
                    && ray[1] * **d >= c.left[*i]
                    && ray[1] * **d <= c.right[*i]
                    && (3. ..=9.).contains(&(c.z[*i] - ray[2] * **d))
            })
            .map(|(_, d)| *d)
            .collect();
        if possible.len() < 3 || possible[0] < 25. || possible[possible.len() - 1] > 140. {
            return None;
        }
        Some([possible[0], possible[possible.len() - 1]])
    }
    pub fn night_support(&self, b: [f64; 4]) -> Option<[f64; 2]> {
        if !self.valid_box(b) {
            return None;
        }
        let ray = self.ray(b[0] + 0.125 * (b[2] - b[0]), (b[1] + b[3]) / 2.)?;
        let ds: Vec<_> = (25..191).map(f64::from).collect();
        let c = self.corridor(&ds, 3.5, true, true)?;
        let possible: Vec<_> = ds
            .iter()
            .enumerate()
            .filter(|(i, d)| {
                let margin = **d / self.focal;
                let height = c.z[*i] - ray[2] * **d;
                c.valid[*i]
                    && height >= 3. + margin
                    && height <= 9. - margin
                    && ray[1] * **d >= c.left[*i] + margin
                    && ray[1] * **d <= c.right[*i] - margin
            })
            .map(|(i, _)| i)
            .collect();
        if possible.len() < 21 || possible[possible.len() - 1] - possible[0] + 1 != possible.len() {
            return None;
        }
        Some([ds[possible[0]], ds[possible[possible.len() - 1]]])
    }
    pub fn alignment(&self, b: [f64; 4], distance: [f64; 2]) -> Option<[f64; 2]> {
        let [lo, hi] = distance;
        let valid = self.valid_box(b)
            && lo.is_finite()
            && hi.is_finite()
            && 25. <= lo
            && lo < hi
            && hi <= 140.;
        if !valid {
            return None;
        }
        let mut ds = vec![lo, hi];
        ds.extend(
            self.edges
                .iter()
                .chain(&self.lanes)
                .flat_map(|c| c.x.iter())
                .filter(|v| lo < **v && **v < hi),
        );
        ds.sort_by(f64::total_cmp);
        ds.dedup();
        let c = self.corridor(&ds, 0., false, true)?;
        if c.source == "edges" || !c.valid.iter().all(|v| *v) {
            return None;
        }
        let lane = self.corridor(&ds, 0., false, false)?;
        if !lane.valid.iter().all(|v| *v) {
            return None;
        }
        // The source's 3x2 NumPy matrix product accumulates each column with
        // fused multiply-add. Its separate vector projections in range/night
        // use scalar accumulation, so retain their existing ray() arithmetic.
        let rotation = openpilot_calibrationd::orientation::rotation(self.rpy);
        let y = ((b[1] + b[3]) / 2. - 380.) / self.focal;
        let mut rays = Vec::with_capacity(2);
        for x in [b[0], b[2]] {
            let x = (x - 672.) / self.focal;
            let ray: [f64; 3] = std::array::from_fn(|i| {
                rotation[2][i].mul_add(y, rotation[1][i].mul_add(x, rotation[0][i]))
            });
            if ray[0] <= 0. {
                return None;
            }
            rays.push(ray.map(|value| value / ray[0]));
        }
        let mut bounds = [f64::INFINITY, f64::NEG_INFINITY];
        for ray in rays {
            for (i, d) in ds.iter().enumerate() {
                let half = (lane.right[i] - lane.left[i]) / 2.;
                if half < 1. {
                    return None;
                }
                let value = (ray[1] * d - (lane.left[i] + lane.right[i]) / 2.) / half;
                bounds[0] = bounds[0].min(value);
                bounds[1] = bounds[1].max(value);
            }
        }
        Some(bounds)
    }
}
