#[cfg(feature = "native-skip-miri")]
mod bridge;
mod options;
pub use options::{Color, ContractError, Layout, Options, Quality};
pub type Error = cxx::Exception;
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
