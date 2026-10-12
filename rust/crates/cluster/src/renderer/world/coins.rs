use super::super::tokens::{
    CORNER_RADAR_COIN_CUTIN_FILL, CORNER_RADAR_COIN_CUTIN_RING, CORNER_RADAR_COIN_CUTIN_SIDE,
    CORNER_RADAR_COIN_FILL, CORNER_RADAR_COIN_LEAD_ONE_FILL, CORNER_RADAR_COIN_LEAD_ONE_RING,
    CORNER_RADAR_COIN_LEAD_ONE_SIDE, CORNER_RADAR_COIN_LEAD_TWO_FILL,
    CORNER_RADAR_COIN_LEAD_TWO_RING, CORNER_RADAR_COIN_LEAD_TWO_SIDE, CORNER_RADAR_COIN_RING,
    CORNER_RADAR_COIN_SIDE, CORNER_RADAR_COIN_SLOWER_FILL, CORNER_RADAR_COIN_SLOWER_RING,
    CORNER_RADAR_COIN_SLOWER_SIDE, CORNER_RADAR_COIN_STOPPED_FILL,
    CORNER_RADAR_COIN_STOPPED_MAX_SPEED_KPH, CORNER_RADAR_COIN_STOPPED_RING,
    CORNER_RADAR_COIN_STOPPED_SIDE,
};
use crate::{config::RED, scene::VehicleBox};
#[must_use]
pub fn camera_overlay_vehicle_is_stopped(vehicle: &VehicleBox) -> bool {
    vehicle
        .absolute_speed_kph
        .is_some_and(|speed| speed.abs() <= CORNER_RADAR_COIN_STOPPED_MAX_SPEED_KPH)
}
#[must_use]
pub fn camera_overlay_vehicle_coin_colors(
    vehicle: &VehicleBox,
    lead_one: bool,
    lead_two: bool,
) -> [[u8; 3]; 3] {
    if vehicle.cut_in || vehicle.label.to_uppercase().contains("CUT-IN") {
        [
            CORNER_RADAR_COIN_CUTIN_FILL,
            CORNER_RADAR_COIN_CUTIN_SIDE,
            CORNER_RADAR_COIN_CUTIN_RING,
        ]
    } else if lead_one {
        [
            CORNER_RADAR_COIN_LEAD_ONE_FILL,
            CORNER_RADAR_COIN_LEAD_ONE_SIDE,
            CORNER_RADAR_COIN_LEAD_ONE_RING,
        ]
    } else if lead_two {
        [
            CORNER_RADAR_COIN_LEAD_TWO_FILL,
            CORNER_RADAR_COIN_LEAD_TWO_SIDE,
            CORNER_RADAR_COIN_LEAD_TWO_RING,
        ]
    } else if camera_overlay_vehicle_is_stopped(vehicle) {
        [
            CORNER_RADAR_COIN_STOPPED_FILL,
            CORNER_RADAR_COIN_STOPPED_SIDE,
            CORNER_RADAR_COIN_STOPPED_RING,
        ]
    } else if vehicle.absolute_speed_kph.is_some_and(|speed| speed < 0.) {
        [RED, [116, 28, 32], RED]
    } else if vehicle.relative_speed_mps.is_some_and(|speed| speed < 0.) {
        [
            CORNER_RADAR_COIN_SLOWER_FILL,
            CORNER_RADAR_COIN_SLOWER_SIDE,
            CORNER_RADAR_COIN_SLOWER_RING,
        ]
    } else {
        [
            CORNER_RADAR_COIN_FILL,
            CORNER_RADAR_COIN_SIDE,
            CORNER_RADAR_COIN_RING,
        ]
    }
}
