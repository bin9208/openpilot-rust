use super::NavigationRuntime;
use crate::sources::{Control, Instruction, Lifecycle, SafetyItem, Snapshot, Source};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Auxiliary {
    pub route: Option<Vec<(f64, f64)>>,
    pub traffic: Option<Traffic>,
    pub route_received: Option<f64>,
    pub traffic_received: Option<f64>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Traffic {
    pub present: bool,
    pub visible: bool,
    pub distance: f64,
    pub source: String,
    pub lamp: String,
    pub remain: i64,
}
#[derive(Clone, Debug)]
pub struct LegacyFields {
    pub raw_road_limit: f64,
    pub control: Control,
}

fn number(data: &Value, name: &str, fallback: f64) -> f64 {
    let value = match data.get(name) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => openpilot_runtime_core::python_float::parse(s),
        Some(Value::Bool(b)) => Some(if *b { 1. } else { 0. }),
        _ => None,
    };
    value.filter(|v| v.is_finite()).unwrap_or(fallback)
}
fn integer(data: &Value, name: &str, fallback: i64) -> i64 {
    if let Some(Value::String(s)) = data.get(name) {
        let text = s.trim();
        if text
            .chars()
            .enumerate()
            .any(|(i, c)| !(c.is_numeric() || c == '_' || (i == 0 && matches!(c, '+' | '-'))))
        {
            return fallback;
        }
        return text
            .replace('_', "")
            .parse::<i64>()
            .ok()
            .or_else(|| {
                openpilot_runtime_core::python_float::parse(text)
                    .and_then(|n| num_traits::ToPrimitive::to_i64(&n))
            })
            .unwrap_or(fallback);
    }
    let fallback_float = num_traits::ToPrimitive::to_f64(&fallback).unwrap_or(0.);
    num_traits::ToPrimitive::to_i64(&number(data, name, fallback_float).trunc()).unwrap_or(fallback)
}
fn text(data: &Value, name: &str) -> String {
    match data.get(name) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(true)) => "True".into(),
        Some(Value::Bool(false)) => String::new(),
        _ => String::new(),
    }
}

pub fn parse_legacy(data: &Value, now: f64) -> Option<LegacyFields> {
    if !data.is_object() || data.get("nRoadLimitSpeed").is_none() {
        return None;
    }
    let safety = |secondary: bool| {
        let prefix = if secondary { "nSdiPlus" } else { "nSdi" };
        let kind = integer(data, &format!("{prefix}Type"), -1);
        (kind >= 0).then(|| SafetyItem {
            kind,
            distance_m: number(data, &format!("{prefix}Dist"), 0.),
            speed_limit_kph: number(data, &format!("{prefix}SpeedLimit"), 0.),
            received_mono_s: now,
            reason: if kind == 22 {
                "bump"
            } else if kind == 4 && !secondary {
                "section"
            } else {
                "cam"
            }
            .into(),
            section: false,
            section_type: integer(data, &format!("{prefix}Section"), -1),
            block_type: integer(data, &format!("{prefix}BlockType"), -1),
            block_speed_kph: number(data, &format!("{prefix}BlockSpeed"), 0.),
            block_distance_m: number(data, &format!("{prefix}BlockDist"), 0.),
            revision: None,
        })
    };
    let current_type = integer(data, "nTBTTurnType", -1);
    let next_type = integer(data, "nTBTTurnTypeNext", -1);
    let latitude = number(data, "vpPosPointLat", 0.);
    let longitude = number(data, "vpPosPointLon", 0.);
    let position =
        latitude != 0. && (-90. ..=90.).contains(&latitude) && (-180. ..=180.).contains(&longitude);
    let route = ["nGoPosDist", "nGoPosTime", "goalPosX", "goalPosY"]
        .iter()
        .any(|key| data.get(key).is_some());
    let destination = data.get("goalPosX").is_some_and(|v| !v.is_null())
        && data.get("goalPosY").is_some_and(|v| !v.is_null());
    let category = data.get("roadcate").map(|_| integer(data, "roadcate", 0));
    let road_name = text(data, "szPosRoadName");
    Some(LegacyFields {
        raw_road_limit: number(data, "nRoadLimitSpeed", 20.).trunc(),
        control: Control {
            current: Instruction {
                present: current_type >= 0,
                turn_type: current_type,
                distance_m: number(data, "nTBTDist", 0.),
                road_name: road_name.clone(),
                main_text: text(data, "szTBTMainText"),
                near_direction: text(data, "szNearDirName"),
                far_direction: text(data, "szFarDirName"),
                next_road_width: integer(data, "nTBTNextRoadWidth", 0),
                received_mono_s: (current_type >= 0).then_some(now),
            },
            next: Instruction {
                present: next_type >= 0,
                turn_type: next_type,
                distance_m: number(data, "nTBTDistNext", 0.),
                main_text: text(
                    data,
                    if data.get("szTBTMainTextNext").is_some() {
                        "szTBTMainTextNext"
                    } else {
                        "szTBTMainText"
                    },
                ),
                received_mono_s: (next_type >= 0).then_some(now),
                ..Instruction::default()
            },
            safety: safety(false),
            secondary_safety: safety(true),
            speed_present: true,
            speed_received_mono_s: Some(now),
            road_limit_received_mono_s: Some(now),
            road_category: category,
            road_category_received_mono_s: category.map(|_| now),
            route_present: route,
            route_received_mono_s: route.then_some(now),
            remaining_distance_m: number(data, "nGoPosDist", 0.),
            remaining_time_s: number(data, "nGoPosTime", 0.),
            destination: destination
                .then(|| (number(data, "goalPosY", 0.), number(data, "goalPosX", 0.))),
            destination_present: destination,
            destination_received_mono_s: destination.then_some(now),
            position_present: position,
            position_received_mono_s: position.then_some(now),
            position_latitude: if position { latitude } else { 0. },
            position_longitude: if position { longitude } else { 0. },
            position_heading_deg: number(data, "nPosAngle", 0.),
            position_speed_kph: number(data, "nPosSpeed", 0.),
            position_road_name: if road_name == "null" {
                String::new()
            } else {
                road_name
            },
            ..Control::default()
        },
    })
}

impl Auxiliary {
    fn apply(&self, control: &mut Control, now: f64) {
        if let Some(points) = &self.route {
            control.route_points = points.clone();
            control.route_present = !points.is_empty();
            control.route_received_mono_s = control
                .route_present
                .then_some(self.route_received.unwrap_or(now));
        }
        if let Some(traffic) = &self.traffic {
            control.traffic_present = traffic.present;
            control.traffic_received_mono_s = traffic
                .present
                .then_some(self.traffic_received.unwrap_or(now));
            control.traffic_visible = traffic.visible;
            control.traffic_distance_m = traffic.distance;
            control.traffic_source = traffic.source.clone();
            control.traffic_lamp = traffic.lamp.clone();
            control.traffic_remain_s = traffic.remain;
        }
    }
    fn merge(&mut self, newer: Auxiliary) {
        if newer.route.is_some() {
            self.route = newer.route;
            self.route_received = newer.route_received;
        }
        if newer.traffic.is_some() {
            self.traffic = newer.traffic;
            self.traffic_received = newer.traffic_received;
        }
    }
}

impl NavigationRuntime {
    pub fn accept_legacy(&mut self, mut fields: LegacyFields, session: &str, now: f64) -> bool {
        if session.is_empty() {
            return false;
        }
        self.legacy_sequence += 1;
        let road = if fields.raw_road_limit > 200. {
            (fields.raw_road_limit - 20.) / 10.
        } else if fields.raw_road_limit == 120. {
            115.
        } else if fields.raw_road_limit <= 0. {
            30.
        } else {
            fields.raw_road_limit
        };
        let state = self
            .legacy_road_limits
            .entry(session.into())
            .or_insert((30., None, 0));
        if road == state.0 {
            *state = (road, Some(road), 0);
        } else {
            let count = if state.1 == Some(road) {
                state.2 + 1
            } else {
                1
            };
            *state = if count > 5 {
                (road, Some(road), 0)
            } else {
                (state.0, Some(road), count)
            };
        }
        fields.control.road_limit_kph = Some(state.0);
        if let Some(previous) = self.legacy_controls.get(session) {
            if !previous.route_points.is_empty() {
                if !fields.control.route_present {
                    fields.control.route_received_mono_s = previous.route_received_mono_s;
                }
                fields.control.route_present = true;
                fields.control.route_points = previous.route_points.clone();
            }
            if previous.traffic_present {
                fields.control.traffic_present = true;
                fields.control.traffic_received_mono_s = previous.traffic_received_mono_s;
                fields.control.traffic_visible = previous.traffic_visible;
                fields.control.traffic_distance_m = previous.traffic_distance_m;
                fields.control.traffic_source = previous.traffic_source.clone();
                fields.control.traffic_lamp = previous.traffic_lamp.clone();
                fields.control.traffic_remain_s = previous.traffic_remain_s;
            }
            if !fields.control.destination_present
                && previous.destination_present
                && previous.destination.is_some()
            {
                fields.control.destination = previous.destination;
                fields.control.destination_present = true;
                fields.control.destination_received_mono_s = previous.destination_received_mono_s;
            }
        }
        if let Some((_, pending)) = self.legacy_pending.iter().find(|(key, _)| key == session) {
            pending.apply(&mut fields.control, now);
        }
        let snapshot = Snapshot {
            source: Source::TmapLegacy,
            session_id: session.into(),
            sequence: self.legacy_sequence,
            lifecycle: Lifecycle::Guiding,
            received_mono_s: now,
            activation_epoch: 0,
            control: fields.control,
            owner_received_mono_s: None,
            original_json: None,
        };
        if !self.accept(snapshot.clone()) {
            return false;
        }
        self.legacy_controls.clear();
        self.legacy_controls
            .insert(session.into(), snapshot.control.clone());
        self.legacy_road_limits.retain(|key, _| key == session);
        self.legacy_snapshot = Some(snapshot);
        self.legacy_pending.retain(|(key, _)| key != session);
        true
    }

    pub fn accept_legacy_aux(&mut self, session: &str, now: f64, mut auxiliary: Auxiliary) -> bool {
        if auxiliary.route.is_some() {
            auxiliary.route_received = Some(now);
        }
        if auxiliary.traffic.is_some() {
            auxiliary.traffic_received = Some(now);
        }
        if session.is_empty()
            || !now.is_finite()
            || (auxiliary.route.is_none() && auxiliary.traffic.is_none())
        {
            return false;
        }
        if auxiliary.route.as_ref().is_some_and(|points| {
            points.len() > 4096
                || points.iter().any(|p| {
                    !p.0.is_finite()
                        || !p.1.is_finite()
                        || !(-90. ..=90.).contains(&p.0)
                        || !(-180. ..=180.).contains(&p.1)
                })
        }) {
            return false;
        }
        if self
            .legacy_snapshot
            .as_ref()
            .is_some_and(|p| p.session_id == session && now < p.received_mono_s)
        {
            return false;
        }
        let Some(previous) = self
            .legacy_snapshot
            .as_ref()
            .filter(|p| p.session_id == session)
            .cloned()
        else {
            if let Some((_, pending)) = self
                .legacy_pending
                .iter_mut()
                .find(|(key, _)| key == session)
            {
                pending.merge(auxiliary);
            } else {
                if self.legacy_pending.len() >= 4 {
                    self.legacy_pending.remove(0);
                }
                self.legacy_pending.push((session.into(), auxiliary));
            }
            return false;
        };
        self.legacy_sequence += 1;
        let mut snapshot = previous;
        snapshot.sequence = self.legacy_sequence;
        snapshot.owner_received_mono_s = Some(snapshot.owner_receipt());
        snapshot.received_mono_s = now;
        auxiliary.apply(&mut snapshot.control, now);
        if !self.accept(snapshot.clone()) {
            return false;
        }
        self.legacy_controls.clear();
        self.legacy_controls
            .insert(session.into(), snapshot.control.clone());
        self.legacy_snapshot = Some(snapshot);
        true
    }
}
