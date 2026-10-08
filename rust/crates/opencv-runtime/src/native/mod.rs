use crate::bridge::ffi;
use crate::{Dimensions, Error, ImageLayout};
use std::sync::OnceLock;
mod image;
mod net;
pub use image::{apply_mask, bgr_to_gray, bounding_rect, nv12_to_rgb, polygon_mask, resize_linear};
pub use net::DnnNet;

static THREADS: OnceLock<(u32, Result<(), String>)> = OnceLock::new();

/// Configure external OpenCV once, before any owner creates a net or processes images.
pub fn initialize(count: u32) -> Result<(), Error> {
    let native =
        i32::try_from(count).map_err(|_| Error::Contract("thread count exceeds OpenCV integer"))?;
    if native == 0 {
        return Err(Error::Contract("thread count must be positive"));
    }
    let (configured, result) = THREADS.get_or_init(|| {
        (
            count,
            ffi::set_threads(native).map_err(|error| error.to_string()),
        )
    });
    if *configured != count {
        return Err(Error::ThreadCount {
            configured: *configured,
            requested: count,
        });
    }
    result
        .as_ref()
        .map_err(|message| Error::Native {
            operation: "setNumThreads",
            message: message.clone(),
        })
        .copied()
}

fn ready() -> Result<(), Error> {
    let Some((_, result)) = THREADS.get() else {
        return Err(Error::Contract("initialize OpenCV before native calls"));
    };
    result
        .as_ref()
        .map_err(|message| Error::Native {
            operation: "setNumThreads",
            message: message.clone(),
        })
        .copied()
}

fn native_error(operation: &'static str, error: cxx::Exception) -> Error {
    Error::Native {
        operation,
        message: error.to_string(),
    }
}

fn size(dimensions: Dimensions) -> ffi::Dimensions {
    ffi::Dimensions {
        width: dimensions.width(),
        height: dimensions.height(),
    }
}

fn layout(value: ImageLayout) -> ffi::Layout {
    let channels = match value.format() {
        crate::Format::Gray => 1,
        crate::Format::Rgb | crate::Format::Bgr => 3,
    };
    ffi::Layout {
        size: size(value.dimensions()),
        channels,
    }
}
