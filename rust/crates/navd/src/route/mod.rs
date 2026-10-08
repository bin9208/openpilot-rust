mod instruction;
mod message;
mod recompute;
mod request;
use crate::{
    geometry::{limit_route_points, Coordinate},
    Error,
};
pub use message::{
    Config, Diagnostic, Instruction, Maneuver, Ports, Position, RequestError, SpeedLimitSign,
};
pub use request::{parameter_coordinate, Endpoint};
use serde_json::{json, Value};

pub struct RouteEngine {
    pub last_position: Option<Endpoint>,
    pub last_bearing: Option<f64>,
    pub gps_ok: bool,
    pub localizer_valid: bool,
    pub nav_destination: Option<Endpoint>,
    pub step_idx: Option<usize>,
    pub route: Option<Value>,
    pub route_geometry: Option<Vec<Vec<Coordinate>>>,
    pub recompute_backoff: u32,
    pub recompute_countdown: u64,
    pub ui_pid: Option<i32>,
    pub reroute_counter: u64,
    pub carrot_route_active: bool,
    config: Config,
}

impl RouteEngine {
    pub fn new<P: Ports>(config: Config, ports: &mut P) -> Result<Self, Error> {
        Self::new_configured(ports, |_| Ok(config))
    }

    pub fn new_configured<P: Ports>(
        ports: &mut P,
        configure: impl FnOnce(&mut P) -> Result<Config, Error>,
    ) -> Result<Self, Error> {
        let (last_position, _) = parameter_coordinate(ports, "LastGPSPosition")?;
        let config = configure(ports)?;
        Ok(Self {
            last_position,
            last_bearing: None,
            gps_ok: false,
            localizer_valid: false,
            nav_destination: None,
            step_idx: None,
            route: None,
            route_geometry: None,
            recompute_backoff: 0,
            recompute_countdown: 0,
            ui_pid: None,
            reroute_counter: 0,
            carrot_route_active: false,
            config,
        })
    }

    pub fn update_ui_pid(&mut self, pid: Option<i32>) -> bool {
        let Some(pid) = pid else {
            return false;
        };
        let restarted = self.ui_pid.is_some_and(|old| old != 0 && old != pid);
        self.ui_pid = Some(pid);
        restarted
    }

    pub fn update<P: Ports>(&mut self, position: Position, ports: &mut P) -> Result<(), Error> {
        if let Err(error) = self.update_location(position) {
            ports.diagnostic(Diagnostic::ComputeFailed(error));
            return Ok(());
        }
        if !self.carrot_route_active {
            if let Err(error) = self
                .recompute_route(ports)
                .and_then(|()| self.send_instruction(ports))
            {
                if matches!(error, Error::Interrupted) {
                    return Err(error);
                }
                ports.diagnostic(Diagnostic::ComputeFailed(error));
            }
        }
        Ok(())
    }

    fn update_location(&mut self, position: Position) -> Result<(), Error> {
        if position.latitude != 0. && position.longitude != 0. {
            self.last_bearing = Some(position.bearing);
            self.last_position = Some(Endpoint::new(Coordinate::new(
                position.latitude,
                position.longitude,
            ))?);
            self.localizer_valid = true;
            self.gps_ok = true;
        } else {
            self.localizer_valid = false;
            self.gps_ok = false;
            self.carrot_route_active = false;
        }
        Ok(())
    }

    pub fn send_route<P: Ports>(&self, ports: &mut P) -> Result<(), Error> {
        let points = self.route_coordinates()?;
        let limited = limit_route_points(&points, 4096)?;
        if limited.len() < points.len() {
            ports.diagnostic(Diagnostic::RouteLimited {
                original: points.len(),
                sent: limited.len(),
            });
        }
        ports.route(&limited)
    }

    pub fn route_coordinates(&self) -> Result<Vec<Coordinate>, Error> {
        let mut points = Vec::new();
        if self.route.is_some() {
            for geometry in self
                .route_geometry
                .as_ref()
                .ok_or(Error::Field("route_geometry"))?
            {
                points.extend_from_slice(geometry);
            }
        }
        Ok(points)
    }

    pub fn clear_route<P: Ports>(&mut self, ports: &mut P) -> Result<(), Error> {
        let had_route = self.route.is_some()
            || self.route_geometry.is_some()
            || self.step_idx.is_some()
            || self.nav_destination.is_some();
        self.route = None;
        self.route_geometry = None;
        self.step_idx = None;
        self.nav_destination = None;
        if had_route {
            ports.geometry_changed(Ok(Vec::new()))?;
            self.send_route(ports)?;
        }
        Ok(())
    }

    pub fn reset_recompute_limits(&mut self) {
        self.recompute_backoff = 0;
        self.recompute_countdown = 0;
    }

    pub fn snapshot(&self) -> Value {
        let point = |endpoint: &Endpoint| json!({"latitude":endpoint.point.latitude,"longitude":endpoint.point.longitude});
        let geometry = self.route_geometry.as_ref().map(|segments| segments.iter().map(|segment| segment.iter().map(|value| {
            let annotations = value.maxspeed.map_or_else(|| json!({}), |speed| json!({"maxspeed":speed}));
            json!({"latitude":value.latitude,"longitude":value.longitude,"annotations":annotations})
        }).collect::<Vec<_>>()).collect::<Vec<_>>());
        json!({"last_position":self.last_position.as_ref().map(point),"last_bearing":self.last_bearing,
            "gps_ok":self.gps_ok,"localizer_valid":self.localizer_valid,"nav_destination":self.nav_destination.as_ref().map(point),
            "step_idx":self.step_idx,"route":self.route,"route_geometry":geometry,"recompute_backoff":self.recompute_backoff,
            "recompute_countdown":self.recompute_countdown,"ui_pid":self.ui_pid,"reroute_counter":self.reroute_counter,
            "carrot_route_active":self.carrot_route_active})
    }
}
