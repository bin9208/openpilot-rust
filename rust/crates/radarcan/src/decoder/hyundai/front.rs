use super::{group3::Object, Hyundai};
use crate::{base::Base, point::Point, Error};
use std::collections::BTreeMap;

impl Hyundai {
    pub(super) fn update_front(&mut self, base: &mut Base) -> Result<(), Error> {
        let reader = self
            .rcp_tracks
            .as_mut()
            .ok_or(Error::Contract("Hyundai track reader absent"))?;
        if self.radar_group3 {
            let mut objects = BTreeMap::new();
            for address in self.radar_start_addr..self.radar_start_addr + self.radar_msg_count {
                if self.updated_tracks.contains(address) {
                    objects.insert(
                        address,
                        Object {
                            object_id: reader.signal(address, "OBJECT_ID")? as i64,
                            x: reader.signal(address, "LONG_DIST")?,
                            y: reader.signal(address, "LAT_DIST")?,
                            v: reader.signal(address, "REL_SPEED")?,
                            length: reader.signal(address, "OBJECT_LENGTH")?,
                        },
                    );
                }
            }
            let assignments = self.group3_track_ids.update(&objects)?;
            for slot in 32..32 + self.radar_msg_count {
                base.pts.shift_remove(&u64::from(slot));
            }
            for (address, id) in assignments {
                let object = objects[&address];
                base.pts.insert(
                    u64::from(32 + address - self.radar_start_addr),
                    Point {
                        track_id: id,
                        measured: true,
                        d_rel: object.distance() as f32,
                        y_rel: object.y as f32,
                        v_rel: object.v as f32,
                        v_lead: (base.v_ego + object.v) as f32,
                        a_rel: f32::NAN,
                        ..Point::default()
                    },
                );
            }
            return Ok(());
        }
        for bank in 0..if self.radar_group1 { 2 } else { 1 } {
            for index in 0..self.radar_msg_count {
                let address = self.radar_start_addr + index;
                let optional_stale = address
                    >= self.radar_start_addr + self.radar_required_msg_count
                    && !self.updated_tracks.contains(address);
                let mut track_state = 0;
                let valid = if self.radar_group1 {
                    reader.signal(
                        address,
                        if bank == 0 {
                            "VALID_CNT1"
                        } else {
                            "VALID_CNT2"
                        },
                    )? > 10.
                } else if self.canfd {
                    let valid = reader.signal(address, "VALID_CNT")? > 10.;
                    track_state = reader.signal(address, "VALID")? as u8;
                    valid
                } else if self.radar_group4 {
                    reader.signal(address, "OBJECT_STATE")? == 3.
                        && 0.2 < reader.signal(address, "LONG_DIST")?
                        && reader.signal(address, "LONG_DIST")? < 325.
                        && reader.signal(address, "LAT_DIST")?.abs() <= 6.
                } else {
                    let state = reader.signal(address, "STATE")?;
                    state == 3. || state == 4.
                };
                let id = u64::from(32 + bank * self.radar_msg_count + index);
                let point = base
                    .pts
                    .get_mut(&id)
                    .ok_or(Error::Contract("Hyundai front point absent"))?;
                point.measured = valid && !optional_stale;
                if !point.measured {
                    clear_front(point, base.v_ego);
                } else if self.radar_group1 {
                    let suffix = bank + 1;
                    point.d_rel = reader.signal(address, &format!("LONG_DIST{suffix}"))? as f32;
                    point.y_rel = reader.signal(address, &format!("LAT_DIST{suffix}"))? as f32;
                    point.v_rel = reader.signal(address, &format!("REL_SPEED{suffix}"))? as f32;
                    point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
                    point.a_rel = reader.signal(address, &format!("REL_ACCEL{suffix}"))? as f32;
                    point.yv_rel = reader.signal(address, &format!("LAT_SPEED{suffix}"))? as f32;
                } else if self.canfd {
                    point.d_rel = reader.signal(address, "LONG_DIST")? as f32;
                    point.y_rel = reader.signal(address, "LAT_DIST")? as f32;
                    point.v_rel = reader.signal(address, "REL_SPEED")? as f32;
                    point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
                    point.a_rel = reader.signal(address, "REL_ACCEL")? as f32;
                    point.yv_rel = reader.signal(address, "LAT_SPEED")? as f32;
                    point.track_state = track_state;
                } else if self.radar_group4 {
                    point.d_rel = reader.signal(address, "LONG_DIST")? as f32;
                    point.y_rel = -reader.signal(address, "LAT_DIST")? as f32;
                    point.v_rel = reader.signal(address, "REL_SPEED")? as f32;
                    point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
                    point.a_rel = f32::NAN;
                    point.yv_rel = 0.;
                } else {
                    let azimuth =
                        reader.signal(address, "AZIMUTH")? * (std::f64::consts::PI / 180.);
                    let distance = reader.signal(address, "LONG_DIST")?;
                    point.d_rel = (azimuth.cos() * distance) as f32;
                    point.y_rel = (0.5 * -azimuth.sin() * distance) as f32;
                    point.v_rel = reader.signal(address, "REL_SPEED")? as f32;
                    point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
                    point.a_rel = reader.signal(address, "REL_ACCEL")? as f32;
                    point.yv_rel = 0.;
                }
            }
        }
        Ok(())
    }
}

pub(super) fn clear_front(point: &mut Point, ego: f64) {
    point.d_rel = 0.;
    point.y_rel = 0.;
    point.v_rel = 0.;
    point.v_lead = (f64::from(point.v_rel) + ego) as f32;
    point.a_rel = f32::NAN;
    point.yv_rel = 0.;
}
