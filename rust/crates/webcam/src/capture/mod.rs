#[expect(
    unsafe_code,
    reason = "CXX declarations isolate external OpenCV capture ownership"
)]
mod bridge;

use crate::{pixels, Error};
use bridge::ffi;

pub struct Capture {
    value: cxx::UniquePtr<ffi::Capture>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Info {
    pub width: f64,
    pub height: f64,
    pub fps: f64,
}

impl Capture {
    /// Open a string camera ID with the unchanged `OpenCV` capture requests.
    ///
    /// # Errors
    /// Returns external `OpenCV` exceptions or an allocation error.
    pub fn path(path: &str) -> Result<Self, Error> {
        Self::owned(ffi::open_path(path)?)
    }

    /// Open a numeric camera ID after the caller selects integer semantics.
    ///
    /// # Errors
    /// Returns external `OpenCV` exceptions or an allocation error.
    pub fn index(index: i32) -> Result<Self, Error> {
        Self::owned(ffi::open_index(index)?)
    }

    fn owned(value: cxx::UniquePtr<ffi::Capture>) -> Result<Self, Error> {
        if value.is_null() {
            return Err(Error::Contract("OpenCV capture allocation failed"));
        }
        Ok(Self { value })
    }

    /// Read actual capture properties.
    ///
    /// # Errors
    /// Returns an external `OpenCV` property-query exception.
    pub fn info(&self) -> Result<Info, Error> {
        let info = ffi::info(&self.value)?;
        Ok(Info {
            width: info.width,
            height: info.height,
            fps: info.fps,
        })
    }

    /// Observe capture ownership without changing it.
    ///
    /// # Errors
    /// Returns an external `OpenCV` ownership-query exception.
    pub fn opened(&self) -> Result<bool, Error> {
        Ok(ffi::opened(&self.value)?)
    }

    /// Read, rotate 180 degrees and convert the next captured BGR frame to NV12.
    ///
    /// # Errors
    /// Returns capture exceptions, invalid BGR extents or scaler errors.
    pub fn read(&mut self) -> Result<Option<Vec<u8>>, Error> {
        let mut frame = ffi::read(self.value.pin_mut())?;
        if frame.data.is_empty() {
            return Ok(None);
        }
        pixels::rotate(&mut frame.data, frame.width, frame.height)?;
        Ok(Some(pixels::nv12(&frame.data, frame.width, frame.height)?))
    }
}
