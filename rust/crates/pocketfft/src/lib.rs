//! Safe CXX ownership around the source NumPy revision's external PocketFFT kernel.
#[cfg(feature = "native-skip-miri")]
mod bridge;
#[cfg(feature = "native-skip-miri")]
use bridge::ffi;
#[cfg(feature = "native-skip-miri")]
pub use ffi::Complex;
#[cfg(feature = "native-skip-miri")]
pub struct Transform(cxx::UniquePtr<ffi::Plan>);
#[cfg(feature = "native-skip-miri")]
impl Transform {
    pub fn new(size: usize) -> Result<Self, cxx::Exception> {
        Ok(Self(ffi::plan(size)?))
    }
    pub fn execute(
        &mut self,
        data: &mut [Complex],
        scale: f64,
        forward: bool,
    ) -> Result<(), cxx::Exception> {
        self.0.pin_mut().transform(data, scale, forward)
    }
}
