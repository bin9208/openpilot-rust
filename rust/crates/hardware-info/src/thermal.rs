use crate::{io::read_text, numeric, value, Error, JsonValue, Number};
use num_bigint::BigInt;
use num_traits::Signed;
use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct ThermalZone {
    pub name: String,
    pub scale: f64,
    pub zone_number: BigInt,
    root: PathBuf,
}
impl ThermalZone {
    pub fn new(name: &str) -> Self {
        Self::with_root(name, 1000.0, Path::new("/sys/devices/virtual/thermal"))
    }
    pub fn with_root(name: &str, scale: f64, root: &Path) -> Self {
        Self {
            name: name.into(),
            scale,
            zone_number: BigInt::from(-1),
            root: root.into(),
        }
    }
    pub fn read(&mut self) -> Result<Number, Error> {
        if self.zone_number.is_negative() {
            let entries: Result<Vec<_>, _> = std::fs::read_dir(&self.root)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect();
            for name in entries? {
                let Some(suffix) = name.as_bytes().strip_prefix(b"thermal_zone") else {
                    continue;
                };
                if numeric::trim(&read_text(&self.root.join(&name).join("type"))?) == self.name {
                    let suffix = std::str::from_utf8(suffix)
                        .map_err(|_| Error::Value("nondecimal thermal zone suffix"))?;
                    self.zone_number = numeric::integer(suffix, 10)?;
                    break;
                }
            }
        }
        let path = self
            .root
            .join(format!("thermal_zone{}/temp", self.zone_number));
        match read_text(&path) {
            Ok(text) => Ok(Number::Float(numeric::divide(
                &numeric::integer(&text, 10)?,
                self.scale,
            )?)),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Number::zero())
            }
            Err(error) => Err(error),
        }
    }
}
#[derive(Debug, Default, Clone)]
pub struct ThermalConfig {
    pub cpu: Option<Vec<ThermalZone>>,
    pub gpu: Option<Vec<ThermalZone>>,
    pub dsp: Option<ThermalZone>,
    pub pmic: Option<Vec<ThermalZone>>,
    pub memory: Option<ThermalZone>,
    pub intake: Option<ThermalZone>,
    pub exhaust: Option<ThermalZone>,
    pub gnss: Option<ThermalZone>,
    pub bottom_soc: Option<ThermalZone>,
}
impl ThermalConfig {
    pub fn get_msg(&mut self) -> Result<JsonValue, Error> {
        let mut fields = Vec::new();
        for (name, zones) in [("cpuTempC", &mut self.cpu), ("gpuTempC", &mut self.gpu)] {
            if let Some(zones) = zones {
                fields.push((name, read_zones(zones)?));
            }
        }
        if let Some(zone) = &mut self.dsp {
            fields.push(("dspTempC", zone.read()?.to_json()?));
        }
        if let Some(zones) = &mut self.pmic {
            fields.push(("pmicTempC", read_zones(zones)?));
        }
        for (name, zone) in [
            ("memoryTempC", &mut self.memory),
            ("intakeTempC", &mut self.intake),
            ("exhaustTempC", &mut self.exhaust),
            ("gnssTempC", &mut self.gnss),
            ("bottomSocTempC", &mut self.bottom_soc),
        ] {
            if let Some(zone) = zone {
                fields.push((name, zone.read()?.to_json()?));
            }
        }
        let parts: Result<Vec<_>, Error> = fields
            .into_iter()
            .map(|(name, value)| {
                Ok(format!(
                    "{}:{}",
                    JsonValue::text(name).to_json()?,
                    value.to_json()?
                ))
            })
            .collect();
        Ok(JsonValue::parse(&format!("{{{}}}", parts?.join(",")))?)
    }
}
fn read_zones(zones: &mut [ThermalZone]) -> Result<JsonValue, Error> {
    let values: Result<Vec<_>, _> = zones
        .iter_mut()
        .map(|zone| zone.read()?.to_json())
        .collect();
    value::array(&values?)
}
