use super::{discovery::Beacon, Error, Projection};
use crate::models::ClusterUiState;
use openpilot_carrot_navi::{json::Value, manifest::MapConfig, native::EmbeddedServer};

pub struct Config {
    pub host: String,
    pub port: u16,
    pub advertise_ip: Option<String>,
    pub beacon: bool,
    pub publish_cereal: bool,
    pub map_theme: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 7714,
            advertise_ip: None,
            beacon: true,
            publish_cereal: false,
            map_theme: "dark".into(),
        }
    }
}
pub struct Source {
    server: EmbeddedServer,
    beacon: Option<Beacon>,
    projection: Projection,
}
impl Source {
    /// # Errors
    /// Returns invalid map configuration, receiver startup, discovery thread or clock errors.
    pub fn new(config: Config) -> Result<Self, Error> {
        let advertised = config
            .advertise_ip
            .filter(|ip| !ip.is_empty())
            .unwrap_or_else(|| super::discovery::detect(&config.host));
        let endpoint = format!("{advertised}:{}", config.port);
        let map = MapConfig::parse(&Value::object([(
            "map_theme",
            Value::text(&config.map_theme),
        )]))?;
        let projection = Projection::new(endpoint, crate::live::monotonic()?);
        let server = EmbeddedServer::start(&config.host, config.port, map, config.publish_cereal)?;
        let beacon = if config.beacon {
            Some(Beacon::start(&advertised)?)
        } else {
            None
        };
        println!(
            "Navi receiver listening on {}:{}; discovery={}",
            config.host,
            config.port,
            if config.beacon { &advertised } else { "off" }
        );
        Ok(Self {
            server,
            beacon,
            projection,
        })
    }
    /// # Errors
    /// Returns native clock, receiver snapshot or navigation projection errors.
    pub fn update(&mut self) -> Result<ClusterUiState, Error> {
        let now = crate::live::monotonic()?;
        let wall = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
        let wall_ms = (i128::from(wall.tv_sec) * 1_000_000_000 + i128::from(wall.tv_nsec))
            .div_euclid(1_000_000);
        let (dashboard, cereal) = self.server.snapshot()?;
        self.projection.update_at(
            &dashboard,
            &cereal,
            now,
            wall_ms,
            &chrono::Local::now().format("%H:%M:%S").to_string(),
        )
    }
    /// # Errors
    /// Returns receiver metadata conversion errors.
    pub fn status_text(&self) -> Result<String, Error> {
        self.projection.status_text()
    }
    /// # Errors
    /// Returns a receiver thread shutdown error after requesting all owned workers to stop.
    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(mut beacon) = self.beacon.take() {
            beacon.close();
        }
        let result = self.server.close();
        self.projection.close();
        result.map_err(Error::from)
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
