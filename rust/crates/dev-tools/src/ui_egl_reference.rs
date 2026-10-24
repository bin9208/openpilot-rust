//! Independent Rust port of the original fixed EGL operation/descriptor policy.
use crate::seal::Result;
use openpilot_startup_ui::egl_reference_abi::{close_descriptor, Core, Extension, RawApi};
use std::{
    fs,
    io::Write,
    os::fd::{AsRawFd, OwnedFd},
};
// This guard owns the duplicate, including the original policy's explicit
// recovery when a foreign destroy function has already closed that descriptor.
struct Descriptor(Option<OwnedFd>);
impl Descriptor {
    fn fd(&self) -> Result<i32> {
        Ok(self
            .0
            .as_ref()
            .ok_or("missing reference descriptor ownership")?
            .as_raw_fd())
    }
    fn release(&mut self) -> std::io::Result<()> {
        match self.0.take() {
            Some(fd) => close_descriptor(fd),
            None => Ok(()),
        }
    }
}
impl Drop for Descriptor {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
fn descriptors() -> Result<usize> {
    Ok(fs::read_dir("/proc/self/fd")?
        .collect::<std::io::Result<Vec<_>>>()?
        .len())
}
fn initialize_display(api: &mut RawApi, display: &mut u64, force: bool) -> Result<bool> {
    let current = api.display()?;
    if current == 0 {
        return Ok(false);
    }
    if !force && *display == current {
        return Ok(true);
    }
    if !api.initialize(current)? {
        api.error()?;
        return Ok(false);
    }
    *display = current;
    api.extensions(current)?;
    Ok(true)
}
fn initialize(api: &mut RawApi, display: &mut u64) -> Result<bool> {
    api.load_egl()?;
    api.load_gles()?;
    for symbol in [
        Core::Current,
        Core::Initialize,
        Core::Query,
        Core::Proc,
        Core::Error,
        Core::Bind,
        Core::Active,
    ] {
        api.resolve_core(symbol)?;
    }
    if !initialize_display(api, display, true)? {
        return Ok(false);
    }
    for symbol in [
        Extension::Create,
        Extension::Destroy,
        Extension::ImageTarget,
    ] {
        if !api.resolve_proc(symbol)? {
            api.resolve_library(symbol)?;
        }
    }
    Ok(true)
}
pub fn probe(printed: &mut dyn Write) -> Result<()> {
    let before = descriptors()?;
    let mut api = RawApi::new();
    let mut display = 0;
    // Original init_egl catches initialization exceptions, retaining partial globals.
    let initialized = initialize(&mut api, &mut display).unwrap_or(false);
    let (mut created, mut rejected) = (0, 0);
    if initialized {
        let file = fs::File::open("/dev/null")?;
        for _ in 0..3 {
            let duplicate = match file.try_clone() {
                Ok(value) => value,
                Err(_) => {
                    rejected += 1;
                    continue;
                }
            };
            let mut duplicate = Descriptor(Some(duplicate.into()));
            let fd = duplicate.fd()?;
            let attrs = [
                0x3057, 1928, 0x3056, 1208, 0x3271, 842094158, 0x3272, fd, 0x3273, 0, 0x3274, 2048,
                0x3275, fd, 0x3276, 2473984, 0x3277, 2048, 0x3038,
            ];
            let mut image = 0;
            for attempt in 0..2 {
                image = api.create(display, 0x3270, &attrs)?;
                if image != 0 {
                    break;
                }
                let error = api.error()?;
                if error == 0x3001
                    && attempt == 0
                    && initialize_display(&mut api, &mut display, true)?
                {
                    continue;
                }
                break;
            }
            if image == 0 {
                duplicate.release()?;
                rejected += 1;
            } else {
                api.active(0x84c0)?;
                api.bind(0x8d65, 17)?;
                api.image_target(0x8d65, image)?;
                created += 1;
                api.destroy(display, image)?;
                // Original destroy_egl_image ignores os.close OSError.
                let _ = duplicate.release();
            }
            drop(duplicate);
        }
    }
    let delta = descriptors()? as i128 - before as i128;
    // Keep the raw adapter and its partially loaded libraries alive through count.
    let result=format!("{{\"initialized\": {initialized}, \"created\": {created}, \"rejected\": {rejected}, \"fd_delta\": {delta}}}\n");
    printed.write_all(result.as_bytes())?;
    printed.flush()?;
    drop(api);
    Ok(())
}
pub fn cli() -> Result<()> {
    probe(&mut std::io::stdout().lock())
}
