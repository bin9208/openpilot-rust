use super::{parameter_coordinate, request::route_url, Diagnostic, Endpoint, Ports, RouteEngine};
use crate::{
    geometry::{minimum_distance, Coordinate},
    json::{field, number},
    Error,
};
use serde_json::Value;

impl RouteEngine {
    pub fn recompute_route<P: Ports>(&mut self, ports: &mut P) -> Result<(), Error> {
        if self.last_position.is_none() {
            return Ok(());
        }
        let (destination, place) = parameter_coordinate(ports, "NavDestination")?;
        let Some(destination) = destination else {
            self.clear_route(ports)?;
            self.reset_recompute_limits();
            return Ok(());
        };
        let external = place.as_ref().and_then(Value::as_str) == Some("External Navi");
        let mut should_recompute = self.should_recompute()? && !external;
        if self.nav_destination.as_ref().map(|value| value.point) != Some(destination.point)
            && !external
        {
            ports.diagnostic(Diagnostic::NewDestination {
                new: destination.clone(),
                previous: self.nav_destination.clone(),
                place,
            });
            should_recompute = true;
        }
        if !self.gps_ok && self.step_idx.is_some() {
            return Ok(());
        }
        if self.recompute_countdown == 0 && should_recompute {
            self.recompute_countdown = 2_u64.pow(self.recompute_backoff);
            self.recompute_backoff = (self.recompute_backoff + 1).min(6);
            self.calculate_route(destination, ports)?;
            self.reroute_counter = 0;
        } else {
            self.recompute_countdown = self.recompute_countdown.saturating_sub(1);
        }
        Ok(())
    }

    pub fn should_recompute(&mut self) -> Result<bool, Error> {
        let (Some(index), Some(route)) = (self.step_idx, self.route.as_ref()) else {
            return Ok(true);
        };
        let route = route.as_array().ok_or(Error::Field("route"))?;
        if route.len().checked_sub(1) == Some(index) {
            return Ok(false);
        }
        let geometry = self
            .route_geometry
            .as_ref()
            .and_then(|geometry| geometry.get(index))
            .ok_or(Error::Field("route_geometry"))?;
        let position = self
            .last_position
            .as_ref()
            .ok_or(Error::Field("last_position"))?
            .point;
        let mut minimum: f64 = 26.;
        for segment in geometry.windows(2) {
            let (a, b) = (segment[0], segment[1]);
            if a.distance_to(b)? < 1. {
                continue;
            }
            let distance = minimum_distance(a, b, position)?;
            if distance < minimum {
                minimum = distance;
            }
        }
        if minimum > 25. {
            self.reroute_counter = self.reroute_counter.saturating_add(1);
        } else {
            self.reroute_counter = 0;
        }
        Ok(self.reroute_counter > 3)
    }

    pub fn calculate_route<P: Ports>(
        &mut self,
        destination: Endpoint,
        ports: &mut P,
    ) -> Result<(), Error> {
        let from = self
            .last_position
            .as_ref()
            .ok_or(Error::Field("last_position"))?
            .clone();
        ports.diagnostic(Diagnostic::Calculating {
            from,
            to: destination.clone(),
        });
        self.nav_destination = Some(destination.clone());
        let url = route_url(
            ports,
            (
                self.last_position
                    .as_ref()
                    .ok_or(Error::Field("last_position"))?,
                &destination,
            ),
            (&self.config, self.last_bearing),
        )?;
        match ports.request(&url) {
            Ok(response) => {
                let routes = field(&response, "routes")?
                    .as_array()
                    .ok_or(Error::Field("routes"))?;
                if let Some(route) = routes.first() {
                    let accepted = self.accept_route(route);
                    ports.geometry_changed(self.route_coordinates())?;
                    accepted?;
                } else {
                    ports.diagnostic(Diagnostic::EmptyRoute);
                    self.clear_route(ports)?;
                }
                ports.remove_parameter("NavDestinationWaypoints")?;
            }
            Err(super::RequestError::Interrupted) => return Err(Error::Interrupted),
            Err(error) => {
                ports.diagnostic(Diagnostic::RequestFailed(error));
                self.clear_route(ports)?;
            }
        }
        self.send_route(ports)
    }

    fn accept_route(&mut self, route: &Value) -> Result<(), Error> {
        let leg = field(route, "legs")?
            .as_array()
            .and_then(|legs| legs.first())
            .ok_or(Error::Field("legs"))?;
        let steps = field(leg, "steps")?.clone();
        self.route = if steps.is_null() { None } else { Some(steps) };
        self.route_geometry = Some(Vec::new());
        let maxspeeds = field(field(leg, "annotation")?, "maxspeed")?
            .as_array()
            .ok_or(Error::Field("maxspeed"))?;
        let steps = self
            .route
            .as_ref()
            .and_then(Value::as_array)
            .ok_or(Error::Field("steps"))?;
        let mut speed_index = 0_i64;
        let speed_count = i64::try_from(maxspeeds.len()).map_err(|_| Error::GeometrySize)?;
        for step in steps {
            let coordinates = field(field(step, "geometry")?, "coordinates")?
                .as_array()
                .ok_or(Error::Field("coordinates"))?;
            let mut points = Vec::with_capacity(coordinates.len());
            for value in coordinates {
                let pair = value.as_array().ok_or(Error::Field("coordinate"))?;
                let longitude =
                    number(pair.first().ok_or(Error::Field("longitude"))?, "longitude")?;
                let latitude = number(pair.get(1).ok_or(Error::Field("latitude"))?, "latitude")?;
                let mut point = Coordinate::new(latitude, longitude);
                if speed_index < speed_count {
                    let index = if speed_index < 0 {
                        speed_count + speed_index
                    } else {
                        speed_index
                    };
                    let index = usize::try_from(index).map_err(|_| Error::Field("maxspeed"))?;
                    let speed = maxspeeds.get(index).ok_or(Error::Field("maxspeed"))?;
                    if speed.get("unknown").is_none() && speed.get("none").is_none() {
                        let multiplier = match field(speed, "unit")?.as_str() {
                            Some("km/h") => 1. / 3.6,
                            Some("mph") => 1.609344 * (1. / 3.6),
                            _ => return Err(Error::Field("speed unit")),
                        };
                        point.maxspeed =
                            Some(multiplier * number(field(speed, "speed")?, "speed")?);
                    }
                }
                points.push(point);
                speed_index = speed_index.checked_add(1).ok_or(Error::GeometrySize)?;
            }
            self.route_geometry
                .as_mut()
                .ok_or(Error::Field("route_geometry"))?
                .push(points);
            speed_index -= 1;
        }
        self.step_idx = Some(0);
        Ok(())
    }
}
