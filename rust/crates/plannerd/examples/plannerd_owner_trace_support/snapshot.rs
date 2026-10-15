use openpilot_plannerd::{
    carrot::CarrotPlanner, lateral_planner::LateralPlanner,
    longitudinal_planner::LongitudinalPlanner,
};
use serde_json::{json, Value};

pub fn frame(
    long: &LongitudinalPlanner,
    lat: &LateralPlanner,
    carrot: &CarrotPlanner,
    long_warning: Option<i32>,
    lat_warning: bool,
    parameters: &[[String; 2]],
) -> Value {
    let mpc = &long.mpc;
    json!({
        "parameters":parameters, "long_warning":long_warning, "lat_warning":lat_warning,
        "long":{"fcw":long.fcw,"a_target":long.target_acceleration,"a_base":long.base_acceleration,
            "v_target":long.target_speed,"j_target":long.target_jerk,"should_stop":long.should_stop,
            "preview":long.lead_preview,"preview_time":long.preview_action_time,"preview_accel":long.preview_acceleration,
            "tracks":long.track_frames,"speed":long.speed_trajectory,"accel":long.accel_trajectory,"jerk":long.jerk_trajectory,
            "cruise":long.cruise_kph,"coast":long.coasting_target,"coast_percent":long.coasting_percent,
            "desired_accel":long.desired_acceleration,"cluster_ratio":long.cluster_ratio},
        "mpc":{"mode":mpc.mode,"source":mpc.source,"x":mpc.x,"u":mpc.u,"p":mpc.parameters,"yref":mpc.reference,
            "initial":mpc.initial,"prev_a":mpc.previous_acceleration,"tf":mpc.following_time,"distance":mpc.desired_distance,
            "base_distances":mpc.base_desired_distances,"danger_factor":mpc.danger_factor,"danger_margin":mpc.predicted_danger_margin,
            "change_cost":mpc.change_cost,"jerk_factor":mpc.jerk_cost_factor,"response_active":mpc.response_active,"response_level":mpc.response_level,
            "gap_margins":mpc.gap_margins,"status":mpc.status,"crash_count":mpc.crash_count,"solution_status":mpc.solution_status},
        "carrot":{"frame":carrot.frame,"mode":carrot.mode,"driving_mode":i32::from(carrot.driving_mode),"safe":carrot.safe_factor,
            "tf_factor":carrot.follow_factor,"lead_response":carrot.lead_response,"traffic":carrot.traffic_state,"state":carrot.x_state,
            "fake_cruise_distance":carrot.fake_cruise_distance,"x_stop":carrot.x_stop,"actual_stop_distance":carrot.actual_stop_distance,
            "user_stop_distance":carrot.user_stop_distance,"brake":carrot.comfort_brake,"soft_hold":carrot.soft_hold_active,
            "lane_active":carrot.lane_change_active,"desire":carrot.desire_state,"desire_count":carrot.desire_state_count,
            "active":carrot.active_carrot,"turn_distance":carrot.distance_to_turn,"atc_type":carrot.atc_type,"atc_active":carrot.atc_active,
            "eco_target":carrot.eco_target_speed,"stop_offset":carrot.traffic_stop_model_lead_offset,
            "events":carrot.events.iter().map(|event| u16::from(*event)).collect::<Vec<_>>(),
            "cruise":carrot.cruise_speed,"stop":carrot.stop_distance,"tf":carrot.following.value,
            "jerk_factor":carrot.following.jerk_factor,"decel_extra":carrot.following.decel_extra},
        "lat":{"output":lat.output(),"x":lat.mpc.x.to_vec(),"u":lat.mpc.u.to_vec(),"initial":lat.initial,
            "status":lat.mpc.status,"cost":lat.mpc.cost,"invalid":lat.invalid_count,
            "path":lat.path.to_vec(),"speeds":lat.planned_speed.to_vec(),"yaw":lat.yaw.to_vec(),"yaw_rate":lat.yaw_rate.to_vec()}
    })
}
