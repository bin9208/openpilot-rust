use super::{CarrotServ, NavigationState};
use crate::{navigation::Projected, sources::Source};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub enum TrafficAction {
    Put {
        distance: f64,
        lamp: String,
        remain: i64,
        source: String,
        ts: f64,
    },
    Remove,
}

impl CarrotServ {
    pub fn project(&mut self, projected: Projected) -> Option<TrafficAction> {
        let was_active = self.navi_active;
        self.nav.next_road_width = projected
            .selection
            .snapshot
            .as_ref()
            .map_or(0, |s| s.control.current.next_road_width);
        self.navi_active = projected.selection.snapshot.is_some();
        let Some(snapshot) = &projected.selection.snapshot else {
            self.projected = Some(projected);
            if was_active {
                return self.clear_control();
            }
            return None;
        };
        let new_session = projected.session_id != self.session_id;
        let mut traffic_action = None;
        if new_session {
            traffic_action = self.clear_control();
            self.session_id = projected.session_id.clone();
        }
        let off_route_changed = snapshot.control.off_route != self.off_route;
        let control = &snapshot.control;
        self.nav.next_road_width = control.current.next_road_width;
        if let Some(destination) = control.destination.filter(|_| control.destination_present) {
            (self.nav.goal_latitude, self.nav.goal_longitude) = destination;
        }
        self.off_route = control.off_route;
        self.road_limit_valid = control.road_limit_kph.is_some();
        if let Some(road) = control.road_limit_kph {
            self.nav.road_limit = road.round_ties_even();
        }
        self.has_control = control.speed_present
            || control.current.present
            || control.next.present
            || control.route_present
            || self.navi_active;
        self.nav.road_category = control.road_category.unwrap_or(0);
        if new_session || off_route_changed || projected.sequences[0] != self.sequences[0] {
            self.sequences[0] = projected.sequences[0];
            let identity = Some((
                snapshot.source,
                snapshot.session_id.clone(),
                control.safety.clone(),
                control.secondary_safety.clone(),
            ));
            if identity == self.disabled_safety {
                self.nav.speed_type = -1;
                self.nav.speed_limit = 0.;
                self.nav.speed_distance = 0.;
            } else {
                let primary = control.safety.as_ref().filter(|_| !control.off_route);
                let secondary = control
                    .secondary_safety
                    .as_ref()
                    .filter(|_| !control.off_route);
                self.nav.sdi_type = primary.map_or(-1, |s| if s.section { 4 } else { s.kind });
                self.nav.sdi_limit = primary.map_or(0., |s| s.speed_limit_kph.round_ties_even());
                self.nav.sdi_distance = primary.map_or(0., |s| s.distance_m.round_ties_even());
                self.nav.sdi_section =
                    primary.map_or(-1, |s| if s.section { 1 } else { s.section_type });
                self.nav.block_type =
                    primary.map_or(-1, |s| if s.section { 2 } else { s.block_type });
                self.nav.block_speed = primary.map_or(0., |s| {
                    if s.section {
                        s.speed_limit_kph
                    } else {
                        s.block_speed_kph
                    }
                    .round_ties_even()
                });
                self.nav.block_distance = primary.map_or(0., |s| {
                    if s.section {
                        s.distance_m
                    } else {
                        s.block_distance_m
                    }
                    .round_ties_even()
                });
                self.nav.secondary_type = secondary.map_or(-1, |s| s.kind);
                self.nav.secondary_limit =
                    secondary.map_or(0., |s| s.speed_limit_kph.round_ties_even());
                self.nav.secondary_distance =
                    secondary.map_or(0., |s| s.distance_m.round_ties_even());
                self.nav.secondary_block_type = secondary.map_or(-1, |s| s.block_type);
                self.nav.secondary_block_speed =
                    secondary.map_or(0., |s| s.block_speed_kph.round_ties_even());
                self.nav.secondary_block_distance =
                    secondary.map_or(0., |s| s.block_distance_m.round_ties_even());
                let naver_bump = snapshot.source == Source::NaverV1
                    && control.road_category.is_none()
                    && [primary, secondary]
                        .iter()
                        .any(|s| s.is_some_and(|s| s.kind == 22 && s.distance_m > 0.));
                self.update_sdi(naver_bump);
            }
        }
        if new_session || projected.sequences[3] != self.sequences[3] {
            self.sequences[3] = projected.sequences[3];
            if !control.position_present {
                self.gps.last_navi = 0.;
                self.nav.road_name.clear();
            } else {
                self.gps.navi_latitude = control.position_latitude;
                self.gps.navi_longitude = control.position_longitude;
                self.gps.angle = control.position_heading_deg;
                self.gps.speed = control.position_speed_kph;
                self.nav.road_name = control.position_road_name.clone();
                self.gps.last_navi = control
                    .position_received_mono_s
                    .unwrap_or(snapshot.received_mono_s);
                self.gps.last_calculate = self.gps.last_navi;
            }
        }
        if new_session || projected.sequences[5] != self.sequences[5] {
            self.sequences[5] = projected.sequences[5];
            if !control.traffic_present
                || !control.traffic_visible
                || control.traffic_lamp.is_empty()
                || control.traffic_remain_s <= 0
            {
                if self.traffic_active {
                    self.traffic_active = false;
                    traffic_action = Some(TrafficAction::Remove);
                }
            } else {
                self.traffic_active = true;
                traffic_action = Some(TrafficAction::Put {
                    distance: control.traffic_distance_m.round_ties_even(),
                    lamp: control.traffic_lamp.clone(),
                    remain: control.traffic_remain_s,
                    source: control.traffic_source.clone(),
                    ts: control
                        .traffic_received_mono_s
                        .unwrap_or(snapshot.received_mono_s),
                });
            }
        }
        self.sequences[4] = projected.sequences[4];
        self.nav.goal_distance = if control.route_present {
            control.remaining_distance_m.round_ties_even()
        } else {
            0.
        };
        self.nav.goal_time = if control.route_present {
            control.remaining_time_s.round_ties_even()
        } else {
            0.
        };
        let current_changed =
            new_session || off_route_changed || projected.sequences[1] != self.sequences[1];
        let next_changed =
            new_session || off_route_changed || projected.sequences[2] != self.sequences[2];
        if current_changed || next_changed {
            let old_current = self.nav.x_turn_distance;
            let old_next = self.nav.x_next_turn_distance;
            if current_changed {
                self.sequences[1] = projected.sequences[1];
                let present = control.current.present && !self.off_route;
                self.nav.turn_distance = if present {
                    control.current.distance_m.round_ties_even()
                } else {
                    0.
                };
                self.nav.turn_type = if present {
                    control.current.turn_type
                } else {
                    -1
                };
                self.nav.main_text = if present {
                    control.current.main_text.clone()
                } else {
                    String::new()
                };
                self.main_text_python_json = if present {
                    control.current.original_main_text_json.clone()
                } else {
                    None
                };
                self.nav.near_direction = if present {
                    control.current.near_direction.clone()
                } else {
                    String::new()
                };
                self.nav.far_direction = if present {
                    control.current.far_direction.clone()
                } else {
                    String::new()
                };
            }
            if next_changed {
                self.sequences[2] = projected.sequences[2];
                let present = control.next.present && !self.off_route;
                self.nav.next_turn_distance = if present {
                    control.next.distance_m.round_ties_even()
                } else {
                    0.
                };
                self.nav.next_turn_type = if present { control.next.turn_type } else { -1 };
                self.nav.next_main_text = if present {
                    control.next.main_text.clone()
                } else {
                    String::new()
                };
            }
            self.update_tbt();
            if !current_changed {
                self.nav.x_turn_distance = old_current;
            }
            if !next_changed {
                self.nav.x_next_turn_distance = old_next;
            }
        }
        if self.has_control {
            self.nav.active_count = 80;
            self.nav.active_sdi_count = 200;
        }
        self.projected = Some(projected);
        traffic_action
    }

    fn clear_control(&mut self) -> Option<TrafficAction> {
        let road = self.nav.road_limit;
        let last_road = self.nav.last_road_limit;
        let active_kisa = self.nav.active_kisa_count;
        self.nav = NavigationState {
            road_limit: road,
            last_road_limit: last_road,
            active_kisa_count: active_kisa,
            sdi_section: -1,
            active_carrot: self.nav.active_carrot,
            road_category: self.nav.road_category,
            next_road_width: self.nav.next_road_width,
            goal_latitude: self.nav.goal_latitude,
            goal_longitude: self.nav.goal_longitude,
            main_text: self.nav.main_text.clone(),
            next_main_text: self.nav.next_main_text.clone(),
            near_direction: self.nav.near_direction.clone(),
            far_direction: self.nav.far_direction.clone(),
            ..NavigationState::default()
        };
        self.speed.rear_events.clear();
        self.has_control = false;
        self.road_limit_valid = false;
        self.off_route = false;
        self.disabled_safety = None;
        self.session_id.clear();
        self.sequences = [0; 6];
        self.gps.last_navi = 0.;
        if self.traffic_active {
            self.traffic_active = false;
            Some(TrafficAction::Remove)
        } else {
            None
        }
    }
    fn update_sdi(&mut self, naver_bump: bool) {
        if matches!(self.nav.sdi_type, 0 | 1 | 2 | 3 | 4 | 7 | 8 | 75 | 76)
            && self.nav.sdi_limit > 0.
            && self.settings.control_mode > 0
        {
            self.nav.speed_limit = self.nav.sdi_limit * self.settings.safety_factor;
            self.nav.speed_distance = self.nav.sdi_distance;
            self.nav.speed_type = self.nav.sdi_type;
            if matches!(self.nav.block_type, 2 | 3) {
                self.nav.speed_distance = self.nav.block_distance;
                self.nav.speed_type = 4;
            } else if self.nav.sdi_type == 7 && self.settings.control_mode < 3 {
                self.nav.speed_limit = 0.;
                self.nav.speed_distance = 0.;
            }
        } else if (self.nav.secondary_type == 22 || self.nav.sdi_type == 22)
            && (self.nav.road_category > 1 || naver_bump)
            && self.settings.control_mode >= 2
        {
            self.nav.speed_limit = self.settings.bump_speed;
            self.nav.speed_distance = if self.nav.secondary_type == 22 {
                self.nav.secondary_distance
            } else {
                self.nav.sdi_distance
            };
            self.nav.speed_type = 22;
        } else {
            self.nav.speed_limit = 0.;
            self.nav.speed_distance = 0.;
            self.nav.speed_type = -1;
        }
    }
}
