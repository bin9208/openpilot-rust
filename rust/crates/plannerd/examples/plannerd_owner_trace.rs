mod plannerd_owner_trace_support;
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::car_capnp::car_params;
use openpilot_plannerd::{
    carrot::CarrotPlanner,
    lateral_planner::{self, LateralPlanner},
    longitudinal_planner::{self, LongitudinalPlanner},
};
use plannerd_owner_trace_support::{decode, snapshot, Store};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Request {
    artifact: PathBuf,
    car_params: PathBuf,
    parameters: BTreeMap<String, String>,
    frames: Vec<decode::Frame>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let request: Request = serde_json::from_str(&raw)?;
    let bytes = std::fs::read(&request.car_params)?;
    let message = serialize::read_message(bytes.as_slice(), ReaderOptions::new())?;
    let cp = message.get_root::<car_params::Reader<'_>>()?;
    let mut parameters = Store {
        values: request.parameters,
        operations: Vec::new(),
    };
    let vehicle = longitudinal_planner::Vehicle {
        longitudinal_control: cp.get_openpilot_longitudinal_control(),
        volkswagen_meb: cp.get_brand()?.to_str()? == "volkswagen" && cp.get_flags() & 16 != 0,
    };
    let mut longitudinal = LongitudinalPlanner::load(vehicle, &request.artifact, 0., 0., 0.05)?;
    let mut lateral = LateralPlanner::load(
        lateral_planner::Vehicle {
            wheelbase: f64::from(cp.get_wheelbase()),
            center_to_front: f64::from(cp.get_center_to_front()),
            mass: f64::from(cp.get_mass()),
            tire_stiffness_rear: f64::from(cp.get_tire_stiffness_rear()),
        },
        &request.artifact,
        &mut parameters,
    )?;
    let mut carrot = CarrotPlanner::new(&mut parameters)?;
    let mut outputs = Vec::new();
    for frame in request.frames {
        parameters.values.extend(frame.parameters.clone());
        let decoded = decode::Decoded::read(&frame)?;
        let mut mono = || frame.time;
        let mut wall = || frame.wall_time;
        let long_warning = longitudinal.update(
            &decoded.input(&frame),
            &mut carrot,
            &mut parameters,
            &mut wall,
            &mut mono,
        )?;
        let lat_warning = lateral.update(
            lateral_planner::Input {
                car: &decoded.car,
                model: &decoded.model,
                curvature: decoded.curvature,
                curve_speed: decoded.curve_speed,
                atc_active: carrot.atc_active,
            },
            &mut parameters,
            &mut mono,
        )?;
        outputs.push(snapshot::frame(
            &longitudinal,
            &lateral,
            &carrot,
            long_warning,
            lat_warning,
            &parameters.operations,
        ));
        parameters.operations.clear();
    }
    serde_json::to_writer(io::stdout().lock(), &outputs)?;
    Ok(())
}
