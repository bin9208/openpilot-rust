use crate::{
    geometry::{curvature, path_after_distance, relative_xy, Point},
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
        let (points, distances) =
            geos.sample(&relative_xy(&path.points, reference, input.heading_deg))?;
        let mut speed = 300.;
        if points.len() >= 9 {
            let mut speeds: Vec<_> = (0..points.len() - 8)
                .map(|i| {
                    let curve = curvature(points[i], points[i + 4], points[i + 8]).abs();
                    let speed = lookup(curve);
                    if curve < 0.02 {
                        speed.max(input.road_limit)
                    } else {
                        speed
                    }
                })
                .collect();
            let accel_kph = input.deceleration * 3.6;
            let mut wait = 0.;
            for i in (0..speeds.len() - 1).rev() {
                let target = speeds[i];
                let next = speeds[i + 1];
                if target < next {
                    wait = -((input.v_ego * 3.6 - target) / accel_kph).max(0.);
                }
                let interval = if next > 0. { 10. / (next / 3.6) } else { 0. };
                let apply = interval.min((interval + wait).max(0.));
                speeds[i] = target.min(next + accel_kph * apply);
                wait += 2_f64.min(interval);
            }
            speed = speeds[0];
        }
        Ok(RouteOutput {
            points,
            distances,
            speed,
        })
    }
}

fn lookup(curve: f64) -> f64 {
    let bp = [
        0.,
        1. / 800.,
        1. / 670.,
        1. / 560.,
        1. / 440.,
        1. / 360.,
        1. / 265.,
        1. / 190.,
        1. / 135.,
        1. / 85.,
        1. / 55.,
        1. / 30.,
        1. / 25.,
    ];
    let values = [
        300., 150., 120., 110., 100., 90., 80., 70., 60., 50., 40., 15., 5.,
    ];
    for i in 1..bp.len() {
        if curve < bp[i] {
            return values[i - 1]
                + (curve - bp[i - 1]) / (bp[i] - bp[i - 1]) * (values[i] - values[i - 1]);
        }
    }
    values[values.len() - 1]
}
