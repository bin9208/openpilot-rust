use crate::{base::Base, data::Data, integer_set::IntegerSet, point::Point, reader::Reader, Error};

pub fn messages() -> Vec<(u32, f64)> {
    (0x2c2..=0x2d4)
        .step_by(2)
        .chain((0x2a2..=0x2b4).step_by(2))
        .map(|a| (a, 20.))
        .collect()
}

pub fn update(base: &mut Base, reader: &mut Reader, updated: &IntegerSet) -> Result<Data, Error> {
    let mut data = Data::default();
    data.errors.can_error = !reader.parser.can_valid();
    for address in updated.iter() {
        let longitudinal = address >= 0x2c2;
        let id = u64::from((address - if longitudinal { 0x2c2 } else { 0x2a2 }) / 2);
        let point = base.pts.entry(id).or_insert_with(|| Point {
            track_id: id,
            a_rel: f32::NAN,
            measured: true,
            ..Point::default()
        });
        if longitudinal {
            point.d_rel = reader.signal(address, "LONG_DIST")? as f32;
            point.y_rel = reader.signal(address, "LAT_DIST")? as f32;
        } else {
            point.v_rel = reader.signal(address, "REL_SPEED")? as f32;
            point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
        }
    }
    data.points_present = true;
    data.points = base
        .pts
        .values()
        .filter(|p| p.d_rel != 0.)
        .cloned()
        .collect();
    Ok(data)
}
