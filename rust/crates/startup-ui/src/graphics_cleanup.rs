//! Process-local HUD cleanup for the exact packaged comma raylib provider.
use crate::{bridge::ffi, Error};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
const PROVIDER: &str = "f62d902f04c9b329d1a8f0741e9ca83dd7c3b92beddaea4a3ec393b1f4bafa44";
/// Return free glibc pages after initialization/teardown, outside the frame loop.
pub fn trim_unused_heap() {
    ffi::hud_trim_heap();
}
/// Must run in the HUD process before the first renderer opens.
pub fn install(provider: &Path, adapter: &Path, owns_fd: bool) -> Result<(), Error> {
    let mut hash = Sha256::new();
    let mut file = File::open(provider)?;
    let mut buffer = [0; 65536];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    if provider.file_name() != Some(std::ffi::OsStr::new("libopenpilot-raylib.so"))
        || format!("{:x}", hash.finalize()) != PROVIDER
    {
        return Err(Error::Contract("HUD GBM cleanup provider identity differs"));
    }
    ffi::hud_graphics_cleanup(
        adapter
            .to_str()
            .ok_or(Error::Contract("HUD adapter path is not UTF-8"))?,
        owns_fd,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn unknown_provider_is_rejected_before_loading_an_adapter() {
        let dir = tempfile::tempdir().unwrap();
        let provider = dir.path().join("libopenpilot-raylib.so");
        std::fs::write(&provider, b"unknown").unwrap();
        assert!(
            super::install(&provider, &dir.path().join("absent.so"), false)
                .unwrap_err()
                .to_string()
                .contains("provider identity")
        );
    }
}
