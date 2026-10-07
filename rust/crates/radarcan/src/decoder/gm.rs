use crate::{base::Base, data::Data, integer_set::IntegerSet, point::Point, reader::Reader, Error};
use std::collections::BTreeSet;

pub fn messages() -> Vec<(u32, f64)> {
    [
        1138, 1136, 1127, 1129, 1140, 1120, 1131, 1133, 1122, 1135, 1126, 1137, 1123, 1124, 1128,
        1139, 1130, 1121, 1132, 1134, 1125,
    ]
    .into_iter()
    .map(|a| (a, 14.))
    .collect()
}

pub fn update(base: &mut Base, reader: &mut Reader, updated: &IntegerSet) -> Result<Data, Error> {
    let mut data = Data::default();
    for signal in [
        "FLRRSnsrBlckd",
        "FLRRSnstvFltPrsntInt",
        "FLRRYawRtPlsblityFlt",
        "FLRRHWFltPrsntInt",
        "FLRRAntTngFltPrsnt",
        "FLRRAlgnFltPrsnt",
    ] {
        if reader.signal(1120, signal)? != 0. {
            data.errors.radar_fault = true;
            break;
        }
    }
    data.errors.can_error = !reader.parser.can_valid();
    let targets = reader.signal(1120, "FLRRNumValidTargets")?;
    let mut current = BTreeSet::new();
    for address in updated.iter() {
        if address == 1120 {
            continue;
        }
        if targets == 0. {
            break;
        }
        let distance = reader.signal(address, "TrkRange")?;
        if distance > 0. {
            let id = reader.signal(address, "TrkObjectID")? as u64;
            current.insert(id);
            let point = base.pts.entry(id).or_insert_with(|| Point {
                track_id: id,
                ..Point::default()
            });
            point.d_rel = distance as f32;
            point.y_rel = ((reader.signal(address, "TrkAzimuth")? * (std::f64::consts::PI / 180.))
                .sin()
                * distance) as f32;
            point.v_rel = reader.signal(address, "TrkRangeRate")? as f32;
            point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
            point.a_rel = f32::NAN;
            point.yv_rel = 0.;
            point.measured = true;
        }
    }
    base.pts.retain(|id, _| current.contains(id));
    data.points_present = true;
    data.points = base.pts.values().cloned().collect();
    Ok(data)
}
