use super::{read, Error};
use crate::{batch::Ego, decoder::Config};
use openpilot_can::{Frame, Packet};
use openpilot_cereal::{car_capnp::car_params, log_capnp::event};

pub fn config(bytes: &[u8]) -> Result<Config, Error> {
    let message = read(bytes)?;
    let cp = message.get_root::<car_params::Reader>()?;
    Ok(Config {
        candidate: cp.get_car_fingerprint()?.to_str()?.to_owned(),
        delay: cp.get_radar_delay(),
        period: cp.get_radar_time_step(),
        unavailable: cp.get_radar_unavailable(),
        flags: cp.get_flags(),
        ext_flags: cp.get_ext_flags(),
        safety_count: usize::try_from(cp.get_safety_configs()?.len())?,
    })
}

pub fn can(bytes: &[u8]) -> Result<Packet, Error> {
    let message = read(bytes)?;
    let root = message.get_root::<event::Reader>()?;
    let event::Which::Can(frames) = root.which()? else {
        return Err(Error::Event("can"));
    };
    let frames = frames?
        .iter()
        .map(|frame| {
            Ok(Frame {
                address: frame.get_address(),
                data: frame.get_dat()?.to_vec(),
                bus: frame.get_src(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Packet {
        mono_time: root.get_log_mono_time(),
        frames,
    })
}

pub fn ego(bytes: &[u8]) -> Result<Ego, Error> {
    let message = read(bytes)?;
    let root = message.get_root::<event::Reader>()?;
    let event::Which::CarState(state) = root.which()? else {
        return Err(Error::Event("carState"));
    };
    let state = state?;
    let input = state.get_radar_input()?;
    Ok(Ego {
        first_can_ns: input.get_first_can_mono_time(),
        last_can_ns: input.get_last_can_mono_time(),
        packet_count: input.get_can_packet_count(),
        receive_ns: input.get_receive_mono_time(),
        v_ego: f64::from(state.get_v_ego()),
        a_ego: f64::from(state.get_a_ego()),
    })
}
