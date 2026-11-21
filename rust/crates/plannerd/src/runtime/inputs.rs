use super::views::Views;
use crate::{
    car_state::CarState,
    car_state_decode,
    carrot::{self, Navigation},
    longitudinal_planner,
    model::Model,
    model_decode,
    radar::Radar,
    types::Personality,
    Error,
};

pub struct Decoded {
    pub car: CarState,
    pub model: Model,
    navigation: Option<Navigation>,
    pub curve_speed: f64,
}

impl Decoded {
    pub fn read(views: &Views<'_>) -> Result<Self, Error> {
        let navigation = views.navigation()?;
        let topic = views.0.topic("carrotMan")?;
        Ok(Self {
            car: car_state_decode::state(views.car()?),
            model: model_decode::model(views.model()?)?,
            curve_speed: f64::from(navigation.get_v_turn_speed()),
            navigation: if topic.seen && topic.valid && topic.alive {
                Some(Navigation {
                    atc_type: navigation.get_atc_type()?.to_str()?.into(),
                    traffic_state: navigation.get_traffic_state(),
                    active_carrot: navigation.get_active_carrot(),
                    x_dist_to_turn: f64::from(navigation.get_x_dist_to_turn()),
                    desired_speed: f64::from(navigation.get_desired_speed()),
                })
            } else {
                None
            },
        })
    }

    pub fn longitudinal<'a>(
        &'a self,
        views: &Views<'_>,
        radar: &'a Radar,
    ) -> Result<longitudinal_planner::Input<'a>, Error> {
        let control = views.controls()?;
        let selfdrive = views.selfdrive()?;
        let pose = views.pose()?;
        let angular = pose.get_angular_velocity_device()?;
        Ok(longitudinal_planner::Input {
            carrot: carrot::Input {
                car: &self.car,
                model: &self.model,
                radar,
                personality: Personality::try_from(i32::from(u16::from(
                    selfdrive.get_personality()?,
                )))?,
                navigation: self.navigation.as_ref(),
                navigation_receive_time: Some(views.0.topic("carrotMan")?.receive_time),
                mode_checks: views.0.all_checks(&["carState", "radarState"])?,
                lane_valid: views.available(&["carState", "modelV2", "radarState"])?,
                model_ns: views.0.topic("modelV2")?.log_mono_time,
                radar_ns: views.0.topic("radarState")?.log_mono_time,
                pose_ns: views.0.topic("livePose")?.log_mono_time,
                pose_valid: views.available(&["livePose"])?
                    && pose.get_inputs_o_k()
                    && pose.get_sensors_o_k()
                    && angular.get_valid(),
                yaw_rate: f64::from(angular.get_z()),
            },
            enabled: selfdrive.get_enabled(),
            experimental: selfdrive.get_experimental_mode(),
            control_state: control.get_long_control_state()?,
            force_deceleration: control.get_force_decel(),
            desired_curvature: f64::from(control.get_desired_curvature()),
            curvature: f64::from(control.get_curvature()),
            coasting_checks: views.0.all_checks(&[
                "carState",
                "controlsState",
                "selfdriveState",
                "radarState",
                "modelV2",
            ])?,
            navigation_seen: views.0.topic("carrotMan")?.seen,
            tesla: tesla_car(views.car()?)?,
            long_active_checks: views.control()?.get_long_active()
                && views.0.all_checks(&[
                    "carState",
                    "carControl",
                    "selfdriveState",
                    "controlsState",
                    "modelV2",
                ])?,
            boot_ns: boot_ns(),
        })
    }
}

/// carrot `CLOCK_BOOTTIME` ns, the domain of CAN sample times.
fn boot_ns() -> u64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    u64::try_from(now.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::try_from(now.tv_nsec).unwrap_or(0))
}

/// The carState fields the Tesla stop-line constraint reads.
fn tesla_car(
    cs: openpilot_cereal::car_capnp::car_state::Reader<'_>,
) -> Result<crate::tesla_traffic_stop::Car, Error> {
    use openpilot_cereal::car_capnp::car_state::{tesla_traffic_control::Light, GearShifter};
    let traffic = cs.get_tesla_traffic_control()?;
    Ok(crate::tesla_traffic_stop::Car {
        v_ego: f64::from(cs.get_v_ego()),
        gas_pressed: cs.get_gas_pressed(),
        brake_pressed: cs.get_brake_pressed(),
        drive: cs.get_gear_shifter() == Ok(GearShifter::Drive),
        can_valid: cs.get_can_valid(),
        sensors_invalid: cs.get_vehicle_sensors_invalid(),
        door_open: cs.get_door_open(),
        seatbelt_unlatched: cs.get_seatbelt_unlatched(),
        parking_brake: cs.get_parking_brake(),
        acc_faulted: cs.get_acc_faulted(),
        esp_disabled: cs.get_esp_disabled(),
        valid: traffic.get_valid(),
        sample: traffic.get_sample_mono_time(),
        stop_line_distance: f64::from(traffic.get_stop_line_distance()),
        light: match traffic.get_light() {
            Ok(Light::Red) => 1,
            Ok(Light::Green) => 2,
            Ok(Light::Yellow) => 3,
            _ => 0,
        },
        status: traffic.get_status()?.to_str()?.to_owned(),
    })
}
