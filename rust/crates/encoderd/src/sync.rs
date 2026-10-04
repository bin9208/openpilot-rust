use crate::config::Camera;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Synchronization {
    pub max_waiting: i32,
    pub encoders_ready: i32,
    pub start_frame_id: u32,
    pub ready: [bool; 3],
    pub synced: [bool; 3],
}
impl Synchronization {
    pub fn new(max_waiting: i32) -> Self {
        Self {
            max_waiting,
            encoders_ready: 0,
            start_frame_id: 0,
            ready: [false; 3],
            synced: [false; 3],
        }
    }
    pub fn frame(&mut self, camera: Camera, frame: u32) -> SyncResult {
        let index = camera.index();
        if self.synced[index] {
            return SyncResult {
                encode: true,
                log: None,
            };
        }
        if self.max_waiting > 1 && self.encoders_ready != self.max_waiting {
            self.start_frame_id = self.start_frame_id.max(frame.wrapping_add(2));
            let log = if self.ready[index] {
                None
            } else {
                self.ready[index] = true;
                self.encoders_ready += 1;
                Some(format!("camera {index} encoder ready"))
            };
            SyncResult { encode: false, log }
        } else {
            if self.max_waiting == 1 {
                self.start_frame_id = self.start_frame_id.max(frame);
            }
            self.synced[index] = frame >= self.start_frame_id;
            SyncResult {
                encode: self.synced[index],
                log: (!self.synced[index]).then(|| {
                    format!(
                        "camera {index} waiting for frame {}, cur {}",
                        self.start_frame_id as i32, frame as i32
                    )
                }),
            }
        }
    }
}
#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub encode: bool,
    pub log: Option<String>,
}
