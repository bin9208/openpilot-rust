use crate::{
    geometry::{path_after_distance, relative_xy, Point},
    geos::Geos,
    Error,
};
use openpilot_navd::geometry::{limit_route_points, Coordinate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize)]
pub struct RouteState {
    pub points: Vec<Point>,
    pub start_index: i64,
    pub active: bool,
    pub navd_active: bool,
    pub last_active_carrot: i64,
    pub session_id: String,
    pub sequence: i64,
    pub owned: bool,
}

#[derive(Debug, Deserialize)]
pub struct RouteUpdate {
    pub session_id: String,
    pub sequence: i64,
    pub present: bool,
    pub polyline: Vec<Point>,
    pub force: bool,
    pub onroad: bool,
}

#[derive(Debug, Deserialize)]
pub struct RouteInput {
    pub onroad: bool,
    pub active_carrot: i64,
    pub position: Point,
    pub heading_deg: f64,
    pub road_limit: f64,
    pub deceleration: f64,
    pub v_ego: f64,
}

#[derive(Debug, Serialize)]
pub struct RouteOutput {
    pub points: Vec<Point>,
    pub distances: Vec<f64>,
    pub speed: f64,
}

impl RouteState {
    pub fn send_routes(
        &mut self,
        coordinates: &[Coordinate],
        from_navd: bool,
    ) -> Result<Vec<Coordinate>, Error> {
        let coordinates = limit_route_points(coordinates, 4096)?;
        if from_navd {
            self.points = coordinates
                .iter()
                .map(|c| (c.longitude, c.latitude))
                .collect();
            self.start_index = 0;
            self.active = !coordinates.is_empty();
            self.navd_active = self.active;
        }
        Ok(coordinates)
    }

    pub fn update(
        &mut self,
        update: Option<RouteUpdate>,
        force: bool,
    ) -> Result<Option<Vec<Coordinate>>, Error> {
        let points = match update {
            None => {
                if self.session_id.is_empty() {
                    return Ok(None);
                }
                self.session_id.clear();
                self.sequence = -1;
                if force {
                    self.owned = false;
                    return Ok(None);
                }
                if !self.owned {
                    return Ok(None);
                }
                Vec::new()
            }
            Some(update) => {
                let new_session = update.session_id != self.session_id;
                if !force && !new_session && update.sequence == self.sequence {
                    let available = update.present && !update.polyline.is_empty();
                    if !available || self.active || !update.onroad {
                        return Ok(None);
                    }
                }
                self.session_id = update.session_id;
                self.sequence = update.sequence;
                let points = if update.present {
                    update.polyline
                } else {
                    Vec::new()
                };
                if points.is_empty() && force {
                    self.owned = false;
                    return Ok(None);
                }
                if points.is_empty() && !self.owned {
                    return Ok(None);
                }
                points
            }
        };
        let coords: Vec<_> = points.iter().map(|p| Coordinate::new(p.0, p.1)).collect();
        self.points = points.iter().map(|p| (p.1, p.0)).collect();
        self.start_index = 0;
        self.active = !points.is_empty();
        self.owned = self.active;
        self.navd_active = self.active;
        Ok(Some(self.send_routes(&coords, false)?))
    }

    /// The route point `distance` m past the car's projection and the route's
    /// bearing there (north 0, east 90); None past the route's end.
    pub fn ahead(&self, position: Point, distance: f64) -> Option<(Point, f64)> {
        if !self.active || self.points.len() < 2 || !distance.is_finite() {
            return None;
        }
        let path = path_after_distance(self.start_index, &self.points, position, distance.max(1.));
        let total: f64 = path
            .points
            .windows(2)
            .map(|w| crate::geometry::haversine(w[0], w[1]))
            .sum();
        if total + 1. < distance {
            return None;
        }
        let end = *path.points.last()?;
        let from = path
            .points
            .iter()
            .rev()
            .find(|p| crate::geometry::haversine(**p, end) > 2.)?;
        let bearing = ((end.0 - from.0) * from.1.to_radians().cos())
            .atan2(end.1 - from.1)
            .to_degrees()
            .rem_euclid(360.);
        Some((end, bearing))
    }

    pub fn preview(
        &mut self,
        input: RouteInput,
        geos: Option<&Geos>,
    ) -> Result<RouteOutput, Error> {
        if !input.onroad
            || !self.active
            || geos.is_none()
            || (input.active_carrot <= 1 && !self.navd_active)
        {
            if self.active {
                self.points.clear();
                self.active = false;
            }
            self.last_active_carrot = input.active_carrot;
            return Ok(RouteOutput {
                points: Vec::new(),
                distances: Vec::new(),
                speed: 300.,
            });
        }
        let geos = geos.ok_or(Error::Geometry)?;
        let path = path_after_distance(self.start_index, &self.points, input.position, 300.);
        self.start_index = path.start_index;
        let Some(reference) = path.closest else {
            return Ok(RouteOutput {
                points: Vec::new(),
                distances: Vec::new(),
                speed: 300.,
            });
        };
        let relative = relative_xy(&path.points, reference, input.heading_deg);
        let (points, distances) = geos.sample(&relative)?;
        let speed = curve_speed(&relative, input.v_ego, input.deceleration);
        Ok(RouteOutput {
            points,
            distances,
            speed,
        })
    }
}

/// Route curve speed (km/h, 300 = none) for the route ahead, in the car's
/// frame (metres). carrot took the curvature of three points 40 m apart and a
/// fixed table: on the polyline's corners and digitisation it jumped, and at
/// intersection turns it allowed about 35 km/h more than drivers took (NAS
/// drives 2026-11-28: radius under 40 m, drivers 21 km/h, carrot +34.7, this
/// -1.2 to +1.5 km/h), while gentle highway curves of 500-600 m radius fell
/// to about 108 km/h. Here the route is resampled every STEP m, its heading
/// smoothed with a SIGMA m Gaussian (a polyline corner becomes the arc a car
/// takes), and each point allows sqrt(LATERAL / curvature), approached after
/// RESPONSE_S at the configured deceleration.
pub(crate) fn curve_speed(path: &[Point], v_ego: f64, deceleration: f64) -> f64 {
    const STEP: f64 = 2.;
    const SIGMA: f64 = 6.;
    const LATERAL: f64 = 1.8;
    const RESPONSE_S: f64 = 1.;
    const NONE: f64 = 300.;
    let Some(start) = (0..path.len()).min_by(|a, b| {
        path[*a]
            .0
            .hypot(path[*a].1)
            .total_cmp(&path[*b].0.hypot(path[*b].1))
    }) else {
        return NONE;
    };
    let path = &path[start..];
    let mut along = vec![0.];
    for pair in path.windows(2) {
        let last = *along.last().unwrap_or(&0.);
        along.push(last + (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1));
    }
    let total = *along.last().unwrap_or(&0.);
    if !total.is_finite() || total < 4. * STEP {
        return NONE;
    }
    let count = (total / STEP) as usize + 1;
    let mut samples = Vec::with_capacity(count);
    let mut segment = 0;
    for i in 0..count {
        let s = i as f64 * STEP;
        while segment + 2 < along.len() && along[segment + 1] < s {
            segment += 1;
        }
        let span = (along[segment + 1] - along[segment]).max(1e-9);
        let f = ((s - along[segment]) / span).clamp(0., 1.);
        let (a, b) = (path[segment], path[segment + 1]);
        samples.push((a.0 + f * (b.0 - a.0), a.1 + f * (b.1 - a.1)));
    }
    let mut heading = Vec::with_capacity(samples.len() - 1);
    for pair in samples.windows(2) {
        let mut angle = (pair[1].1 - pair[0].1).atan2(pair[1].0 - pair[0].0);
        if let Some(previous) = heading.last() {
            while angle - previous > std::f64::consts::PI {
                angle -= 2. * std::f64::consts::PI;
            }
            while angle - previous < -std::f64::consts::PI {
                angle += 2. * std::f64::consts::PI;
            }
        }
        heading.push(angle);
    }
    let radius = (3. * SIGMA / STEP) as isize;
    let weights: Vec<f64> = (-radius..=radius)
        .map(|k| (-0.5 * (k as f64 * STEP / SIGMA).powi(2)).exp())
        .collect();
    let norm: f64 = weights.iter().sum();
    let last = heading.len() as isize - 1;
    let smooth: Vec<f64> = (0..heading.len() as isize)
        .map(|i| {
            weights
                .iter()
                .zip(-radius..=radius)
                .map(|(w, k)| w * heading[(i + k).clamp(0, last) as usize])
                .sum::<f64>()
                / norm
        })
        .collect();
    let mut best = f64::INFINITY;
    for i in 0..smooth.len() {
        let (a, b) = (i.saturating_sub(1), (i + 1).min(smooth.len() - 1));
        let curvature = if b > a {
            ((smooth[b] - smooth[a]) / ((b - a) as f64 * STEP)).abs()
        } else {
            0.
        };
        let curve_ms = (LATERAL / curvature.max(1e-6)).sqrt();
        let room = (i as f64 * STEP - v_ego.max(0.) * RESPONSE_S).max(0.);
        best = best.min((curve_ms * curve_ms + 2. * deceleration.max(0.1) * room).sqrt());
    }
    NONE.min(best * 3.6)
}

#[cfg(test)]
mod curve_speed_tests {
    use super::curve_speed;

    fn straight_then(turn_at: f64, radius: f64, angle_deg: f64) -> Vec<(f64, f64)> {
        let mut path: Vec<(f64, f64)> =
            (0..=(turn_at as i32)).map(|x| (f64::from(x), 0.)).collect();
        let steps = (radius * angle_deg.to_radians()).ceil().max(1.) as i32;
        for i in 1..=steps {
            let a = angle_deg.to_radians() * f64::from(i) / f64::from(steps);
            path.push((turn_at + radius * a.sin(), -radius * (1. - a.cos())));
        }
        let end = *path.last().unwrap();
        let heading = -angle_deg.to_radians();
        for d in 1..=200 {
            path.push((
                end.0 + f64::from(d) * heading.cos(),
                end.1 + f64::from(d) * heading.sin(),
            ));
        }
        path
    }

    #[test]
    fn a_straight_route_sets_no_limit() {
        let path: Vec<(f64, f64)> = (-20..=300).map(|x| (f64::from(x), 0.)).collect();
        assert!(curve_speed(&path, 20., 1.2) >= 299.);
    }

    #[test]
    fn a_gentle_highway_curve_keeps_highway_speed() {
        // radius 560 m from 20 m ahead: carrot's table gave ~108 km/h here
        let speed = curve_speed(&straight_then(20., 560., 25.), 30.5, 1.2);
        assert!((110. ..120.).contains(&speed), "{speed}");
    }

    #[test]
    fn a_sharp_corner_ahead_slows_an_approach() {
        // a polyline corner (radius 0) 30 m ahead, at 40 km/h
        let corner = straight_then(30., 0.01, 90.);
        let speed = curve_speed(&corner, 40. / 3.6, 1.2);
        assert!((15. ..35.).contains(&speed), "{speed}");
        // further away it allows more
        let far = curve_speed(&straight_then(120., 0.01, 90.), 40. / 3.6, 1.2);
        assert!(far > 50., "{far}");
    }

    #[test]
    fn too_little_route_sets_no_limit() {
        assert!(curve_speed(&[(0., 0.), (3., 0.)], 10., 1.2) >= 299.);
        assert!(curve_speed(&[], 10., 1.2) >= 299.);
    }
}
