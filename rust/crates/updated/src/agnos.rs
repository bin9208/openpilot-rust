use crate::Error;
use std::path::Path;
/// Project-owned flashing is supplied by the native AGNOS port (#119).
/// Callers never fall back to the original Python runtime.
pub trait Agnos {
    fn get_target_slot_number(&mut self) -> Result<u32, Error>;
    fn flash_agnos_update(&mut self, manifest: &Path, target_slot: u32) -> Result<(), Error>;
}
pub struct NotLinked;
impl Agnos for NotLinked {
    fn get_target_slot_number(&mut self) -> Result<u32, Error> {
        Err(Error::AgnosUnavailable)
    }
    fn flash_agnos_update(&mut self, _manifest: &Path, _target_slot: u32) -> Result<(), Error> {
        Err(Error::AgnosUnavailable)
    }
}
