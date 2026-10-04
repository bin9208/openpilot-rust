//! Checked borrowed inputs and owned results for pinned external OpenCV calls.
//! Cropping, normalization, model interpretation and runtime policy remain in Rust callers.

mod error;
mod image;
mod tensor;
pub use error::Error;
pub use image::{Dimensions, Format, Image, ImageLayout, ImageView, Point, Rect};
pub use tensor::{Tensor, TensorView};

#[cfg(feature = "native-skip-miri")]
mod bridge;
#[cfg(feature = "native-skip-miri")]
mod native;
#[cfg(feature = "native-skip-miri")]
pub use native::{
    apply_mask, bgr_to_gray, bounding_rect, initialize, nv12_to_rgb, polygon_mask, resize_linear,
    DnnNet,
};
