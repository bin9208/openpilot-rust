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
        })
    }
}
