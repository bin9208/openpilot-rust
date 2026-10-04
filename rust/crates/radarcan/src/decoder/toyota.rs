use crate::{base::Base, data::Data, integer_set::IntegerSet, point::Point, reader::Reader, Error};
use indexmap::IndexMap;

pub struct Toyota {
    pub track_id: u64,
    pub valid_cnt: IndexMap<u32, u64>,
    pub first: u32,
}

impl Toyota {
    pub fn new(tss2: bool) -> Self {
        let first = if tss2 { 0x180 } else { 0x210 };
        Self {
            track_id: 0,
            valid_cnt: (first..first + 16).map(|a| (a, 0)).collect(),
            first,
        }
    }

    pub fn messages(&self) -> Vec<(u32, f64)> {
        (self.first..self.first + 32).map(|a| (a, 20.)).collect()
    }

    pub fn update(
        &mut self,
        base: &mut Base,
        reader: &mut Reader,
        updated: &IntegerSet,
    ) -> Result<Data, Error> {
        let mut data = Data::default();
        data.errors.can_error = !reader.parser.can_valid();
        let mut addresses = updated.iter().collect::<Vec<_>>();
        addresses.sort_unstable();
        for address in addresses {
            let Some(count) = self.valid_cnt.get_mut(&address) else {
                continue;
            };
            let distance = reader.signal(address, "LONG_DIST")?;
            let new = reader.signal(address, "NEW_TRACK")? != 0.;
            let valid = reader.signal(address, "VALID")? != 0.;
            if distance >= 255. || new {
                *count = 0;
            }
            if valid && distance < 255. {
                *count += 1;
            } else {
                *count = count.saturating_sub(1);
            }
            let score = reader.signal(address + 16, "SCORE")?;
            let id = u64::from(address);
            if valid || (score > 50. && distance < 255. && *count > 0) {
                if !base.pts.contains_key(&id) || new {
                    base.pts.insert(
                        id,
                        Point {
                            track_id: self.track_id,
                            ..Point::default()
                        },
                    );
                    self.track_id += 1;
                }
                let point = base
                    .pts
                    .get_mut(&id)
                    .ok_or(Error::Contract("Toyota point absent"))?;
                point.d_rel = distance as f32;
                point.y_rel = -reader.signal(address, "LAT_DIST")? as f32;
                point.v_rel = reader.signal(address, "REL_SPEED")? as f32;
                point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
                point.a_rel = f32::NAN;
                point.yv_rel = 0.;
                point.measured = valid;
            } else {
                base.pts.shift_remove(&id);
            }
        }
        data.points_present = true;
        data.points = base.pts.values().cloned().collect();
        Ok(data)
    }
}
