use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CarState {
    pub v_ego: f64,
    pub a_ego: f64,
    pub v_clu_ratio: f64,
    pub v_ego_cluster: f64,
    pub v_cruise: f64,
    pub cruise_speed: f64,
    pub log_carrot: String,
    pub speed_limit: f64,
    pub speed_limit_distance: f64,
    pub speed_bump_distance: f64,
    pub school_zone_active: bool,
    pub vehicle_navi_active: bool,
    pub vehicle_navi_speed: f64,
    pub vehicle_navi_section_active: bool,
    pub vehicle_navi_available: bool,
    pub gas_pressed: bool,
    pub brake_pressed: bool,
    pub steering_pressed: bool,
    pub steering_torque: f64,
    pub can_valid: bool,
    pub can_timeout: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GpsInput {
    pub car_updated: bool,
    pub control_updated: bool,
    pub gps_updated: bool,
    pub has_fix: bool,
    pub bearing_deg: f64,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GpsState {
    pub latitude: f64,
    pub longitude: f64,
    pub navi_latitude: f64,
    pub navi_longitude: f64,
    pub phone_latitude: f64,
    pub phone_longitude: f64,
    pub phone_accuracy: f64,
    pub phone_frame: u64,
    pub angle: f64,
    pub phone_angle: f64,
    pub speed: f64,
    pub bearing: f64,
    pub offset: f64,
    pub measured: f64,
    pub diff_angle_count: u64,
    pub last_calculate: f64,
    pub last_phone: f64,
    pub last_navi: f64,
    pub valid: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NavigationState {
    pub road_limit: f64,
    pub last_road_limit: f64,
    pub active_carrot: i64,
    pub active_count: i64,
    pub active_sdi_count: i64,
    pub active_kisa_count: i64,
    pub road_category: i64,
    pub sdi_type: i64,
    pub sdi_limit: f64,
    pub sdi_distance: f64,
    pub sdi_section: i64,
    pub block_type: i64,
    pub block_speed: f64,
    pub block_distance: f64,
    pub secondary_type: i64,
    pub secondary_limit: f64,
    pub secondary_distance: f64,
    pub secondary_block_type: i64,
    pub secondary_block_speed: f64,
    pub secondary_block_distance: f64,
    pub speed_type: i64,
    pub speed_limit: f64,
    pub speed_distance: f64,
    pub turn_type: i64,
    pub next_turn_type: i64,
    pub turn_distance: f64,
    pub next_turn_distance: f64,
    pub turn_info: i64,
    pub next_turn_info: i64,
    pub x_turn_distance: f64,
    pub x_next_turn_distance: f64,
    pub main_text: String,
    pub next_main_text: String,
    pub near_direction: String,
    pub far_direction: String,
    pub road_name: String,
    pub next_road_width: i64,
    pub goal_distance: f64,
    pub goal_time: f64,
    pub goal_latitude: f64,
    pub goal_longitude: f64,
}
impl Default for NavigationState {
    fn default() -> Self {
        Self {
            road_limit: 30.,
            last_road_limit: 30.,
            active_carrot: 0,
            active_count: 0,
            active_sdi_count: 0,
            active_kisa_count: 0,
            road_category: 8,
            sdi_type: -1,
            sdi_limit: 0.,
            sdi_distance: 0.,
            sdi_section: 0,
            block_type: -1,
            block_speed: 0.,
            block_distance: 0.,
            secondary_type: -1,
            secondary_limit: 0.,
            secondary_distance: 0.,
            secondary_block_type: -1,
            secondary_block_speed: 0.,
            secondary_block_distance: 0.,
            speed_type: -1,
            speed_limit: 0.,
            speed_distance: 0.,
            turn_type: -1,
            next_turn_type: -1,
            turn_distance: 0.,
            next_turn_distance: 0.,
            turn_info: -1,
            next_turn_info: -1,
            x_turn_distance: 0.,
            x_next_turn_distance: 0.,
            main_text: String::new(),
            next_main_text: String::new(),
            near_direction: String::new(),
            far_direction: String::new(),
            road_name: String::new(),
            next_road_width: 0,
            goal_distance: 0.,
            goal_time: 0.,
            goal_latitude: 0.,
            goal_longitude: 0.,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandState {
    pub index: i64,
    pub command_index: i64,
    pub last_command_index: i64,
    pub command: String,
    pub argument: String,
    pub command_text: bool,
    pub argument_text: bool,
    pub command_hashable: bool,
    pub handler_failed: bool,
}
impl Default for CommandState {
    fn default() -> Self {
        Self {
            index: 0,
            command_index: 0,
            last_command_index: 0,
            command: String::new(),
            argument: String::new(),
            command_text: true,
            argument_text: true,
            command_hashable: true,
            handler_failed: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SpeedState {
    pub total_distance: f64,
    pub rear_events: Vec<RearEvent>,
    pub external_active: bool,
    pub gas_override: f64,
    pub gas_pressed_state: bool,
    pub event_gas_pressed: bool,
    pub last_source: String,
    pub school_override_since: Option<f64>,
    pub school_suppressed: bool,
    pub speed_countdown_distance: f64,
    pub turn_countdown_distance: f64,
    pub left_speed_seconds: i64,
    pub left_turn_seconds: i64,
    pub left_seconds: i64,
    pub max_left_seconds: i64,
    pub carrot_left_seconds: i64,
    pub sdi_inform: bool,
    pub atc_paused: bool,
    pub atc_activate_count: i64,
}
impl Default for SpeedState {
    fn default() -> Self {
        Self {
            total_distance: 0.,
            rear_events: Vec::new(),
            external_active: false,
            gas_override: 0.,
            gas_pressed_state: false,
            event_gas_pressed: false,
            last_source: "none".into(),
            school_override_since: None,
            school_suppressed: false,
            speed_countdown_distance: 0.,
            turn_countdown_distance: 0.,
            left_speed_seconds: 100,
            left_turn_seconds: 100,
            left_seconds: 100,
            max_left_seconds: 100,
            carrot_left_seconds: 100,
            sdi_inform: false,
            atc_paused: false,
            atc_activate_count: 0,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct RearEvent {
    pub target: f64,
    pub speed: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Detection {
    pub x: f64,
    pub y: f64,
    pub color: String,
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TurnResult {
    pub desired: f64,
    pub kind: String,
    pub speed: f64,
    pub distance: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TickInput {
    pub now: f64,
    pub car: Option<CarState>,
    pub selfdrive_alive: bool,
    pub distance_traveled: f64,
    pub vision_speed: f64,
    pub route_speed: f64,
    pub gps: GpsInput,
    #[serde(default)]
    pub nav_instruction: Option<InstructionInput>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct InstructionInput {
    pub distance_remaining: f64,
    pub time_remaining: f64,
    pub speed_limit: f64,
    pub maneuver_distance: f64,
    pub primary_text: String,
    pub kind: String,
    pub modifier: String,
}
