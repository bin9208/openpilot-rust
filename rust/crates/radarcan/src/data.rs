use crate::point::{Point, Source};
use serde::Serialize;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Errors {
    pub can_error: bool,
    pub radar_fault: bool,
    pub wrong_config: bool,
    pub radar_unavailable_temporary: bool,
}

impl Errors {
    pub fn any(&self) -> bool {
        self.can_error || self.radar_fault || self.wrong_config || self.radar_unavailable_temporary
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub points: Vec<Point>,
    #[serde(skip)]
    pub points_present: bool,
    pub errors: Errors,
    pub radar_track_flipped: bool,
    #[serde(rename = "canMonoTimesDEPRECATED")]
    pub deprecated_times: Vec<u64>,
    #[serde(rename = "errorsDEPRECATED")]
    pub deprecated_errors: Vec<String>,
}

impl Data {
    pub fn set_flip(&mut self, flip: bool) {
        if self.radar_track_flipped != flip {
            for point in &mut self.points {
                if point.radar_source == Source::FrontRadar {
                    point.y_rel = -point.y_rel;
                    point.yv_rel = -point.yv_rel;
                }
            }
        }
        self.radar_track_flipped = flip;
    }
}
