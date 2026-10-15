//! Pinned Bosch point writes, including native Float32 projection order.
use super::physical::Object;
use crate::{
    data::Data,
    point::{Point, Source},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, PartialEq)]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
}
fn alias_id(object: &Object, aliases: Option<&BTreeMap<u32, u64>>) -> Result<u64, Error> {
    aliases.map_or(Ok(u64::from(object.physical_track_id)), |values| {
        values
            .get(&object.physical_track_id)
            .copied()
            .ok_or_else(|| Error {
                kind: "KeyError",
                message: object.physical_track_id.to_string(),
            })
    })
}
pub fn fill(
    point: &mut Point,
    object: &Object,
    ego: f64,
    aliases: Option<&BTreeMap<u32, u64>>,
    acceleration: f64,
) -> Result<(), Error> {
    point.track_id = alias_id(object, aliases)?;
    let (_, d, y, v) = object.published();
    point.d_rel = d as f32;
    point.y_rel = y as f32;
    point.v_rel = v as f32;
    point.a_rel = f32::NAN;
    point.yv_rel = f32::NAN;
    point.j_lead = f32::NAN;
    point.a_lead = acceleration as f32;
    point.v_lead = (ego + v) as f32;
    point.radar_source = Source::FrontRadar;
    point.track_state = 0;
    point.measured = true;
    Ok(())
}
pub fn append(
    data: &mut Data,
    objects: &[Object],
    ego: f64,
    now_ns: i64,
    aliases: Option<&BTreeMap<u32, u64>>,
    accelerations: Option<&BTreeMap<u32, f64>>,
) -> Result<(), Error> {
    if objects.is_empty() {
        return Ok(());
    }
    let offset = data.points.len();
    data.points.resize(offset + objects.len(), Point::default());
    data.points_present = true;
    for (point, object) in data.points[offset..].iter_mut().zip(objects) {
        let a = accelerations
            .and_then(|a| a.get(&object.physical_track_id))
            .copied()
            .unwrap_or(f64::NAN);
        fill(point, object, ego, aliases, a)?;
        let representative = if object.members.len() == 1 {
            object.members.first()
        } else {
            let id = object.published().0;
            object.members.iter().find(|m| m.raw_track_id == id)
        }
        .ok_or_else(|| Error {
            kind: "StopIteration",
            message: String::new(),
        })?;
        let age = (now_ns - representative.detection.timestamp_ns) as f64 * 1e-9;
        point.d_rel = (f64::from(point.d_rel) + f64::from(point.v_rel) * age) as f32;
    }
    Ok(())
}
pub fn native(
    objects: &[Object],
    timestamp_ns: i64,
    ego: f64,
    complete: bool,
    unsupported: bool,
) -> Result<Data, Error> {
    if objects.iter().any(|o| o.timestamp_ns != timestamp_ns) {
        return Err(Error {
            kind: "ValueError",
            message: "stale physical observation cannot be serialized as fresh".into(),
        });
    }
    if objects
        .iter()
        .map(|o| o.physical_track_id)
        .collect::<BTreeSet<_>>()
        .len()
        != objects.len()
    {
        return Err(Error {
            kind: "ValueError",
            message: "duplicate physical ID".into(),
        });
    }
    let mut data = Data {
        points: vec![Point::default(); objects.len()],
        points_present: true,
        ..Data::default()
    };
    data.errors.can_error = !complete;
    data.errors.wrong_config = unsupported;
    for (point, object) in data.points.iter_mut().zip(objects) {
        fill(point, object, ego, None, f64::NAN)?;
    }
    Ok(data)
}
pub fn make(objects: &[Object], ego: f64) -> Result<Vec<Point>, Error> {
    objects.first().map_or_else(
        || Ok(Vec::new()),
        |first| native(objects, first.timestamp_ns, ego, true, false).map(|v| v.points),
    )
}
/// The 0x601 word is a geometry probe, never assumed to be a raw record identity.
pub fn match_processed(objects: &[Object], word: Option<u32>) -> BTreeSet<u32> {
    let Some(word) = word.filter(|word| *word != 0x4010_0000 && word & (1 << 31) == 0) else {
        return BTreeSet::new();
    };
    let d = f64::from(word & 0x3ff) * 0.25;
    let y = f64::from((word >> 10) & 0x7ff) * 0.03125 - 32.;
    let v = f64::from((word >> 21) & 0x3ff) * 0.25 - 128.;
    objects
        .iter()
        .filter(|o| {
            (o.d_rel - d).abs() <= 1. && (o.y_rel - y).abs() <= 0.5 && (o.v_rel - v).abs() <= 0.5
        })
        .map(|o| o.physical_track_id)
        .collect()
}
