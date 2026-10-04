mod cluster;

use crate::{
    base::Base, data::Data, integer_set::IntegerSet, numerics::Numerics, point::Point,
    reader::Reader, Error,
};
use indexmap::IndexMap;
use serde::Serialize;

pub enum Radar {
    Esr,
    Mrr,
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::Ford;

    #[test]
    fn unavailable_esr_metadata_does_not_initialize_active_radar_counters() {
        let state = Ford::new(Some("ford_fusion_2018_adas"), true).unwrap();
        assert!(
            state.valid_cnt.is_none(),
            "source unavailable ESR has no active counters; native length={:?}",
            state.valid_cnt.as_ref().map(indexmap::IndexMap::len)
        );
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub track_id: u64,
}

pub struct Ford {
    pub radar: Radar,
    pub track_id: u64,
    pub valid_cnt: Option<IndexMap<u32, u64>>,
    pub points: Vec<[f64; 3]>,
    pub clusters: Vec<Cluster>,
    pub scan_index_invalid_cnt: u64,
    pub radar_unavailable_cnt: u64,
    pub prev_header_scan_index: u8,
}

impl Ford {
    pub fn new(name: Option<&str>, unavailable: bool) -> Result<Self, Error> {
        let radar = match name {
            Some("ford_fusion_2018_adas") => Radar::Esr,
            Some("FORD_CADS") => Radar::Mrr,
            _ if unavailable => Radar::Disabled,
            _ => return Err(Error::UnsupportedRadar(name.map(str::to_owned))),
        };
        let valid_cnt = (!unavailable && matches!(radar, Radar::Esr))
            .then(|| (0x500..0x540).map(|a| (a, 0)).collect());
        Ok(Self {
            radar,
            track_id: 0,
            valid_cnt,
            points: Vec::new(),
            clusters: Vec::new(),
            scan_index_invalid_cnt: 0,
            radar_unavailable_cnt: 0,
            prev_header_scan_index: 0,
        })
    }

    pub fn messages(&self) -> Vec<(u32, f64)> {
        match self.radar {
            Radar::Esr => (0x500..0x540).map(|a| (a, 20.)).collect(),
            Radar::Mrr => [0x170, 0x174]
                .into_iter()
                .chain(0x120..0x160)
                .map(|a| (a, 33.))
                .collect(),
            Radar::Disabled => Vec::new(),
        }
    }

    pub fn trigger(&self) -> u32 {
        match self.radar {
            Radar::Esr => 0x53f,
            Radar::Mrr => 0x174,
            Radar::Disabled => 0,
        }
    }

    pub fn update(
        &mut self,
        base: &mut Base,
        reader: &mut Reader,
        updated: &mut IntegerSet,
        numerics: &Numerics,
    ) -> Result<Option<Data>, Error> {
        updated.clear();
        let mut data = Data::default();
        data.errors.can_error = !reader.parser.can_valid();
        match self.radar {
            Radar::Esr => self.update_esr(base, reader, updated)?,
            Radar::Mrr => {
                if !self.update_mrr(base, reader, &mut data, numerics)? {
                    return Ok(None);
                }
            }
            Radar::Disabled => {
                return Err(Error::Contract("disabled Ford radar unexpectedly active"))
            }
        }
        data.points_present = true;
        data.points = base.pts.values().cloned().collect();
        Ok(Some(data))
    }

    fn update_esr(
        &mut self,
        base: &mut Base,
        reader: &mut Reader,
        updated: &IntegerSet,
    ) -> Result<(), Error> {
        let mut addresses = updated.iter().collect::<Vec<_>>();
        addresses.sort_unstable();
        for address in addresses {
            let distance = reader.signal(address, "X_Rel")?;
            let count = self
                .valid_cnt
                .as_mut()
                .and_then(|c| c.get_mut(&address))
                .ok_or(Error::Contract("ESR counter absent"))?;
            if distance > 0.00001 {
                *count = 0;
            }
            if distance > 0.00001 {
                *count += 1;
            } else {
                *count = count.saturating_sub(1);
            }
            let id = u64::from(address);
            if *count > 0 {
                if !base.pts.contains_key(&id) {
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
                    .ok_or(Error::Contract("ESR point absent"))?;
                point.d_rel = distance as f32;
                point.y_rel = (distance
                    * reader.signal(address, "Angle")?
                    * (std::f64::consts::PI / 180.)) as f32;
                point.v_rel = reader.signal(address, "V_Rel")? as f32;
                point.v_lead = (f64::from(point.v_rel) + base.v_ego) as f32;
                point.a_rel = f32::NAN;
                point.yv_rel = 0.;
                point.measured = true;
            } else {
                base.pts.shift_remove(&id);
            }
        }
        Ok(())
    }

    fn update_mrr(
        &mut self,
        base: &mut Base,
        reader: &mut Reader,
        data: &mut Data,
        numerics: &Numerics,
    ) -> Result<bool, Error> {
        let scan = (reader.signal(0x170, "CAN_SCAN_INDEX")? as u8) & 3;
        if (self.prev_header_scan_index + 1) % 4 != scan {
            self.radar_unavailable_cnt += 1;
        } else {
            self.radar_unavailable_cnt = 0;
        }
        self.prev_header_scan_index = scan;
        if self.radar_unavailable_cnt >= 5 {
            base.pts.clear();
            self.points.clear();
            self.clusters.clear();
            data.errors.radar_unavailable_temporary = true;
            return Ok(true);
        }
        if scan != 2 && scan != 3 {
            return Ok(false);
        }
        let coverage = if scan == 2 { 45 } else { 175 };
        if reader.signal(0x174, "CAN_RANGE_COVERAGE")? as i64 != coverage {
            self.scan_index_invalid_cnt += 1;
        } else {
            self.scan_index_invalid_cnt = 0;
        }
        if self.scan_index_invalid_cnt >= 5 {
            data.errors.wrong_config = true;
        }
        for index in 1..=64 {
            let address = 0x11f + index;
            let scan_index = reader.signal(address, &format!("CAN_SCAN_INDEX_2LSB_{index:02}"))?;
            if scan_index != f64::from(scan) {
                continue;
            }
            let mut valid =
                reader.signal(address, &format!("CAN_DET_VALID_LEVEL_{index:02}"))? != 0.;
            let distance = reader.signal(address, &format!("CAN_DET_RANGE_{index:02}"))?;
            if (scan_index == 1. || scan_index == 3.) && distance < 30. {
                valid = false;
            }
            if valid {
                let azimuth = reader.signal(address, &format!("CAN_DET_AZIMUTH_{index:02}"))?;
                let velocity = reader.signal(address, &format!("CAN_DET_RANGE_RATE_{index:02}"))?;
                self.points.push([
                    azimuth.cos() * distance,
                    (-azimuth.sin() * distance) * 2.,
                    velocity * 2.,
                ]);
            }
        }
        if scan != 3 {
            return Ok(false);
        }
        self.publish(base, numerics)?;
        Ok(true)
    }
}
