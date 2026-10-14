use crate::Error;
use openpilot_modeld::camera::{CameraSource, Captured, FrameMeta};
use openpilot_msgq::{VisionClient, VisionMetadata};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct Image {
    pub layout: VisionMetadata,
    pub bytes: Vec<u8>,
    spare: Rc<RefCell<Spare>>,
}

#[derive(Default)]
struct Spare {
    len: usize,
    bytes: Option<Vec<u8>>,
}

impl Drop for Image {
    fn drop(&mut self) {
        // A source may already have gone away or changed layouts while this snapshot was retained.
        // Keep at most one spare and never panic while releasing an image.
        if let Ok(mut spare) = self.spare.try_borrow_mut() {
            if self.bytes.len() == spare.len && spare.bytes.is_none() {
                spare.bytes = Some(std::mem::take(&mut self.bytes));
            }
        }
    }
}

pub struct Source {
    pub client: VisionClient,
    pub stop: Arc<AtomicBool>,
    spare: Rc<RefCell<Spare>>,
}

impl Source {
    pub fn new(client: VisionClient, stop: Arc<AtomicBool>) -> Self {
        Self {
            client,
            stop,
            spare: Rc::default(),
        }
    }
}

impl CameraSource for Source {
    type Buffer = Image;
    type Error = Error;

    fn receive(&mut self) -> Result<Option<Captured<Image>>, Error> {
        if self.stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        if !self.client.is_connected() {
            return Err(Error::Contract("camera generation lost; restart modeld"));
        }
        let Some(frame) = self.client.receive(Duration::from_millis(100))? else {
            if !self.stop.load(Ordering::Relaxed) && !self.client.is_connected() {
                return Err(Error::Contract("camera generation lost; restart modeld"));
            }
            return Ok(None);
        };
        let layout = *frame.metadata();
        let mut bytes = {
            let mut spare = self.spare.borrow_mut();
            if spare.len != layout.len {
                spare.len = layout.len;
                spare.bytes = None;
            }
            spare.bytes.take().unwrap_or_else(|| vec![0; layout.len])
        };
        frame.copy_into(&mut bytes)?;
        Ok(Some(Captured {
            metadata: FrameMeta {
                frame_id: layout.frame_id,
                timestamp_sof: layout.timestamp_sof,
                timestamp_eof: layout.timestamp_eof,
            },
            buffer: Image {
                layout,
                bytes,
                spare: Rc::clone(&self.spare),
            },
        }))
    }
}
