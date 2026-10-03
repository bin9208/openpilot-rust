use super::{flags as f, parameters::setting_int, state::State, Error};
use crate::{
    ecu::{self, DisableConfig, EcuAddress},
    firmware_query::StartupIo,
    query::{ParallelQuery, QueryConfig, Target},
};

pub fn init(state: &mut State, io: &mut impl StartupIo) -> Result<(), Error> {
    state.settings.put("LongitudinalPersonalityMax", b"4")?;
    let config = &state.config;
    if config.longitudinal && config.flags & f::CAMERA_SCC == 0 {
        let target = if config.flags & f::HDA2 != 0 {
            EcuAddress(0x730, None, config.bus.ecan)
        } else {
            EcuAddress(0x7d0, None, 0)
        };
        ecu::disable(
            DisableConfig {
                target,
                communication_request: &[0x28, 0x83, 1],
                timeout: 0.1,
                retry: 10,
            },
            io,
        );
    }
    if setting_int(&state.settings, "EnableRadarTracks")? > 0 && config.flags & f::CANFD == 0 {
        let result = enable_tracks(
            if config.flags & f::CAMERA_SCC != 0 {
                2
            } else {
                0
            },
            io,
            &mut state.inputs.diagnostics.prints,
        );
        state.settings.put_bool("EnableRadarTracksResult", result)?;
    }
    if config.flags & f::ENABLE_BLINKERS != 0 {
        ecu::disable(
            DisableConfig {
                target: EcuAddress(0x7b1, None, config.bus.ecan),
                communication_request: &[0x28, 0x83, 1],
                timeout: 0.1,
                retry: 10,
            },
            io,
        );
    }
    Ok(())
}

fn enable_tracks(bus: u8, io: &mut impl StartupIo, prints: &mut Vec<String>) -> bool {
    prints.push("################ Try To Enable Radar Tracks ####################".into());
    let result = tracks_query(bus, io, prints);
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            prints.push(format!("Failed : {error}"));
            false
        }
    };
    prints.push("################ END Try to enable radar tracks".into());
    result
}

fn tracks_query(
    bus: u8,
    io: &mut impl StartupIo,
    prints: &mut Vec<String>,
) -> Result<bool, crate::query::Error> {
    let targets = [Target(0x7d0, None)];
    let mut query = ParallelQuery::new(QueryConfig {
        bus,
        targets: &targets,
        request: &[vec![0x10, 7]],
        response: &[vec![0x50, 7]],
        response_offset: 8,
        functional_addrs: &[],
        response_pending_timeout: 10.,
    })?;
    if query.get_data(0.1, 60., io)?.is_empty() {
        return Ok(false);
    }
    prints.push("ecu write data by id ...".into());
    let mut write = ParallelQuery::new(QueryConfig {
        bus,
        targets: &targets,
        request: &[vec![0x2e, 1, 0x42, 0, 0, 0, 1, 0, 1]],
        response: &[vec![0x68]],
        response_offset: 8,
        functional_addrs: &[],
        response_pending_timeout: 10.,
    })?;
    let result = write.get_data(0., 60., io)?;
    prints.push(format!(
        "result= {{{}}}",
        result
            .iter()
            .map(|(target, data)| format!(
                "({}, None): {}",
                target.0,
                crate::vehicle_params::bytes_repr(data)
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    Ok(true)
}
