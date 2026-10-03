use super::Endpoint;
use crate::{geometry::Coordinate, instructions::BannerInstruction, Error};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub token: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub latitude: f64,
    pub longitude: f64,
    pub bearing: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum SpeedLimitSign {
    Mutcd,
    Vienna,
}

#[derive(Debug)]
pub struct Maneuver {
    pub distance: f64,
    pub kind: Option<String>,
    pub modifier: Option<String>,
}

#[derive(Debug, Default)]
pub struct Instruction {
    pub valid: bool,
    pub maneuver_distance: f64,
    pub banner: Option<BannerInstruction>,
    pub maneuvers: Vec<Maneuver>,
    pub distance_remaining: f64,
    pub time_remaining: f64,
    pub time_remaining_typical: f64,
    pub speed_limit: Option<f64>,
    pub speed_limit_sign: Option<SpeedLimitSign>,
}

#[derive(Debug)]
pub enum Diagnostic {
    NewDestination {
        new: Endpoint,
        previous: Option<Endpoint>,
        place: Option<Value>,
    },
    Calculating {
        from: Endpoint,
        to: Endpoint,
    },
    EmptyRoute,
    RequestFailed(RequestError),
    ComputeFailed(Error),
    RouteLimited {
        original: usize,
        sent: usize,
    },
    DestinationReached,
    SpeedLimit(f64),
}

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("route request interrupted")]
    Interrupted,
    #[error("route request failed: {0}")]
    Transport(String),
    #[error("route request returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub trait Ports {
    fn parameter(&mut self, name: &str) -> Result<Option<String>, Error>;
    fn remove_parameter(&mut self, name: &str) -> Result<(), Error>;
    fn request(&mut self, url: &str) -> Result<Value, RequestError>;
    fn instruction(&mut self, message: &Instruction) -> Result<(), Error>;
    fn route(&mut self, coordinates: &[Coordinate]) -> Result<(), Error>;
    fn diagnostic(&mut self, event: Diagnostic);
    fn geometry_changed(
        &mut self,
        _coordinates: Result<Vec<Coordinate>, Error>,
    ) -> Result<(), Error> {
        Ok(())
    }
}
