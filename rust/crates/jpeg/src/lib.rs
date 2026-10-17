#[cfg(feature = "native-skip-miri")]
mod bridge;
mod options;
pub use options::{Color, ContractError, Layout, Options, Quality};
pub type Error = cxx::Exception;
#[cfg(feature = "native-skip-miri")]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub components: u32,
    pub pixels: Vec<u8>,
}

/// Decode using libjpeg's integer slow DCT and fancy upsampling. Components are
/// 1 (gray), 3 (RGB), or 4 (CMYK with Pillow's inverted rawmode convention).
#[cfg(feature = "native-skip-miri")]
pub fn decode(bytes: &[u8], max_pixels: u64) -> Result<DecodedImage, Error> {
    let decoded = bridge::ffi::decode(bytes, max_pixels)?;
    Ok(DecodedImage {
        width: decoded.width,
        height: decoded.height,
        components: decoded.components,
        pixels: decoded.pixels,
    })
}
#[cfg(feature = "native-skip-miri")]
pub fn encode(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, Error> {
    bridge::ffi::encode(rgb, width, height)
}

#[cfg(feature = "native-skip-miri")]
#[derive(Debug)]
pub enum EncodeError {
    Contract(ContractError),
    Native(Error),
}

#[cfg(feature = "native-skip-miri")]
impl std::fmt::Display for EncodeError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(error) => std::fmt::Display::fmt(error, output),
            Self::Native(error) => std::fmt::Display::fmt(error, output),
        }
    }
}
#[cfg(feature = "native-skip-miri")]
impl std::error::Error for EncodeError {}

#[cfg(feature = "native-skip-miri")]
pub fn encode_with(
    pixels: &[u8],
    layout: Layout,
    options: Options,
) -> Result<Vec<u8>, EncodeError> {
    layout.check_pixels(pixels).map_err(EncodeError::Contract)?;
    bridge::ffi::encode_with(
        pixels,
        bridge::ffi::Layout {
            width: layout.width(),
            height: layout.height(),
            components: layout.color().components(),
        },
        options.quality().value(),
    )
    .map_err(EncodeError::Native)
}

#[cfg(feature = "native-skip-miri")]
mod turbo;
#[cfg(feature = "native-skip-miri")]
pub use turbo::TurboJpeg;
