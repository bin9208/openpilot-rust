use super::{
    NaviCrossroadInfo, NaviGuidanceInfo, NaviLaneInfo, NaviRouteInfo, NaviSpeedInfo,
    NaviStatusInfo, NaviTrafficLightInfo, NaviVehicleInfo,
};
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct NaviLiveState {
    pub generation: u64,
    pub session_id: String,
    pub vehicle: Option<NaviVehicleInfo>,
    pub current: Option<NaviGuidanceInfo>,
    pub next: Option<NaviGuidanceInfo>,
    pub lane_current: Option<NaviLaneInfo>,
    pub lane_ahead: Vec<NaviLaneInfo>,
    pub speed: Option<NaviSpeedInfo>,
    pub traffic_light: Option<NaviTrafficLightInfo>,
    pub crossroad: Option<NaviCrossroadInfo>,
    pub route: Option<NaviRouteInfo>,
    pub status: Option<NaviStatusInfo>,
}
