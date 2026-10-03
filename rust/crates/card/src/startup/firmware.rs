use super::Error;
use crate::firmware::{Ecu, Firmware};
use openpilot_cereal::car_capnp::car_params::{self, car_fw};

fn ecu(ecu: Ecu) -> car_params::Ecu {
    match ecu {
        Ecu::Eps => car_params::Ecu::Eps,
        Ecu::Abs => car_params::Ecu::Abs,
        Ecu::FwdRadar => car_params::Ecu::FwdRadar,
        Ecu::FwdCamera => car_params::Ecu::FwdCamera,
        Ecu::Engine => car_params::Ecu::Engine,
        Ecu::Unknown => car_params::Ecu::Unknown,
        Ecu::Dsu => car_params::Ecu::Dsu,
        Ecu::ParkingAdas => car_params::Ecu::ParkingAdas,
        Ecu::Transmission => car_params::Ecu::Transmission,
        Ecu::Srs => car_params::Ecu::Srs,
        Ecu::Gateway => car_params::Ecu::Gateway,
        Ecu::Hud => car_params::Ecu::Hud,
        Ecu::CombinationMeter => car_params::Ecu::CombinationMeter,
        Ecu::Vsa => car_params::Ecu::Vsa,
        Ecu::ProgrammedFuelInjection => car_params::Ecu::ProgrammedFuelInjection,
        Ecu::ElectricBrakeBooster => car_params::Ecu::ElectricBrakeBooster,
        Ecu::ShiftByWire => car_params::Ecu::ShiftByWire,
        Ecu::Debug => car_params::Ecu::Debug,
        Ecu::Hybrid => car_params::Ecu::Hybrid,
        Ecu::Adas => car_params::Ecu::Adas,
        Ecu::Hvac => car_params::Ecu::Hvac,
        Ecu::CornerRadar => car_params::Ecu::CornerRadar,
        Ecu::Epb => car_params::Ecu::Epb,
        Ecu::Telematics => car_params::Ecu::Telematics,
        Ecu::Body => car_params::Ecu::Body,
    }
}

pub(super) fn write(
    mut output: capnp::struct_list::Builder<'_, car_fw::Owned>,
    firmware: &[Firmware],
) -> Result<(), Error> {
    for (index, source) in firmware.iter().enumerate() {
        let mut fw = output.reborrow().get(u32::try_from(index)?);
        fw.set_ecu(ecu(source.ecu));
        fw.set_address(source.address);
        fw.set_sub_address(source.sub_address);
        fw.set_fw_version(&source.fw_version);
        fw.set_response_address(source.response_address);
        fw.set_brand(&source.brand);
        fw.set_bus(source.bus);
        fw.set_logging(source.logging);
        fw.set_obd_multiplexing(source.obd_multiplexing);
        let mut requests = fw.init_request(u32::try_from(source.request.len())?);
        for (index, bytes) in source.request.iter().enumerate() {
            requests.set(u32::try_from(index)?, bytes);
        }
    }
    Ok(())
}

pub(super) fn read(
    firmware: capnp::struct_list::Reader<'_, car_fw::Owned>,
) -> Result<Vec<Firmware>, Error> {
    let mut output = Vec::new();
    for fw in firmware {
        let ecu = fw.get_ecu()?;
        let candidate = [
            Ecu::Eps,
            Ecu::Abs,
            Ecu::FwdRadar,
            Ecu::FwdCamera,
            Ecu::Engine,
            Ecu::Unknown,
            Ecu::Dsu,
            Ecu::ParkingAdas,
            Ecu::Transmission,
            Ecu::Srs,
            Ecu::Gateway,
            Ecu::Hud,
            Ecu::CombinationMeter,
            Ecu::Vsa,
            Ecu::ProgrammedFuelInjection,
            Ecu::ElectricBrakeBooster,
            Ecu::ShiftByWire,
            Ecu::Debug,
            Ecu::Hybrid,
            Ecu::Adas,
            Ecu::Hvac,
            Ecu::CornerRadar,
            Ecu::Epb,
            Ecu::Telematics,
            Ecu::Body,
        ]
        .into_iter()
        .find(|&value| self::ecu(value) == ecu)
        .ok_or(capnp::NotInSchema(ecu as u16))?;
        output.push(Firmware {
            ecu: candidate,
            address: fw.get_address(),
            sub_address: fw.get_sub_address(),
            fw_version: fw.get_fw_version()?.to_vec(),
            response_address: fw.get_response_address(),
            request: fw
                .get_request()?
                .iter()
                .map(|bytes| bytes.map(<[u8]>::to_vec))
                .collect::<Result<_, _>>()?,
            brand: fw.get_brand()?.to_str()?.to_owned(),
            bus: fw.get_bus(),
            logging: fw.get_logging(),
            obd_multiplexing: fw.get_obd_multiplexing(),
        });
    }
    Ok(output)
}
