use super::{corner_id, Data, Radar};
use crate::models::{radar_position_is_zero, RadarPoint};
use std::cmp::Ordering;

impl Radar {
    pub fn tracks(&mut self, tracks: Data<'_>, time: f64, speed: f64) {
        let mut points = std::collections::BTreeMap::new();
        for (index, track) in tracks.field("points").items().into_iter().enumerate() {
            if let Some(point) = track_point(track, index, speed, self.brand == "hyundai") {
                if self.front_only && point.source == "cornerRadar" {
                    continue;
                }
                if point.source == "cornerRadar" {
                    self.corner_seen = true;
                }
                points.insert(point.label.clone(), point);
            }
        }
        self.points = points;
        self.point_time = time;
    }
    pub fn visible_points(&self, time: f64) -> Vec<RadarPoint> {
        if time - self.point_time >= 0.25 {
            return Vec::new();
        }
        let corners: Vec<_> = self
            .points
            .values()
            .filter(|point| point.source == "cornerRadar")
            .cloned()
            .collect();
        let mut points = if !corners.is_empty() {
            corners
        } else if self.active() {
            Vec::new()
        } else {
            self.points.values().cloned().collect()
        };
        points.sort_by(|left, right| {
            left.longitudinal_m
                .partial_cmp(&right.longitudinal_m)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    left.lateral_m
                        .abs()
                        .partial_cmp(&right.lateral_m.abs())
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| left.label.cmp(&right.label))
        });
        points
    }
}
fn track_point(track: Data<'_>, index: usize, speed: f64, legacy: bool) -> Option<RadarPoint> {
    let longitudinal = track.field("dRel").float()?;
    let lateral = -track.number("yRel", 0.0);
    if !(0.0..=180.0).contains(&longitudinal)
        || !(-12.0..=12.0).contains(&lateral)
        || radar_position_is_zero(longitudinal, lateral)
    {
        return None;
    }
    let id = track.field("trackId").int();
    let source = track
        .field("radarSource")
        .text()
        .unwrap_or_else(|| "frontRadar".into());
    let corner = matches!(source.as_str(), "corner235" | "corner180" | "corner430")
        || (legacy && source == "frontRadar" && corner_id(id));
    let label = if corner {
        corner_label(id?, &source)
    } else {
        id.map_or_else(|| format!("T{index:03}"), |id| format!("T{id}"))
    };
    let relative = track.field("vRel").float();
    let absolute = track
        .field("vLead")
        .float()
        .map(|value| value * 3.6)
        .or_else(|| relative.map(|value| speed + value * 3.6));
    let measured = track.flag("measured", true);
    Some(RadarPoint {
        label,
        longitudinal_m: longitudinal,
        lateral_m: lateral,
        source: if corner {
            "cornerRadar".into()
        } else {
            "liveTracks".into()
        },
        relative_speed_mps: relative,
        absolute_speed_kph: absolute,
        lateral_speed_mps: track.field("yvRel").float().map(|value| -value),
        relative_accel_mps2: track.field("aRel").float(),
        probability: Some(if measured { 0.72 } else { 0.38 }),
        valid: Some(i64::from(measured)),
        ..RadarPoint::default()
    })
}
fn corner_label(id: i64, source: &str) -> String {
    if id >= 1000 {
        return format!(
            "CR{}_T{id}",
            match source {
                "corner180" => "180",
                "corner430" => "430",
                _ => "235",
            }
        );
    }
    if (240..250).contains(&id) {
        return format!("CR180_{:02}", id - 240);
    }
    if (300..412).contains(&id) {
        return format!("CR430_{:03}", id - 300);
    }
    format!("CR{:02}", id - 200)
}
