mod events;
mod limits;
mod profile;
mod pv5;
use super::{parameters::setting_int, wire::Values, Error};
use openpilot_params::Params;
pub use profile::{EventType, Profile};

#[derive(Clone, Debug)]
pub struct Event {
    pub id: u64,
    pub kind: EventType,
    pub speed: f64,
    pub subtype: u8,
    pub target: f64,
}

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Output {
    pub speed_limit: f64,
    pub speed_limit_distance: f64,
    pub bump_distance: f64,
    pub school_zone: bool,
    pub active: bool,
    pub section_active: bool,
    pub available: bool,
    pub speed: f64,
}

pub struct Input<'a> {
    pub hda: Option<&'a Values>,
    pub position: Option<&'a Values>,
    pub segment: Option<&'a Values>,
    pub profile: Option<&'a Values>,
    pub status: Option<&'a Values>,
    pub profile_timestamp: u64,
    pub position_timestamp: u64,
    pub segment_timestamp: u64,
    pub hda_timestamp: u64,
    pub status_timestamp: u64,
    pub last_update: u64,
    pub pt_timeout: bool,
    pub alt_timeout: bool,
    pub hda_size: usize,
    pub status_size: usize,
    pub metric: bool,
}

pub struct Navigation {
    pub wrapped: bool,
    pub total_distance: f64,
    pub speed_limit_distance: f64,
    pub distance_time: f64,
    pub mode: i32,
    pub school_control: bool,
    pub params_counter: u32,
    pub events: Vec<Event>,
    pub segment_timestamp: u64,
    pub profile_timestamp: u64,
    pub available: bool,
    pub route_reset_timestamp: u64,
    pub route_state: u8,
    pub route_path: Option<u8>,
    pub road_class: u8,
    pub camera_target: Option<f64>,
    pub camera_event: Option<Event>,
    pub camera_speed: f64,
    pub camera_status_target: Option<f64>,
    pub speed_zone_active: bool,
    pub speed_zone_speed: f64,
    pub school_active: bool,
    pub school_start_distance: f64,
    pub school_uses_status: bool,
    pub pv5_section_previous: bool,
    pub pv5_completed_speed: f64,
    next_event: u64,
}

impl Navigation {
    pub fn new(wrapped: bool, settings: &Params) -> Result<Self, Error> {
        Ok(Self {
            wrapped,
            total_distance: 0.,
            speed_limit_distance: 0.,
            distance_time: f64::from(
                setting_int(settings, "VehicleSpeedCameraDistanceTime")?.clamp(10, 200),
            ) / 10.,
            mode: setting_int(settings, "VehicleNaviCanControl")?.clamp(0, 3),
            school_control: settings.get_bool("VehicleNaviSchoolZoneControl")?,
            params_counter: 0,
            events: Vec::with_capacity(32),
            segment_timestamp: 0,
            profile_timestamp: 0,
            available: false,
            route_reset_timestamp: 0,
            route_state: 0,
            route_path: None,
            road_class: 7,
            camera_target: None,
            camera_event: None,
            camera_speed: 0.,
            camera_status_target: None,
            speed_zone_active: false,
            speed_zone_speed: 0.,
            school_active: false,
            school_start_distance: 0.,
            school_uses_status: false,
            pv5_section_previous: false,
            pv5_completed_speed: 0.,
            next_event: 0,
        })
    }

    pub fn controlled_access(&self, input: &Input<'_>) -> bool {
        let link = input
            .hda
            .and_then(|data| data.get("LinkClass"))
            .copied()
            .unwrap_or(0.);
        [1., 2., 3.].contains(&link) || [1, 2].contains(&self.road_class)
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
        self.camera_target = None;
        self.camera_event = None;
    }
    pub fn clear_school(&mut self) {
        self.school_active = false;
        self.school_start_distance = self.total_distance;
        self.school_uses_status = false;
    }
    pub fn clear_speed_zone(&mut self) {
        self.speed_zone_active = false;
        self.speed_zone_speed = 0.;
    }
    pub fn clear_route_filtered(&mut self) {
        if self.mode < 2 {
            return;
        }
        self.events.retain(|event| match event.kind {
            EventType::Camera => self.mode == 2,
            EventType::Bump => false,
            EventType::SpeedZone => true,
        });
        if self
            .camera_event
            .as_ref()
            .is_some_and(|cached| !self.events.iter().any(|event| event.id == cached.id))
        {
            self.camera_event = None;
            self.camera_target = None;
        }
        if self.mode == 3 {
            self.clear_speed_zone();
        }
    }

    pub fn refresh(&mut self, settings: &Params) -> Result<bool, Error> {
        self.params_counter += 1;
        if self.params_counter < 100 {
            return Ok(false);
        }
        self.params_counter = 0;
        let distance_time =
            f64::from(setting_int(settings, "VehicleSpeedCameraDistanceTime")?.clamp(10, 200))
                / 10.;
        let changed = distance_time != self.distance_time;
        self.distance_time = distance_time;
        if changed && self.camera_status_target.is_some() {
            self.camera_status_target =
                Some(self.total_distance + self.camera_speed * distance_time);
        }
        let mode = setting_int(settings, "VehicleNaviCanControl")?.clamp(0, 3);
        if mode != self.mode {
            self.mode = mode;
            if mode == 0 {
                self.clear_events();
                self.clear_speed_zone();
            } else if mode >= 2 {
                self.clear_route_filtered();
            }
        }
        let school = settings.get_bool("VehicleNaviSchoolZoneControl")?;
        if school != self.school_control {
            self.school_control = school;
            if !school {
                self.clear_school();
            }
        }
        Ok(changed)
    }
}
