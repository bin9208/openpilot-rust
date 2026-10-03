use super::{EventType, Input, Navigation, Profile};
use crate::brands::hyundai::{wire::get, Error};

impl Navigation {
    fn pv5_fresh(input: &Input<'_>) -> bool {
        let hda_age = i128::from(input.last_update) - i128::from(input.hda_timestamp);
        let status_age = i128::from(input.last_update) - i128::from(input.status_timestamp);
        input.hda_timestamp > 0
            && (0..=1_000_000_000).contains(&hda_age)
            && !input.pt_timeout
            && input.hda_size == 16
            && input.status_timestamp > 0
            && (0..=1_000_000_000).contains(&status_age)
            && !input.alt_timeout
            && input.status_size == 24
            && input.hda.is_some()
            && input.status.is_some()
    }

    pub fn pv5_camera_warning(&mut self, input: &Input<'_>) -> Result<bool, Error> {
        if let Some(event) = self.camera_event.clone().map(|cached| {
            self.events
                .iter()
                .find(|event| event.id == cached.id)
                .cloned()
                .unwrap_or(cached)
        }) {
            if event.target <= self.total_distance {
                self.pv5_completed_speed = event.speed;
                self.events.retain(|candidate| candidate.id != event.id);
                self.camera_event = None;
            }
        }
        let fresh = Self::pv5_fresh(input);
        let mut speed = 0.;
        let mut map_warning = false;
        if fresh {
            if let Some(hda) = input.hda {
                let raw = get(hda, "SPEED_LIMIT")?;
                speed = raw * if input.metric { 1. } else { 1.609344 };
                map_warning = get(hda, "MapSource")?.trunc() == 2. && raw > 0. && raw < 255.;
            }
        }
        let previous = if self.camera_speed != 0. {
            self.camera_speed
        } else {
            self.pv5_completed_speed
        };
        if !map_warning || (previous != 0. && speed != previous) {
            self.events.retain(|event| event.kind != EventType::Camera);
            self.camera_event = None;
            self.camera_target = None;
            self.camera_status_target = None;
            self.camera_speed = 0.;
            if let Some(data) = input.profile {
                if Profile::decode(data)?
                    .classify()
                    .is_some_and(|event| event.0 == EventType::Camera)
                {
                    self.profile_timestamp = self.profile_timestamp.max(input.profile_timestamp);
                }
            }
        }
        if !fresh {
            return Ok(false);
        }
        if !map_warning || speed != self.pv5_completed_speed {
            self.pv5_completed_speed = 0.;
        }
        Ok(map_warning && self.pv5_completed_speed == 0.)
    }

    pub fn pv5_section(&mut self, input: &Input<'_>) -> Result<(), Error> {
        if !Self::pv5_fresh(input) {
            self.clear_speed_zone();
            self.pv5_section_previous = true;
            return Ok(());
        }
        let status = input
            .status
            .ok_or_else(|| Error::Signal("PV5 status".into()))?;
        let hda = input.hda.ok_or_else(|| Error::Signal("PV5 HDA".into()))?;
        let start = get(status, "SECTION_ALERT")? != 0.;
        let rising = start && !self.pv5_section_previous;
        self.pv5_section_previous = start;
        let speed = get(status, "SPEED_LIMIT")?.trunc();
        let valid = speed > 30.
            && speed <= 150.
            && speed % 5. == 0.
            && speed == get(hda, "SPEED_LIMIT")?.trunc()
            && get(hda, "MapSource")?.trunc() == 2.;
        if self.mode == 0 || !valid {
            self.clear_speed_zone();
            return Ok(());
        }
        let speed = speed * if input.metric { 1. } else { 1.609344 };
        if self.speed_zone_active && speed != self.speed_zone_speed {
            self.clear_speed_zone();
        }
        if rising {
            self.speed_zone_active = true;
            self.speed_zone_speed = speed;
        }
        Ok(())
    }
}
