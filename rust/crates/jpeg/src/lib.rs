#[cfg(feature = "native-skip-miri")]
mod bridge;
pub type Error = cxx::Exception;
#[cfg(feature = "native-skip-miri")]
pub fn encode(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, Error> {
    bridge::ffi::encode(rgb, width, height)
}
