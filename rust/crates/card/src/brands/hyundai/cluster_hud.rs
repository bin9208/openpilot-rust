use super::{
    cluster_fields,
    wire::{set, Values},
    Error,
};

pub struct HudInput {
    pub frame: u32,
    pub enabled: bool,
    pub active: bool,
    pub main_enabled: bool,
    pub lat_enabled: bool,
    pub nav_active: bool,
    pub navi_available: bool,
    pub hdp_use: i32,
    pub set_speed: f64,
    pub gap: f64,
    pub lead_visible: bool,
    pub lead_distance: f64,
    pub paddle: bool,
    pub paddle_mode: i32,
    pub hda_mode: f64,
    pub steering_angle: f64,
    pub soft_hold: u8,
    pub trailer: bool,
    pub lane_check: i32,
    pub lane_lines: [i16; 2],
    pub lane_visible: [bool; 2],
    pub lane_depart: [bool; 2],
    pub change_available: [bool; 2],
    pub blindspot: [bool; 2],
    pub blinker: [bool; 2],
    pub desire: u16,
}

pub fn hud(data: &mut Values, i: &HudInput) -> Result<(), Error> {
    let hdp = match i.hdp_use {
        1 => i.enabled && i.nav_active,
        2 => i.enabled,
        _ => false,
    };
    set(
        data,
        &[
            (
                "SETSPEED",
                if i.main_enabled {
                    if hdp {
                        6.
                    } else if i.enabled {
                        3.
                    } else {
                        1.
                    }
                } else {
                    0.
                },
            ),
            (
                "SETSPEED_HUD",
                if i.main_enabled {
                    if hdp {
                        5.
                    } else if i.enabled {
                        3.
                    } else {
                        1.
                    }
                } else {
                    0.
                },
            ),
            ("vSetDis", (i.set_speed + 0.5).trunc()),
            ("DISTANCE", if hdp { 4. } else { i.gap }),
            (
                "DISTANCE_LEAD",
                if i.lead_visible {
                    if i.enabled {
                        2.
                    } else if i.main_enabled {
                        1.
                    } else {
                        0.
                    }
                } else {
                    0.
                },
            ),
            (
                "DISTANCE_CAR",
                if hdp {
                    3.
                } else if i.enabled {
                    2.
                } else if i.main_enabled {
                    1.
                } else {
                    0.
                },
            ),
            (
                "DISTANCE_SPACING",
                if hdp {
                    5.
                } else if i.enabled {
                    1.
                } else {
                    0.
                },
            ),
            ("TARGET", if i.lead_visible && i.enabled { 1. } else { 0. }),
            ("TARGET_DISTANCE", i.lead_distance),
            (
                "BACKGROUND",
                if i.paddle_mode > 0 && i.paddle {
                    6.
                } else if i.enabled {
                    1.
                } else if i.active {
                    3.
                } else {
                    7.
                },
            ),
            ("CENTERLINE", if i.hda_mode > 0. { 1. } else { 0. }),
            (
                "CAR_CIRCLE",
                if hdp {
                    2.
                } else if i.enabled {
                    1.
                } else {
                    0.
                },
            ),
            (
                "NAV_ICON",
                if i.nav_active || i.navi_available {
                    if i.enabled {
                        2.
                    } else if i.main_enabled {
                        1.
                    } else {
                        0.
                    }
                } else {
                    0.
                },
            ),
            (
                "HDA_ICON",
                if hdp {
                    5.
                } else if i.enabled {
                    2.
                } else if i.main_enabled {
                    1.
                } else {
                    0.
                },
            ),
            (
                "LFA_ICON",
                if hdp {
                    5.
                } else if i.active {
                    2.
                } else if i.lat_enabled {
                    1.
                } else {
                    0.
                },
            ),
            (
                "LKA_ICON",
                if i.active {
                    4.
                } else if i.lat_enabled {
                    3.
                } else {
                    0.
                },
            ),
            ("FCA_ALT_ICON", 0.),
        ],
    );
    cluster_fields::hide_alerts(data, true, i.soft_hold, i.trailer)?;
    cluster_fields::lane_lines(data, (i.steering_angle, i.active, 0));
    for (side, index) in [("LEFT", 0), ("RIGHT", 1)] {
        let color = if i.change_available[index] { 6. } else { 2. };
        let warn = if i.lane_check >= 1 {
            !matches!(i.lane_lines[index].rem_euclid(10), 0 | 5)
        } else {
            i.lane_lines[index].div_euclid(10) == 2
        };
        let color = if warn || i.blindspot[index] {
            4.
        } else {
            color
        };
        let lane = if i.trailer {
            if i.lane_visible[index] {
                2.
            } else {
                0.
            }
        } else if i.lane_depart[index] {
            if (i.frame / 50).is_multiple_of(2) {
                4.
            } else {
                1.
            }
        } else if i.lane_visible[index] {
            color
        } else {
            0.
        };
        data.insert(format!("LANELINE_{side}"), lane);
        data.insert(
            format!("LCA_{side}_ARROW"),
            if i.blinker[index] { 2. } else { 0. },
        );
        data.insert(
            format!("LCA_{side}_ICON"),
            if i.active {
                if i.trailer || i.blindspot[index] {
                    1.
                } else {
                    2.
                }
            } else {
                0.
            },
        );
        let desire = match index {
            0 => matches!(i.desire, 1 | 3),
            1 => matches!(i.desire, 2 | 4),
            _ => false,
        };
        data.insert(
            format!("LANE_{side}"),
            if !i.trailer && desire { 1. } else { 0. },
        );
    }
    Ok(())
}
