use super::{
    flags as f,
    parameters::setting_int,
    state::State,
    state_fields::float32,
    wire::{get, Values},
    Error,
};
use openpilot_cereal::car_capnp::car_state;

fn maximum(infos: &[Values], signal: &str) -> Result<f64, Error> {
    let mut result = None;
    for data in infos {
        let value = get(data, signal)?;
        result = Some(match result {
            Some(previous) => value.max(previous),
            None => value,
        });
    }
    result.ok_or(Error::Numeric)
}

pub fn update(state: &State, mut ret: car_state::Builder<'_>) -> Result<(), Error> {
    let infos = [
        state.inputs.captured("adrv_0x1ea")?,
        state.inputs.captured("ccnc_0x162")?,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if infos.is_empty() {
        return Ok(());
    }
    let front = [
        maximum(&infos, "LF_DETECT_DISTANCE")?,
        maximum(&infos, "RF_DETECT_DISTANCE")?,
    ];
    let rear = [
        maximum(&infos, "LR_DETECT_DISTANCE")?,
        maximum(&infos, "RR_DETECT_DISTANCE")?,
    ];
    ret.set_left_long_dist(float32(front[0])?);
    ret.set_right_long_dist(float32(front[1])?);
    ret.set_left_lat_dist(float32(maximum(&infos, "LF_DETECT_LATERAL")?)?);
    ret.set_right_lat_dist(float32(maximum(&infos, "RF_DETECT_LATERAL")?)?);
    ret.set_left_rear_long_dist(float32(rear[0])?);
    ret.set_right_rear_long_dist(float32(rear[1])?);
    ret.set_left_rear_lat_dist(float32(maximum(&infos, "LR_DETECT_LATERAL")?)?);
    ret.set_right_rear_lat_dist(float32(maximum(&infos, "RR_DETECT_LATERAL")?)?);
    let raw = setting_int(&state.settings, "EnableCornerRadar")? > 0
        && state.config.ext_flags & (f::ext::CORNER_235 | f::ext::CORNER_180) != 0;
    let front = [
        f64::from(ret.reborrow_as_reader().get_left_long_dist()),
        f64::from(ret.reborrow_as_reader().get_right_long_dist()),
    ];
    let rear_limit = if raw { 5. } else { 7. };
    if (!raw && front[0] > 0. && front[0] < 7.) || (rear[0] > 0. && rear[0] < rear_limit) {
        ret.set_left_blindspot(true);
    }
    if (!raw && front[1] > 0. && front[1] < 7.) || (rear[1] > 0. && rear[1] < rear_limit) {
        ret.set_right_blindspot(true);
    }
    Ok(())
}
