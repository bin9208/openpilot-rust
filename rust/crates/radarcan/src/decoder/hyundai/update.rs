use super::{corner::Corner, Hyundai};
use crate::{base::Base, data::Data, Error};
use openpilot_can::Packet;

impl Hyundai {
    pub fn update(&mut self, base: &mut Base, packets: &[Packet]) -> Result<Option<Data>, Error> {
        base.frame += 1;
        if self.radar_off_can
            || (self.bosch.is_none()
                && self.rcp_tracks.is_none()
                && self.rcp_scc.is_none()
                && self.rcp_corner_objects.is_none()
                && self.rcp_corner_objects_180.is_none())
        {
            return Ok(base.fallback());
        }
        if let Some(reader) = &mut self.rcp_scc {
            reader.update(packets, &mut self.updated_scc)?;
        }
        let mut track_ready = if let Some(bosch) = self.bosch.as_mut() {
            bosch.update(base.v_ego, packets, || {
                let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
                time.tv_sec * 1_000_000_000 + time.tv_nsec
            })?
        } else {
            false
        };
        if self.radar_tracks {
            if let Some(reader) = &mut self.rcp_tracks {
                reader.update(packets, &mut self.updated_tracks)?;
                track_ready = self.updated_tracks.contains(self.trigger_msg_tracks);
            }
        }
        let mut corner_ready = false;
        if let Some(reader) = &mut self.rcp_corner_objects {
            reader.update(packets, &mut self.updated_corner_objects)?;
            corner_ready = self
                .updated_corner_objects
                .contains(self.trigger_msg_corner_objects);
        }
        let mut corner_180_ready = false;
        if let Some(reader) = &mut self.rcp_corner_objects_180 {
            reader.update(packets, &mut self.updated_corner_objects_180)?;
            corner_180_ready = self
                .updated_corner_objects_180
                .contains(self.trigger_msg_corner_objects_180);
        }
        let scc_ready = (!self.radar_tracks || self.bosch.is_some())
            && base.frame.is_multiple_of(5)
            && self.rcp_scc.is_some();
        if track_ready && self.rcp_tracks.is_some() {
            self.update_front(base)?;
            self.updated_tracks.clear();
        }
        if corner_ready {
            self.update_corner(base, Corner::Radar235)?;
            self.corner_object_missed_updates = 0;
            self.updated_corner_objects.clear();
        }
        if corner_180_ready {
            self.update_corner(base, Corner::Radar180)?;
            self.corner_object_180_missed_updates = 0;
            self.updated_corner_objects_180.clear();
        }
        let bosch_front_only = self.bosch.is_some() && self.radar_track_mode == 1;
        let publish_ready = if bosch_front_only {
            base.frame.is_multiple_of(5)
        } else if self.bosch.is_some() {
            scc_ready
        } else {
            track_ready || scc_ready
        };
        if !publish_ready {
            return Ok(None);
        }
        if self.rcp_scc.is_some() {
            self.update_scc(base)?;
        }
        if self.rcp_corner_objects.is_some() {
            if !self.updated_corner_objects.is_empty() {
                self.update_corner(base, Corner::Radar235)?;
                self.corner_object_missed_updates = 0;
            } else {
                self.corner_object_missed_updates += 1;
                if self.corner_object_missed_updates > 10 {
                    self.clear_corner(base, Corner::Radar235)?;
                }
            }
        }
        if self.rcp_corner_objects_180.is_some() {
            if !self.updated_corner_objects_180.is_empty() {
                self.update_corner(base, Corner::Radar180)?;
                self.corner_object_180_missed_updates = 0;
            } else {
                self.corner_object_180_missed_updates += 1;
                if self.corner_object_180_missed_updates > 10 {
                    self.clear_corner(base, Corner::Radar180)?;
                }
            }
        }
        self.updated_scc.clear();
        self.updated_corner_objects.clear();
        self.updated_corner_objects_180.clear();
        self.updated_corner_objects_430.clear();
        let mut data = Data::default();
        data.errors.wrong_config = self.bosch.as_ref().is_some_and(|b| b.provider.wrong_config);
        let parser_error = (self.radar_tracks
            && self
                .rcp_tracks
                .as_mut()
                .is_some_and(|reader| !reader.parser.can_valid()))
            || (!bosch_front_only
                && !self.corner_objects_available
                && self
                    .rcp_scc
                    .as_mut()
                    .is_some_and(|reader| !reader.parser.can_valid()))
            || self
                .rcp_corner_objects
                .as_mut()
                .is_some_and(|reader| !reader.parser.can_valid())
            || self
                .rcp_corner_objects_180
                .as_mut()
                .is_some_and(|reader| !reader.parser.can_valid());
        data.errors.can_error =
            self.bosch.as_ref().is_some_and(|b| b.provider.can_error) || parser_error;
        data.points_present = true;
        data.points = base
            .pts
            .values()
            .filter(|point| point.measured)
            .cloned()
            .collect();
        Ok(Some(data))
    }
}
