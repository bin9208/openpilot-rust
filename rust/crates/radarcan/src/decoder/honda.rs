use crate::{base::Base, data::Data, integer_set::IntegerSet, point::Point, reader::Reader, Error};

#[derive(Default)]
pub struct Honda {
    pub track_id: u64,
    pub radar_fault: bool,
    pub radar_wrong_config: bool,
}

pub fn messages() -> Vec<(u32, f64)> {
    [0x400]
        .into_iter()
        .chain(0x430..0x43a)
        .chain(0x440..0x446)
        .map(|a| (a, 20.))
        .collect()
}

impl Honda {
    pub fn update(
        &mut self,
        base: &mut Base,
        reader: &mut Reader,
        updated: &IntegerSet,
    ) -> Result<Data, Error> {
        let mut addresses = updated.iter().collect::<Vec<_>>();
        addresses.sort_unstable();
        for address in addresses {
            if address == 0x400 {
                let state = reader.signal(address, "RADAR_STATE")?;
                self.radar_fault = state != 0x79 as f64;
                self.radar_wrong_config = state == 0x69 as f64;
            } else if reader.signal(address, "LONG_DIST")? < 255. {
                let id = u64::from(address);
                if !base.pts.contains_key(&id) || reader.signal(address, "NEW_TRACK")? != 0. {
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
                    .ok_or(Error::Contract("Honda point absent"))?;
                point.d_rel = reader.signal(address, "LONG_DIST")? as f32;
                point.y_rel = -reader.signal(address, "LAT_DIST")? as f32;
                point.v_rel = reader.signal(address, "REL_SPEED")? as f32;
                point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
                point.a_rel = f32::NAN;
                point.yv_rel = 0.;
                point.measured = true;
            } else {
                base.pts.shift_remove(&u64::from(address));
            }
        }
        let mut data = Data::default();
        data.errors.can_error = !reader.parser.can_valid();
        data.errors.radar_fault = self.radar_fault;
        data.errors.wrong_config = self.radar_wrong_config;
        data.points_present = true;
        data.points = base.pts.values().cloned().collect();
        Ok(data)
    }
}
