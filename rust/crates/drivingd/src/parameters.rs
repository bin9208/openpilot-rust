use crate::{state::Settings, Error};
use openpilot_desire::types::Config;
use openpilot_params::Params;

pub fn open() -> Result<Params, Error> {
    Ok(Params::for_runtime()?)
}

fn terminated(params: &Params, key: &str) -> Result<Option<Vec<u8>>, Error> {
    let Some(mut bytes) = params.get(key)?.filter(|bytes| !bytes.is_empty()) else {
        return Ok(None);
    };
    bytes.push(0);
    Ok(Some(bytes))
}

pub fn float(params: &Params, key: &str) -> Result<f64, Error> {
    let Some(bytes) = terminated(params, key)? else {
        return Ok(0.0);
    };
    let start = bytes.as_ptr().cast();
    let mut end = std::ptr::null_mut();
    // SAFETY: the terminated input and writable end pointer outlive strtof; errno is thread-local.
    let (value, error) = unsafe {
        *libc::__errno_location() = 0;
        let value = libc::strtof(start, &mut end);
        (value, *libc::__errno_location())
    };
    if start == end || error == libc::ERANGE {
        return Err(Error::Contract("invalid float parameter"));
    }
    Ok(f64::from(value))
}

pub fn integer(params: &Params, key: &str) -> Result<i32, Error> {
    let Some(bytes) = terminated(params, key)? else {
        return Ok(0);
    };
    let start = bytes.as_ptr().cast();
    let mut end = std::ptr::null_mut();
    // SAFETY: the terminated input and writable end pointer outlive strtol; errno is thread-local.
    let (value, error) = unsafe {
        *libc::__errno_location() = 0;
        let value = libc::strtol(start, &mut end, 10);
        (value, *libc::__errno_location())
    };
    if start == end || error == libc::ERANGE {
        return Err(Error::Contract("invalid integer parameter"));
    }
    i32::try_from(value).map_err(|_| Error::Contract("integer parameter out of range"))
}

pub fn use_wide(params: &Params) -> Result<bool, Error> {
    let bytes = params
        .get("UseWideCamera")?
        .filter(|bytes| !bytes.is_empty());
    Ok(bytes.as_deref().unwrap_or(b"1") == b"1")
}

pub fn settings(params: &Params) -> Result<Settings, Error> {
    Ok(Settings {
        custom_lateral_delay: float(params, "SteerActuatorDelay")? * 0.01,
        lateral_smooth: float(params, "LatSmoothSec")? * 0.01,
        longitudinal_delay: float(params, "LongActuatorDelay")? * 0.01,
        v_ego_stopping: float(params, "VEgoStopping")? * 0.01,
        camera_yaw_trim: float(params, "CameraYawTrimDeg")? * 0.01,
    })
}

pub fn desire_config(params: &Params) -> Result<Config, Error> {
    Ok(Config {
        need_torque: integer(params, "LaneChangeNeedTorque")?,
        bsd: integer(params, "LaneChangeBsd")?,
        line_check: integer(params, "LaneLineCheck")?,
        delay_tenths: float(params, "LaneChangeDelay")?,
    })
}
