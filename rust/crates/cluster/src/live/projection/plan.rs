use super::{Data, Projection};
use crate::{config::MAX_ACCEL_MPS2, models::ClusterAlert};
use num_traits::ToPrimitive;

impl Projection {
    pub(super) fn longitudinal(&mut self, plan: Data<'_>, valid: bool) {
        if let Some(source) = plan
            .field("longitudinalPlanSource")
            .text()
            .filter(|source| !source.is_empty())
        {
            self.state.longitudinal_plan_source = Some(source);
        }
        self.state.longitudinal_plan_speeds_kph = plan
            .field("speeds")
            .numbers(usize::MAX, 0.0, 90.0)
            .into_iter()
            .map(|speed| speed * 3.6)
            .collect();
        self.state.longitudinal_plan_accels_mps2 =
            plan.field("accels")
                .numbers(usize::MAX, -MAX_ACCEL_MPS2, MAX_ACCEL_MPS2);
        self.state.longitudinal_plan_jerks_mps3 =
            plan.field("jerks").numbers(usize::MAX, -12.0, 12.0);
        self.state.longitudinal_plan_fcw = plan.flag("fcw", self.state.longitudinal_plan_fcw);
        self.state.longitudinal_plan_should_stop =
            plan.flag("shouldStop", self.state.longitudinal_plan_should_stop);
        self.state.longitudinal_plan_allow_throttle = Some(plan.flag(
            "allowThrottle",
            self.state.longitudinal_plan_allow_throttle.unwrap_or(false),
        ));
        self.state.longitudinal_plan_allow_brake = Some(plan.flag(
            "allowBrake",
            self.state.longitudinal_plan_allow_brake.unwrap_or(false),
        ));
        if let Some(traffic) = plan
            .field("trafficState")
            .int()
            .filter(|traffic| (0..=2).contains(traffic))
        {
            self.state.traffic_state = traffic;
        }
        self.state.driving_mode = plan
            .field("myDrivingMode")
            .int()
            .filter(|mode| valid && (1..=4).contains(mode));
        if let Some(value) = plan
            .field("tFollow")
            .float()
            .filter(|value| (0.0..=5.0).contains(value))
        {
            self.state.longitudinal_t_follow_s = Some(value);
        }
        if let Some(value) = plan
            .field("desiredDistance")
            .float()
            .filter(|value| (0.0..=250.0).contains(value))
        {
            self.state.longitudinal_desired_distance_m = Some(value);
        }
        if let Some(value) = plan
            .field("vTargetNow")
            .float()
            .filter(|value| (0.0..=90.0).contains(value))
        {
            self.state.longitudinal_v_target_kph = Some(value * 3.6);
        }
        if let Some(value) = plan
            .field("jTargetNow")
            .float()
            .filter(|value| value.abs() <= 12.0)
        {
            self.state.longitudinal_jerk_target_mps3 = Some(value);
        }
    }
    pub(super) fn lateral(&mut self, plan: Data<'_>) {
        if let Some(width) = plan.field("laneWidth").float().filter(|width| *width > 0.0) {
            self.state.lane_width_m = width.clamp(2.4, 4.6);
        }
        if let Some(curvature) = plan
            .field("curvatures")
            .index(0)
            .float()
            .filter(|value| value.abs() < 0.05)
        {
            self.model_curvature = Some(curvature);
        }
        self.state.lateral_plan_valid = Some(plan.flag(
            "mpcSolutionValid",
            self.state.lateral_plan_valid.unwrap_or(false),
        ));
        self.state.lateral_plan_use_lane_lines = Some(plan.flag(
            "useLaneLines",
            self.state.lateral_plan_use_lane_lines.unwrap_or(false),
        ));
        if let Some(cost) = plan
            .field("solverCost")
            .float()
            .filter(|value| *value >= 0.0)
        {
            self.state.lateral_plan_solver_cost = Some(cost.min(1_000_000.0));
        }
        if let Some(text) = plan
            .field("latDebugText")
            .text()
            .filter(|text| !text.is_empty())
        {
            self.state.lateral_plan_debug_text = Some(text.chars().take(64).collect());
        }
        self.state.lateral_plan_curvatures =
            plan.field("curvatures").numbers(usize::MAX, -0.08, 0.08);
        self.state.lateral_plan_curvature_rates =
            plan.field("curvatureRates")
                .numbers(usize::MAX, -0.08, 0.08);
    }
    pub(super) fn controls(&mut self, controls: Data<'_>) {
        if let Some(enabled) = controls.field("enabled").boolean() {
            self.controls_enabled = Some(enabled);
        }
        if let Some(active) = controls.field("activeLaneLine").boolean() {
            self.state.active_lane_line = Some(active);
        }
        for field in ["desiredCurvature", "curvature"] {
            if let Some(curvature) = controls
                .field(field)
                .float()
                .filter(|value| value.abs() < 0.05)
            {
                self.controls_curvature = Some(curvature);
                break;
            }
        }
    }
    pub(super) fn selfdrive(&mut self, drive: Data<'_>) {
        if let Some(enabled) = drive.field("enabled").boolean() {
            self.selfdrive_enabled = Some(enabled);
        }
        let size = drive.field("alertSize").int().unwrap_or(0);
        self.state.alert = (size > 0).then(|| ClusterAlert {
            text1: drive.field("alertText1").text().unwrap_or_default(),
            text2: drive.field("alertText2").text().unwrap_or_default(),
            size,
            status: drive.field("alertStatus").int().unwrap_or(0),
            alert_type: drive.field("alertType").text().unwrap_or_default(),
        });
        if let Some(gap) = drive
            .field("personality")
            .int()
            .filter(|value| (0..=3).contains(value))
        {
            self.state.cruise_gap = Some(gap + 1);
        }
    }
    pub(super) fn navigation(&mut self, instruction: Data<'_>, time: f64) {
        let limit = instruction.number("speedLimit", 0.0);
        if limit <= 0.1 {
            if time - self.nav_time > 10.0 {
                self.nav_limit = None;
                self.nav_time = -999.0;
            }
            return;
        }
        let rounded = limit.round_ties_even();
        let integer_like = (limit - rounded).abs() < 0.05;
        let kph_like = limit >= 45.0 || (limit >= 30.0 && integer_like && rounded % 5.0 == 0.0);
        self.nav_limit = (if kph_like {
            rounded
        } else {
            (limit * 3.6).round_ties_even()
        })
        .to_i64();
        self.nav_time = time;
    }
}
