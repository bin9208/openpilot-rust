use super::Error;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Clone, Deserialize)]
pub struct Platform {
    pub candidate: String,
    pub brand: String,
    pub mass: f64,
    pub wheelbase: f64,
    pub steer_ratio: f64,
    pub center_front_ratio: f64,
    pub min_enable_speed: f64,
    pub min_steer_speed: f64,
    pub tire_stiffness_factor: f64,
    pub flags: u32,
    pub dbc_pt: Option<String>,
    pub dbc_radar: Option<String>,
    torque: Option<Torque>,
}
impl Platform {
    pub(super) fn torque(&self) -> Result<&Torque, Error> {
        self.torque
            .as_ref()
            .ok_or_else(|| Error::MissingTorque(self.candidate.clone()))
    }
}

#[derive(Clone, Deserialize)]
pub(super) struct Torque {
    #[serde(rename = "LAT_ACCEL_FACTOR")]
    pub lat_accel_factor: Option<f64>,
    #[serde(rename = "MAX_LAT_ACCEL_MEASURED")]
    pub max_lateral_accel: f64,
    #[serde(rename = "FRICTION")]
    pub friction: Option<f64>,
}

#[derive(Deserialize)]
pub(super) struct Catalog {
    pub platforms: Vec<Platform>,
    pub ff_files: Vec<String>,
    pub speed_gain: [f64; 2],
}
pub(super) fn catalog() -> Result<&'static Catalog, Error> {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    if let Some(catalog) = CATALOG.get() {
        return Ok(catalog);
    }
    let value = serde_json::from_str(include_str!("../../data/vehicle.json"))?;
    Ok(CATALOG.get_or_init(|| value))
}

pub fn platform(candidate: &str) -> Result<Platform, Error> {
    catalog()?
        .platforms
        .iter()
        .find(|platform| platform.candidate == candidate)
        .cloned()
        .ok_or_else(|| Error::UnknownPlatform(candidate.to_owned()))
}

pub fn speed_gain() -> Result<[f64; 2], Error> {
    Ok(catalog()?.speed_gain)
}
