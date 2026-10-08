use crate::{
    curve::ModelPath,
    navigation::v2,
    serv::{CarState, GpsInput, InstructionInput},
    Error,
};
use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{event, nav_instruction},
};
use openpilot_messaging::state::State;
use openpilot_navd::geometry::Coordinate;

#[path = "bus_v2.rs"]
mod navi;

pub struct Inputs {
    pub car: Option<CarState>,
    pub selfdrive_alive: bool,
    pub distance_traveled: f64,
    pub selfdrive_active: bool,
    pub model: ModelPath,
    pub model_usable: bool,
    pub model_stamp: u64,
    pub gps: GpsInput,
    pub v2: Option<v2::Payload>,
    pub v2_alive: bool,
    pub navd_route: Option<Vec<Coordinate>>,
    pub instruction: Option<InstructionInput>,
    pub x_state: i64,
    pub traffic_state: i64,
    pub network_connected: bool,
}

macro_rules! topic {
    ($state:expr,$name:expr,$variant:ident) => {{
        let event::$variant(value) = $state.topic($name)?.event()?.which()? else {
            return Err(Error::Contract("cereal service union"));
        };
        value?
    }};
}

fn text(value: capnp::text::Reader<'_>) -> Result<String, Error> {
    value
        .to_str()
        .map(str::to_owned)
        .map_err(|_| Error::Contract("invalid cereal UTF-8"))
}
pub fn car(reader: car_state::Reader<'_>) -> Result<CarState, Error> {
    Ok(CarState {
        v_ego: f64::from(reader.get_v_ego()),
        a_ego: f64::from(reader.get_a_ego()),
        v_clu_ratio: f64::from(reader.get_v_clu_ratio()),
        v_ego_cluster: f64::from(reader.get_v_ego_cluster()),
        v_cruise: f64::from(reader.get_v_cruise()),
        cruise_speed: f64::from(reader.get_cruise_state()?.get_speed()),
        log_carrot: text(reader.get_log_carrot()?)?,
        speed_limit: f64::from(reader.get_speed_limit()),
        speed_limit_distance: f64::from(reader.get_speed_limit_distance()),
        speed_bump_distance: f64::from(reader.get_speed_bump_distance()),
        school_zone_active: reader.get_school_zone_active(),
        vehicle_navi_active: reader.get_vehicle_navi_active(),
        vehicle_navi_speed: f64::from(reader.get_vehicle_navi_speed()),
        vehicle_navi_section_active: reader.get_vehicle_navi_section_active(),
        vehicle_navi_available: reader.get_vehicle_navi_available(),
        gas_pressed: reader.get_gas_pressed(),
        brake_pressed: reader.get_brake_pressed(),
        steering_pressed: reader.get_steering_pressed(),
        steering_torque: f64::from(reader.get_steering_torque()),
        can_valid: reader.get_can_valid(),
        can_timeout: reader.get_can_timeout(),
    })
}

pub fn instruction(reader: nav_instruction::Reader<'_>) -> Result<InstructionInput, Error> {
    Ok(InstructionInput {
        distance_remaining: f64::from(reader.get_distance_remaining()),
        time_remaining: f64::from(reader.get_time_remaining()),
        speed_limit: f64::from(reader.get_speed_limit()),
        maneuver_distance: f64::from(reader.get_maneuver_distance()),
        primary_text: text(reader.get_maneuver_primary_text()?)?,
        kind: text(reader.get_maneuver_type()?)?,
        modifier: text(reader.get_maneuver_modifier()?)?,
    })
}
pub fn fallback(state: &State) -> Result<Option<nav_instruction::Reader<'_>>, Error> {
    let topic = state.topic("navInstruction")?;
    if !topic.alive || !topic.valid {
        return Ok(None);
    }
    Ok(Some(topic!(state, "navInstruction", NavInstruction)))
}

pub fn read(state: &State, gps_service: &str) -> Result<Inputs, Error> {
    let car_topic = state.topic("carState")?;
    let car = if car_topic.alive {
        Some(car(topic!(state, "carState", CarState))?)
    } else {
        None
    };
    let selfdrive = topic!(state, "selfdriveState", SelfdriveState);
    let model = topic!(state, "modelV2", ModelV2);
    let xyz = model.get_position()?;
    let model_path = ModelPath {
        x: xyz.get_x()?.iter().map(f64::from).collect(),
        y: xyz.get_y()?.iter().map(f64::from).collect(),
        z: xyz.get_z()?.iter().map(f64::from).collect(),
        velocity: model
            .get_velocity()?
            .get_x()?
            .iter()
            .map(f64::from)
            .collect(),
        yaw_rate: model
            .get_orientation_rate()?
            .get_z()?
            .iter()
            .map(f64::from)
            .collect(),
    };
    let gps = if gps_service == "gpsLocationExternal" {
        topic!(state, gps_service, GpsLocationExternal)
    } else {
        topic!(state, gps_service, GpsLocation)
    };
    let gps = GpsInput {
        car_updated: car_topic.updated,
        control_updated: state.topic("carControl")?.updated,
        gps_updated: state.topic(gps_service)?.updated,
        has_fix: gps.get_has_fix(),
        bearing_deg: f64::from(gps.get_bearing_deg()),
        latitude: gps.get_latitude(),
        longitude: gps.get_longitude(),
    };
    let navi_topic = state.topic("carrotNavi")?;
    let v2_alive = navi_topic.alive && navi_topic.valid;
    let v2 = if v2_alive && navi_topic.updated {
        Some(navi::read(topic!(state, "carrotNavi", CarrotNavi))?)
    } else {
        None
    };
    let navd_route = if state.topic("navRouteNavd")?.updated {
        Some(
            topic!(state, "navRouteNavd", NavRouteNavd)
                .get_coordinates()?
                .iter()
                .map(|p| Coordinate::new(f64::from(p.get_latitude()), f64::from(p.get_longitude())))
                .collect(),
        )
    } else {
        None
    };
    let instruction = fallback(state)?.map(instruction).transpose()?;
    let plan_topic = state.topic("longitudinalPlan")?;
    let plan = topic!(state, "longitudinalPlan", LongitudinalPlan);
    let device = topic!(state, "deviceState", DeviceState);
    Ok(Inputs {
        car,
        selfdrive_alive: state.topic("selfdriveState")?.alive,
        distance_traveled: f64::from(selfdrive.get_distance_traveled()),
        selfdrive_active: state.topic("selfdriveState")?.alive && selfdrive.get_active(),
        model: model_path,
        model_usable: car_topic.alive
            && car_topic.valid
            && state.topic("modelV2")?.alive
            && state.topic("modelV2")?.valid,
        model_stamp: state.topic("modelV2")?.log_mono_time,
        gps,
        v2,
        v2_alive,
        navd_route,
        instruction,
        x_state: if plan_topic.alive {
            i64::from(plan.get_x_state())
        } else {
            0
        },
        traffic_state: if plan_topic.alive {
            i64::from(plan.get_traffic_state())
        } else {
            0
        },
        network_connected: device.get_network_type()?
            != openpilot_cereal::log_capnp::device_state::NetworkType::None,
    })
}
