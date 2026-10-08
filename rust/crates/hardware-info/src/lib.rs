//! Read-only hardware information from system/hardware. Board control and the
//! hardwared loop remain separate; no method here launches a Python runtime.
mod error;
mod io;
mod keyfile;
mod networks;
mod numeric;
pub mod paths;
mod thermal;
mod tici;
mod value;

pub use error::Error;
pub use io::{
    get_cmdline, get_default_route_iface, wpa_supplicant_cmd, Commands, HardwarePaths,
    NativeCommands,
};
pub use networks::{CellNetwork, Networks, WlanNetwork};
pub use numeric::Number;
pub use openpilot_logmessaged::{JsonValue, JsonView};
pub use thermal::{ThermalConfig, ThermalZone};
pub fn parse_float(text: &str) -> Result<f64, Error> {
    numeric::float(text)
}
pub use tici::Tici;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkType(pub u16);
impl NetworkType {
    pub const NONE: Self = Self(0);
    pub const WIFI: Self = Self(1);
    pub const CELL_2G: Self = Self(2);
    pub const CELL_3G: Self = Self(3);
    pub const CELL_4G: Self = Self(4);
    pub const CELL_5G: Self = Self(5);
    pub const ETHERNET: Self = Self(6);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum NetworkStrength {
    Unknown = 0,
    Poor = 1,
    Moderate = 2,
    Good = 3,
    Great = 4,
}

impl NetworkStrength {
    pub const fn ordinal(self) -> u16 {
        match self {
            Self::Unknown => 0,
            Self::Poor => 1,
            Self::Moderate => 2,
            Self::Good => 3,
            Self::Great => 4,
        }
    }
}

pub fn default_network_metered(network: NetworkType) -> bool {
    !matches!(
        network,
        NetworkType::NONE | NetworkType::WIFI | NetworkType::ETHERNET
    )
}

/// HardwareBase defaults. Implementations only override source-defined behavior.
/// Raw modem JSON fields deliberately retain non-string, nonfinite and null
/// values: consumers must apply their own source-compatible conversion boundary.
pub trait HardwareInfo {
    fn get_device_type(&self) -> Result<String, Error>;
    fn booted(&self) -> Result<bool, Error> {
        Ok(true)
    }
    fn get_os_version(&self) -> Result<Option<String>, Error> {
        Ok(None)
    }
    fn get_serial(&self) -> Result<String, Error> {
        Ok(String::new())
    }
    fn get_imei(&self, _slot: usize) -> Result<JsonValue, Error> {
        Ok(JsonValue::text(""))
    }
    fn get_network_info(&self) -> Result<Option<JsonValue>, Error> {
        Ok(None)
    }
    fn get_network_type(&self) -> Result<NetworkType, Error> {
        Ok(NetworkType::NONE)
    }
    fn get_sim_info(&self) -> Result<JsonValue, Error> {
        Ok(JsonValue::parse(
            r#"{"sim_id":"","mcc_mnc":null,"network_type":["Unknown"],"sim_state":["ABSENT"],"data_connected":false}"#,
        )?)
    }
    fn get_network_strength(&self, _network: NetworkType) -> Result<NetworkStrength, Error> {
        Ok(NetworkStrength::Unknown)
    }
    fn get_network_metered(&self, network: NetworkType) -> Result<bool, Error> {
        Ok(default_network_metered(network))
    }
    fn get_current_power_draw(&self) -> Result<Number, Error> {
        Ok(Number::zero())
    }
    fn get_som_power_draw(&self) -> Result<Number, Error> {
        Ok(Number::zero())
    }
    fn get_screen_brightness(&self) -> Result<Number, Error> {
        Ok(Number::zero())
    }
    fn get_gpu_usage_percent(&self) -> Result<Number, Error> {
        Ok(Number::zero())
    }
    fn get_thermal_config(&self) -> Result<ThermalConfig, Error> {
        Ok(ThermalConfig::default())
    }
    fn get_modem_version(&self) -> Result<JsonValue, Error> {
        Ok(JsonValue::parse("null")?)
    }
    fn get_modem_temperatures(&self) -> Result<JsonValue, Error> {
        Ok(JsonValue::parse("[]")?)
    }
    fn get_networks(&self) -> Result<Option<Networks>, Error> {
        Ok(None)
    }
    fn has_internal_panda(&self) -> bool {
        false
    }
    fn get_modem_data_usage(&self) -> Result<(JsonValue, JsonValue), Error> {
        Ok((JsonValue::parse("-1")?, JsonValue::parse("-1")?))
    }
    fn get_voltage(&self) -> Result<Number, Error> {
        Ok(Number::Float(0.0))
    }
    fn get_current(&self) -> Result<Number, Error> {
        Ok(Number::Float(0.0))
    }
}

pub struct Pc;
impl HardwareInfo for Pc {
    fn get_device_type(&self) -> Result<String, Error> {
        Ok("pc".into())
    }
    fn get_network_type(&self) -> Result<NetworkType, Error> {
        Ok(NetworkType::WIFI)
    }
}

pub fn for_runtime() -> Box<dyn HardwareInfo> {
    if std::path::Path::new("/TICI").is_file() {
        Box::new(Tici::default())
    } else {
        Box::new(Pc)
    }
}
