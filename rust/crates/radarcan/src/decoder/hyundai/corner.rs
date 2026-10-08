use super::{
    identity::{deduplicate, position_valid, Candidate},
    Hyundai,
};
use crate::{
    base::Base,
    point::{Point, Source},
    Error,
};

#[derive(Clone, Copy)]
pub(super) enum Corner {
    Radar235,
    Radar180,
}

struct Layout {
    first: u32,
    messages: u32,
    slots: u32,
    offset: u64,
    source: Source,
}

impl Corner {
    fn layout(self) -> Layout {
        match self {
            Self::Radar235 => Layout {
                first: 0x235,
                messages: 20,
                slots: 1,
                offset: 200,
                source: Source::Corner235,
            },
            Self::Radar180 => Layout {
                first: 0x180,
                messages: 5,
                slots: 2,
                offset: 240,
                source: Source::Corner180,
            },
        }
    }
}

impl Hyundai {
    pub(super) fn update_corner(&mut self, base: &mut Base, corner: Corner) -> Result<(), Error> {
        let layout = corner.layout();
        let (reader, updated) = match corner {
            Corner::Radar235 => (&mut self.rcp_corner_objects, &self.updated_corner_objects),
            Corner::Radar180 => (
                &mut self.rcp_corner_objects_180,
                &self.updated_corner_objects_180,
            ),
        };
        let Some(reader) = reader else {
            return Ok(());
        };
        if updated.is_empty() {
            return self.clear_corner(base, corner);
        }
        let mut candidates = Vec::new();
        for index in 0..layout.messages {
            let address = layout.first + index;
            for slot in 0..layout.slots {
                let prefix = match corner {
                    Corner::Radar235 => "OBJ_".to_owned(),
                    Corner::Radar180 => format!("SLOT{}_", slot + 1),
                };
                let distance = reader.signal(address, &format!("{prefix}REL_POS_X"))?;
                let lateral = reader.signal(address, &format!("{prefix}REL_POS_Y"))?;
                let velocity = reader.signal(address, &format!("{prefix}REL_VEL_X"))?;
                let lateral_velocity = reader.signal(address, &format!("{prefix}REL_VEL_Y"))?;
                let acceleration = reader.signal(address, &format!("{prefix}REL_ACCEL_X"))?;
                let quality = reader.signal(address, &format!("{prefix}QUAL_LEVEL"))?;
                if quality > 0. && position_valid(distance, lateral) && velocity > -99. {
                    candidates.push(Candidate {
                        slot: layout.offset + u64::from(index * layout.slots + slot),
                        object_id: reader.signal(address, &format!("{prefix}OBJECT_ID"))? as i64,
                        age: reader.signal(address, &format!("{prefix}AGE"))? as i64,
                        quality: quality as i64,
                        distance,
                        lateral,
                        velocity,
                        lateral_velocity,
                        acceleration,
                    });
                }
            }
        }
        for slot in layout.offset..layout.offset + u64::from(layout.messages * layout.slots) {
            clear_point(
                base.pts
                    .get_mut(&slot)
                    .ok_or(Error::Contract("Hyundai corner point absent"))?,
                base.v_ego,
            );
        }
        let candidates = deduplicate(&candidates);
        let ids = self
            .corner_object_track_ids
            .assign(layout.source, &candidates)?;
        for candidate in candidates {
            let point = base
                .pts
                .get_mut(&candidate.slot)
                .ok_or(Error::Contract("Hyundai corner point absent"))?;
            point.measured = true;
            point.track_id = ids[&candidate.slot];
            point.radar_source = layout.source;
            point.d_rel = candidate.distance as f32;
            point.y_rel = candidate.lateral as f32;
            point.v_rel = candidate.velocity as f32;
            point.v_lead = (candidate.velocity + base.v_ego) as f32;
            point.a_rel = candidate.acceleration as f32;
            point.yv_rel = candidate.lateral_velocity as f32;
        }
        Ok(())
    }

    pub(super) fn clear_corner(&mut self, base: &mut Base, corner: Corner) -> Result<(), Error> {
        let layout = corner.layout();
        for slot in layout.offset..layout.offset + u64::from(layout.messages * layout.slots) {
            clear_point(
                base.pts
                    .get_mut(&slot)
                    .ok_or(Error::Contract("Hyundai corner point absent"))?,
                base.v_ego,
            );
        }
        self.corner_object_track_ids.clear_source(layout.source);
        Ok(())
    }
}

fn clear_point(point: &mut Point, ego: f64) {
    point.measured = false;
    point.d_rel = 0.;
    point.y_rel = 0.;
    point.v_rel = 0.;
    point.v_lead = ego as f32;
    point.a_rel = f32::NAN;
    point.yv_rel = 0.;
}
