use super::options::Options;
use crate::{manifest::MapConfig, Error};
use openpilot_beepd::integer;
use openpilot_params::{Error as ParamsError, Params};

pub struct MapReader {
    params: Params,
    options: Options,
}

impl MapReader {
    pub fn new(options: Options) -> Result<Self, Error> {
        Ok(Self {
            params: Params::for_runtime()
                .map_err(|error| Error::typed("ParamsError", error.to_string()))?,
            options,
        })
    }
    fn read_int(&self, key: &str) -> Result<i32, Error> {
        let bytes = match self.params.get(key) {
            Ok(bytes) => bytes.unwrap_or_default(),
            Err(ParamsError::Io(_)) => Vec::new(),
            Err(error) => return Err(Error::typed("ParamsError", error.to_string())),
        };
        integer(&bytes)
            .map_err(|error| Error::typed("SourceNativeIntegerFatal", format!("{key}: {error}")))
    }
    pub fn read(&self) -> Result<MapConfig, Error> {
        let theme = match self.read_int("ClusterNaviMapTheme")? {
            0 => "auto",
            2 => "light",
            _ => "dark",
        };
        let map_type = if self.read_int("ClusterNaviMapType")? == 1 {
            "satellite"
        } else {
            "normal"
        };
        let hz = match self.read_int("ClusterNaviMapFps")? {
            0 => 5,
            2 => 20,
            3 => 30,
            _ => 10,
        };
        let screen_center_y_ratio = if self.read_int("CarrotNaviHudMapProfile")? == 1 {
            0.68
        } else {
            0.8
        };
        Ok(MapConfig {
            theme: self
                .options
                .map_theme
                .clone()
                .unwrap_or_else(|| theme.into()),
            map_type: self
                .options
                .map_type
                .clone()
                .unwrap_or_else(|| map_type.into()),
            hz,
            bitrate_kbps: match hz {
                5 => 1500,
                30 => 6000,
                _ => 3000,
            },
            screen_center_y_ratio,
        })
    }
}
