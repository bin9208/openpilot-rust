use crate::Error;
use openpilot_modeld::camera::{CameraSource, Captured, FrameMeta};
use openpilot_msgq::{VisionClient, VisionMetadata};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct Image {
    pub layout: VisionMetadata,
    pub bytes: Vec<u8>,
}

pub struct Source {
    pub client: VisionClient,
    pub stop: Arc<AtomicBool>,
}

impl CameraSource for Source {
    type Buffer = Image;
    type Error = Error;

    fn receive(&mut self) -> Result<Option<Captured<Image>>, Error> {
        if self.stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        if !self.client.is_connected() {
            std::thread::sleep(Duration::from_millis(100));
            return Ok(None);
        }
        let Some(frame) = self.client.receive(Duration::from_millis(100))? else {
            return Ok(None);
        };
        let layout = *frame.metadata();
        let mut bytes = vec![0; layout.len];
        frame.copy_into(&mut bytes)?;
        Ok(Some(Captured {
            metadata: FrameMeta {
                frame_id: layout.frame_id,
                timestamp_sof: layout.timestamp_sof,
                timestamp_eof: layout.timestamp_eof,
            },
            buffer: Image { layout, bytes },
        }))
    }
}
