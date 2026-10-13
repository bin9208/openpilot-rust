mod camera;
mod draw;
mod draw_vehicle;
mod projection;
mod radar;
mod scratch;
mod strip;
mod vehicle;
pub use camera::{camera_for_state, CameraOverlayCamera};
pub use projection::{projection, screen_xy, CameraOverlayProjection};
use std::ops::Range;
pub use vehicle::{vehicle_draw_key, vehicle_frame, vehicle_overlay_label, VehicleFrame};

pub struct OverlayState {
    pub z_offset_m: f64,
    pub pitch_offset_deg: f64,
    pub wide: bool,
    pub tuning_visible: bool,
    scratch: scratch::Scratch,
}
impl Default for OverlayState {
    fn default() -> Self {
        Self {
            z_offset_m: 0.,
            pitch_offset_deg: 0.,
            wide: false,
            tuning_visible: std::env::var("CLUSTER_ROUTE_CAMERA_TUNING").as_deref() == Ok("1"),
            scratch: scratch::Scratch::default(),
        }
    }
}
impl OverlayState {
    pub fn close_resources(&mut self) {
        self.scratch = scratch::Scratch::default();
    }
    #[must_use]
    pub fn buffer_capacities(&self) -> (usize, usize, usize) {
        self.scratch.capacities()
    }
}
pub fn visible_runs(pairs: &[u8]) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut cursor = 0;
    std::iter::from_fn(move || {
        while cursor < pairs.len() {
            while cursor < pairs.len() && pairs[cursor] == 0 {
                cursor += 1;
            }
            let start = cursor;
            while cursor < pairs.len() && pairs[cursor] != 0 {
                cursor += 1;
            }
            if cursor - start >= 2 {
                return Some(start..cursor);
            }
        }
        None
    })
}
