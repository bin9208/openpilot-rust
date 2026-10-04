use crate::{
    base::Base,
    data::Data,
    point::Point,
    reader::Reader,
    scalar::{maximum, minimum},
    Error,
};
use indexmap::IndexMap;

#[derive(Default)]
pub struct Volkswagen {
    pub track_id_counter: u64,
    pub yv_state: IndexMap<u64, (f64, f64)>,
}

impl Volkswagen {
    pub fn update(&mut self, base: &mut Base, reader: &mut Reader) -> Result<Data, Error> {
        let mut data = Data::default();
        if !reader.parser.can_valid() {
            data.errors.can_error = true;
            return Ok(data);
        }
        data.errors.radar_unavailable_temporary = reader.signal(0x24f, "Distance_Status")? != 0.;
        let mut active = IndexMap::new();
        for lane in ["Same_Lane", "Left_Lane", "Right_Lane"] {
            for index in [1, 2] {
                let prefix = format!("{lane}_0{index}");
                let id = reader.signal(0x24f, &format!("{prefix}_ObjectID"))?;
                if id == 0. {
                    continue;
                }
                let distance = reader.signal(0x24f, &format!("{prefix}_Long_Distance"))?;
                let lateral = reader.signal(0x24f, &format!("{prefix}_Lat_Distance"))?;
                let velocity = reader.signal(0x24f, &format!("{prefix}_Rel_Velo"))?;
                if !(1. < distance && distance < 250.) || lateral.abs() > 10. {
                    continue;
                }
                let id = id as u64;
                if active.contains_key(&id) {
                    data.errors.can_error = true;
                    return Ok(data);
                }
                active.insert(id, (distance, lateral, velocity));
            }
        }
        for (id, (distance, lateral, velocity)) in &active {
            if !base.pts.contains_key(id) {
                base.pts.insert(
                    *id,
                    Point {
                        track_id: self.track_id_counter,
                        ..Point::default()
                    },
                );
                self.track_id_counter += 1;
            }
            let point = base
                .pts
                .get_mut(id)
                .ok_or(Error::Contract("Volkswagen point absent"))?;
            point.measured = true;
            point.d_rel = *distance as f32;
            point.y_rel = *lateral as f32;
            point.v_rel = *velocity as f32;
            point.v_lead = (base.v_ego + velocity) as f32;
            point.a_rel = f32::NAN;
            let yv = if let Some((previous, filtered)) = self.yv_state.get(id) {
                let raw = maximum(-6., minimum(6., (lateral - previous) / 0.04));
                filtered + 0.25 * (raw - filtered)
            } else {
                0.
            };
            self.yv_state.insert(*id, (*lateral, yv));
            point.yv_rel = yv as f32;
        }
        base.pts.retain(|id, _| active.contains_key(id));
        self.yv_state.retain(|id, _| active.contains_key(id));
        data.points_present = true;
        data.points = base.pts.values().cloned().collect();
        Ok(data)
    }
}
