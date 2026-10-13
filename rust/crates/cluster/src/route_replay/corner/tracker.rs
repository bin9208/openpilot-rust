use super::{probability, sorted_points, RawObject};
use crate::{models::RadarPoint, route_replay::Error};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Track {
    object: RawObject,
    hits: i64,
}

#[derive(Default)]
pub struct StableTracker {
    tracks: BTreeMap<i64, Track>,
    next_id: i64,
}
impl StableTracker {
    /// # Errors
    /// Returns an error if the native track or observation counter overflows.
    pub fn update(&mut self, object: &RawObject) -> Result<(), Error> {
        self.expire(object.t);
        if let Some(id) = self.match_id(object) {
            let track = self
                .tracks
                .get_mut(&id)
                .ok_or_else(|| Error::Invalid("matched corner track disappeared".into()))?;
            let dt = (object.t - track.object.t).max(0.0);
            let x = (track.object.x + track.object.vx * dt) * (1.0 - 0.62) + object.x * 0.62;
            let y = (track.object.y + track.object.vy * dt) * (1.0 - 0.62) + object.y * 0.62;
            let vx = track.object.vx * (1.0 - 0.45) + object.vx * 0.45;
            let vy = track.object.vy * (1.0 - 0.45) + object.vy * 0.45;
            track.object = RawObject {
                x,
                y,
                vx,
                vy,
                ..object.clone()
            };
            track.hits = track
                .hits
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("corner hit counter overflow".into()))?;
        } else {
            let id = self.next_id;
            self.next_id = self
                .next_id
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("corner track counter overflow".into()))?;
            self.tracks.insert(
                id,
                Track {
                    object: object.clone(),
                    hits: 1,
                },
            );
        }
        Ok(())
    }
    fn match_id(&self, object: &RawObject) -> Option<i64> {
        let mut same = None;
        let mut same_cost = 5.5;
        let mut fallback = None;
        let mut fallback_cost = 5.5;
        for (id, track) in &self.tracks {
            let previous = &track.object;
            let dt = (object.t - previous.t).max(0.0);
            if dt > 0.55 {
                continue;
            }
            let dx = (object.x - (previous.x + previous.vx * dt)).abs();
            let dy = (object.y - (previous.y + previous.vy * dt)).abs();
            if dx > 7.0 || dy > 3.2 {
                continue;
            }
            let dv = (object.vx - previous.vx).abs();
            let penalty = if object.age >= previous.age || previous.age == 255 {
                0.0
            } else {
                0.8
            };
            let cost = dx * 0.55 + dy * 1.05 + dv * 0.12 + penalty;
            if object.object_id == previous.object_id {
                if cost < same_cost {
                    same_cost = cost;
                    same = Some(*id);
                }
            } else if cost + 3.5 < fallback_cost {
                fallback_cost = cost + 3.5;
                fallback = Some(*id);
            }
        }
        same.or(fallback)
    }
    fn expire(&mut self, t: f64) {
        self.tracks.retain(|_, track| t - track.object.t <= 0.55);
    }
    pub fn points_at(&mut self, t: f64, speed: f64) -> Vec<RadarPoint> {
        self.expire(t);
        let visible: Vec<_> = self
            .tracks
            .iter()
            .filter_map(|(id, track)| {
                if track.hits < 4 {
                    return None;
                }
                let object = &track.object;
                let dt = (t - object.t).max(0.0);
                let x = object.x + object.vx * dt;
                let y = object.y + object.vy * dt;
                if (y.abs() > 8.0 && track.hits < 12)
                    || !(0.0..=180.0).contains(&x)
                    || y.abs() > 12.0
                {
                    return None;
                }
                Some((*id, track, x, y))
            })
            .collect();
        let mut counts = BTreeMap::<u16, usize>::new();
        for (_, track, _, _) in &visible {
            *counts.entry(track.object.object_id).or_default() += 1;
        }
        sorted_points(visible.into_iter().map(|(id, track, x, y)| {
            let object = &track.object;
            let label = if counts.get(&object.object_id) == Some(&1) {
                format!("CO{:03}", object.object_id)
            } else {
                format!("CO{:03}_{id:02}", object.object_id)
            };
            RadarPoint {
                label,
                longitudinal_m: x,
                lateral_m: -y,
                source: "cornerRadar".into(),
                relative_speed_mps: Some(object.vx),
                absolute_speed_kph: Some(speed + object.vx * 3.6),
                lateral_speed_mps: Some(-object.vy),
                relative_accel_mps2: Some(object.ax),
                probability: Some(probability(object.quality)),
                valid: Some(1),
                valid_count: Some(track.hits),
                ..RadarPoint::default()
            }
        }))
    }
}
