use super::NavigationRuntime;
use crate::sources::{Control, Instruction, Lifecycle, SafetyItem, Snapshot, Source};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Meta {
    pub present: bool,
    pub sequence: u64,
    pub received_mono_time_nanos: u64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Guidance {
    pub meta: Meta,
    pub distance_m: i64,
    pub turn_type: i64,
    pub main_text: String,
    pub near_direction: String,
    pub far_direction: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Vehicle {
    pub meta: Meta,
    pub latitude: f64,
    pub longitude: f64,
    pub heading_deg: f64,
    pub speed_kph: f64,
    pub road_name: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Route {
    pub meta: Meta,
    pub remaining_distance_m: i64,
    pub remaining_time_sec: i64,
    pub polyline: Vec<Coordinate>,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Coordinate {
    pub latitude: f64,
    pub longitude: f64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Status {
    pub meta: Meta,
    pub off_route: bool,
    pub guidance_active: bool,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Lane {
    pub meta: Meta,
    pub road_category: i64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Traffic {
    pub meta: Meta,
    pub visible: bool,
    pub distance_m: i64,
    pub source: String,
    pub red_valid: bool,
    pub red_on: bool,
    pub red_remain_sec: i64,
    pub left_valid: bool,
    pub left_on: bool,
    pub left_remain_sec: i64,
    pub green_valid: bool,
    pub green_on: bool,
    pub green_remain_sec: i64,
    pub right_valid: bool,
    pub right_on: bool,
    pub right_remain_sec: i64,
    pub uturn_valid: bool,
    pub uturn_on: bool,
    pub uturn_remain_sec: i64,
    pub ui_counter_valid: bool,
    pub ui_counter_remain_sec: i64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Speed {
    pub meta: Meta,
    pub road_limit_valid: bool,
    pub road_limit_kph: i64,
    pub sdi_present: bool,
    pub sdi_type: i64,
    pub sdi_distance_m: i64,
    pub sdi_speed_limit_kph: i64,
    pub sdi_section_type: i64,
    pub sdi_block_type: i64,
    pub sdi_block_speed_kph: i64,
    pub sdi_block_distance_m: i64,
    pub secondary_sdi_present: bool,
    pub secondary_sdi_type: i64,
    pub secondary_sdi_distance_m: i64,
    pub secondary_sdi_speed_limit_kph: i64,
    pub secondary_sdi_section_type: i64,
    pub secondary_sdi_block_type: i64,
    pub secondary_sdi_block_speed_kph: i64,
    pub secondary_sdi_block_distance_m: i64,
    pub section_present: bool,
    pub section_active: bool,
    pub section_suspended: bool,
    pub section_off_route: bool,
    pub section_speed_limit_kph: i64,
    pub section_remaining_distance_m: f64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Payload {
    pub schema_version: i64,
    pub connected: bool,
    pub session_id: String,
    pub guidance_current: Guidance,
    pub guidance_next: Guidance,
    pub vehicle: Vehicle,
    pub speed: Speed,
    pub route: Route,
    pub navigation_status: Status,
    pub lane_current: Lane,
    pub traffic_signal: Traffic,
}

fn number(value: i64) -> f64 {
    num_traits::ToPrimitive::to_f64(&value).unwrap_or(0.)
}
fn receipt(meta: &Meta, previous: Option<&(u64, f64)>, now: f64) -> f64 {
    if meta.received_mono_time_nanos > 0 {
        num_traits::ToPrimitive::to_f64(&meta.received_mono_time_nanos).unwrap_or(0.) / 1e9
    } else {
        previous
            .filter(|p| p.0 == meta.sequence)
            .map_or(now, |p| p.1)
    }
}

impl NavigationRuntime {
    pub fn v2_transport_lost(&mut self, now: f64) -> bool {
        self.store
            .record_transport_loss(Source::CarrotNaviV2, &self.v2_session, now)
    }

    pub fn accept_v2(&mut self, payload: Payload, now: f64) -> bool {
        if payload.schema_version != 1 || !payload.connected || payload.session_id.is_empty() {
            self.v2_transport_lost(now);
            return false;
        }
        if self.v2_session != payload.session_id {
            self.v2_items.clear();
        }
        self.v2_session = payload.session_id.clone();
        self.v2_sequence += 1;
        let mut times = [0.; 8];
        for (index, (name, meta)) in [
            ("speed", &payload.speed.meta),
            ("guidanceCurrent", &payload.guidance_current.meta),
            ("guidanceNext", &payload.guidance_next.meta),
            ("vehicle", &payload.vehicle.meta),
            ("route", &payload.route.meta),
            ("trafficSignal", &payload.traffic_signal.meta),
            ("laneCurrent", &payload.lane_current.meta),
            ("navigationStatus", &payload.navigation_status.meta),
        ]
        .into_iter()
        .enumerate()
        {
            times[index] = receipt(meta, self.v2_items.get(name), now);
            if meta.received_mono_time_nanos == 0 {
                self.v2_items.insert(name, (meta.sequence, times[index]));
            }
        }
        if times.iter().any(|t| !t.is_finite() || *t > now) {
            return false;
        }
        let off_route =
            payload.navigation_status.meta.present && payload.navigation_status.off_route;
        let guidance_active =
            payload.navigation_status.meta.present && payload.navigation_status.guidance_active;
        let guidance = |g: Guidance, time: f64| {
            let present = g.meta.present && !off_route;
            Instruction {
                present,
                distance_m: if present {
                    number(g.distance_m.max(0))
                } else {
                    0.
                },
                turn_type: if present { g.turn_type } else { -1 },
                main_text: if present { g.main_text } else { String::new() },
                near_direction: if present {
                    g.near_direction
                } else {
                    String::new()
                },
                far_direction: if present {
                    g.far_direction
                } else {
                    String::new()
                },
                received_mono_s: present.then_some(time),
                ..Instruction::default()
            }
        };
        let s = &payload.speed;
        let safety = |secondary: bool| -> Option<SafetyItem> {
            let (present, kind, distance, speed, section, block, block_speed, block_distance) =
                if secondary {
                    (
                        s.secondary_sdi_present,
                        s.secondary_sdi_type,
                        s.secondary_sdi_distance_m,
                        s.secondary_sdi_speed_limit_kph,
                        s.secondary_sdi_section_type,
                        s.secondary_sdi_block_type,
                        s.secondary_sdi_block_speed_kph,
                        s.secondary_sdi_block_distance_m,
                    )
                } else {
                    (
                        s.sdi_present,
                        s.sdi_type,
                        s.sdi_distance_m,
                        s.sdi_speed_limit_kph,
                        s.sdi_section_type,
                        s.sdi_block_type,
                        s.sdi_block_speed_kph,
                        s.sdi_block_distance_m,
                    )
                };
            (s.meta.present && present && !off_route).then(|| SafetyItem {
                kind,
                distance_m: number(distance.max(0)),
                speed_limit_kph: number(speed.max(0)),
                received_mono_s: times[0],
                reason: if kind == 22 { "bump" } else { "cam" }.into(),
                section: false,
                section_type: section,
                block_type: block,
                block_speed_kph: number(block_speed.max(0)),
                block_distance_m: number(block_distance.max(0)),
                revision: None,
            })
        };
        let mut primary = safety(false);
        if s.meta.present
            && s.section_present
            && s.section_active
            && !s.section_suspended
            && !s.section_off_route
            && !off_route
            && s.section_speed_limit_kph > 0
        {
            let distance = s.section_remaining_distance_m.round_ties_even().max(0.);
            primary = Some(SafetyItem {
                kind: 4,
                distance_m: distance,
                speed_limit_kph: number(s.section_speed_limit_kph),
                received_mono_s: times[0],
                reason: "section".into(),
                section: true,
                section_type: 1,
                block_type: 2,
                block_speed_kph: number(s.section_speed_limit_kph),
                block_distance_m: distance,
                revision: None,
            });
        }
        let road = (s.meta.present
            && s.road_limit_valid
            && s.road_limit_kph > 0
            && s.road_limit_kph <= 200
            && s.road_limit_kph % 10 == 0)
            .then(|| number(s.road_limit_kph));
        let p = &payload.vehicle;
        let position = p.meta.present
            && (-90. ..=90.).contains(&p.latitude)
            && (-180. ..=180.).contains(&p.longitude);
        let t = &payload.traffic_signal;
        let visible = t.meta.present && t.visible;
        let lamps = [
            ("red", t.red_valid, t.red_on, t.red_remain_sec),
            ("left", t.left_valid, t.left_on, t.left_remain_sec),
            ("green", t.green_valid, t.green_on, t.green_remain_sec),
            ("right", t.right_valid, t.right_on, t.right_remain_sec),
            ("uturn", t.uturn_valid, t.uturn_on, t.uturn_remain_sec),
        ];
        let (lamp, remain) = lamps
            .into_iter()
            .find(|(_, valid, on, _)| visible && *valid && *on)
            .map_or(("", 0), |(name, _, _, remain)| {
                (
                    name,
                    if remain > 0 {
                        remain
                    } else if t.ui_counter_valid {
                        t.ui_counter_remain_sec.max(0)
                    } else {
                        0
                    },
                )
            });
        let c = Control {
            current: guidance(payload.guidance_current, times[1]),
            next: guidance(payload.guidance_next, times[2]),
            safety: primary,
            secondary_safety: safety(true),
            speed_present: s.meta.present,
            speed_received_mono_s: s.meta.present.then_some(times[0]),
            road_limit_kph: road,
            road_limit_received_mono_s: road.map(|_| times[0]),
            road_category: payload
                .lane_current
                .meta
                .present
                .then_some(payload.lane_current.road_category),
            road_category_received_mono_s: payload.lane_current.meta.present.then_some(times[6]),
            route_present: payload.route.meta.present,
            route_revision: Some(payload.route.meta.sequence),
            route_received_mono_s: payload.route.meta.present.then_some(times[4]),
            remaining_distance_m: if payload.route.meta.present {
                number(payload.route.remaining_distance_m.max(0))
            } else {
                0.
            },
            remaining_time_s: if payload.route.meta.present {
                number(payload.route.remaining_time_sec.max(0))
            } else {
                0.
            },
            route_points: if payload.route.meta.present {
                payload
                    .route
                    .polyline
                    .iter()
                    .take(256)
                    .filter(|p| {
                        (-90. ..=90.).contains(&p.latitude)
                            && (-180. ..=180.).contains(&p.longitude)
                    })
                    .map(|p| (p.latitude, p.longitude))
                    .collect()
            } else {
                Vec::new()
            },
            off_route,
            status_present: payload.navigation_status.meta.present,
            status_received_mono_s: payload.navigation_status.meta.present.then_some(times[7]),
            position_present: position,
            position_received_mono_s: position.then_some(times[3]),
            position_latitude: if position { p.latitude } else { 0. },
            position_longitude: if position { p.longitude } else { 0. },
            position_heading_deg: if position && p.heading_deg.is_finite() {
                p.heading_deg.rem_euclid(360.)
            } else {
                0.
            },
            position_speed_kph: if position && p.speed_kph.is_finite() {
                p.speed_kph.max(0.)
            } else {
                0.
            },
            position_road_name: if position {
                p.road_name.clone()
            } else {
                String::new()
            },
            traffic_present: t.meta.present,
            traffic_received_mono_s: t.meta.present.then_some(times[5]),
            traffic_visible: visible,
            traffic_distance_m: if t.meta.present {
                number(t.distance_m.max(0))
            } else {
                0.
            },
            traffic_source: if t.meta.present {
                t.source.clone()
            } else {
                String::new()
            },
            traffic_lamp: lamp.into(),
            traffic_remain_s: remain,
            ..Control::default()
        };
        let guiding = guidance_active
            || c.speed_present
            || c.current.present
            || c.next.present
            || c.route_present;
        self.accept(Snapshot {
            source: Source::CarrotNaviV2,
            session_id: payload.session_id,
            sequence: self.v2_sequence,
            lifecycle: if guiding {
                Lifecycle::Guiding
            } else {
                Lifecycle::Idle
            },
            received_mono_s: now,
            activation_epoch: 0,
            control: c,
            owner_received_mono_s: None,
            original_json: None,
        })
    }
}
