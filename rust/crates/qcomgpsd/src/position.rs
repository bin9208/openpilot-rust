use crate::{reports::Position, Error};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::gps_location_data::{self, SensorSource};
use std::f64::consts::PI;
fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Protocol("float32 conversion"))
}
pub fn timestamp(week: u16, millis: u32) -> Result<i64, Error> {
    let seconds = 0.001 * f64::from(millis) - 18.;
    let whole = seconds
        .trunc()
        .to_i64()
        .ok_or(Error::Protocol("GPS seconds"))?;
    let micros = (seconds.fract() * 1e6)
        .round_ties_even()
        .to_i64()
        .ok_or(Error::Protocol("GPS microseconds"))?;
    let total = ((315_964_800 + i64::from(week) * 604_800 + whole) * 1_000_000) + micros;
    let seconds = total
        .div_euclid(1_000_000)
        .to_f64()
        .ok_or(Error::Protocol("GPS epoch"))?
        + total
            .rem_euclid(1_000_000)
            .to_f64()
            .ok_or(Error::Protocol("GPS fractional epoch"))?
            / 1e6;
    (seconds * 1e3)
        .to_i64()
        .ok_or(Error::Protocol("GPS timestamp"))
}
pub fn fill(mut target: gps_location_data::Builder<'_>, data: &Position) -> Result<bool, Error> {
    let velocity = [
        data.q_flt_vel_enu_mps[1],
        data.q_flt_vel_enu_mps[0],
        -data.q_flt_vel_enu_mps[2],
    ];
    let sigma = [
        data.q_flt_vel_sigma_mps[1],
        data.q_flt_vel_sigma_mps[0],
        -data.q_flt_vel_sigma_mps[2],
    ];
    target.set_latitude(data.t_dbl_final_pos_lat_lon[0] * 180. / PI);
    target.set_longitude(data.t_dbl_final_pos_lat_lon[1] * 180. / PI);
    target.set_altitude(f64::from(data.q_flt_final_pos_alt));
    target.set_speed(float(
        velocity
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
            .sqrt(),
    )?);
    target.set_bearing_deg(float(f64::from(data.q_flt_heading_rad) * 180. / PI)?);
    target.set_unix_timestamp_millis(timestamp(data.w_gps_week_number, data.q_gps_fix_time_ms)?);
    target.set_source(SensorSource::Qcomdiag);
    target.set_v_n_e_d(&velocity)?;
    target.set_vertical_accuracy(data.q_flt_vdop);
    target.set_bearing_accuracy_deg(if data.q_flt_heading_unc_rad != 0. {
        float(f64::from(data.q_flt_heading_unc_rad) * 180. / PI)?
    } else {
        180.
    });
    target.set_speed_accuracy(float(
        sigma
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
            .sqrt(),
    )?);
    let has_fix = data.q_flt_vdop != 500.;
    target.set_has_fix(has_fix);
    Ok(has_fix)
}
