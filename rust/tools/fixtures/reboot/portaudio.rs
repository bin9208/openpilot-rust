//! Owned fake v19 provider. Compile as cdylib; never loads an audio device.
use std::{
    ffi::{c_int, c_ulong, c_void},
    io::Write,
    sync::atomic::{AtomicUsize, Ordering},
};
static CHANNELS: AtomicUsize = AtomicUsize::new(0);
fn record(name: &str) -> i32 {
    let path = std::env::var_os("REBOOT_AUDIO_TRACE").unwrap();
    writeln!(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap(),
        "{name}"
    )
    .unwrap();
    if std::env::var("REBOOT_AUDIO_FAULT").as_deref() == Ok(name) {
        -9999
    } else {
        0
    }
}
#[no_mangle]
pub extern "C" fn Pa_Initialize() -> c_int {
    record("initialize")
}
#[no_mangle]
pub extern "C" fn Pa_Terminate() -> c_int {
    record("terminate")
}
#[no_mangle]
pub unsafe extern "C" fn Pa_OpenDefaultStream(
    stream: *mut *mut c_void,
    input: c_int,
    output: c_int,
    format: c_ulong,
    rate: f64,
    frames: c_ulong,
    callback: *const c_void,
    user: *mut c_void,
) -> c_int {
    assert_eq!(input, 0);
    assert!(output > 0);
    assert_eq!(format, 1);
    assert!(rate > 0.0);
    assert_eq!(frames, 0);
    assert!(callback.is_null());
    assert!(user.is_null());
    let code = record("open");
    if code == 0 {
        // SAFETY: the caller passes a valid unique output slot.
        unsafe {
            *stream = if std::env::var("REBOOT_AUDIO_FAULT").as_deref() == Ok("null") {
                std::ptr::null_mut()
            } else {
                std::ptr::dangling_mut::<c_void>()
            };
        }
    }
    CHANNELS.store(output as usize, Ordering::Relaxed);
    code
}
#[no_mangle]
pub extern "C" fn Pa_StartStream(_: *mut c_void) -> c_int {
    record("start")
}
#[no_mangle]
pub extern "C" fn Pa_StopStream(_: *mut c_void) -> c_int {
    record("stop")
}
#[no_mangle]
pub extern "C" fn Pa_AbortStream(_: *mut c_void) -> c_int {
    record("abort")
}
#[no_mangle]
pub extern "C" fn Pa_CloseStream(_: *mut c_void) -> c_int {
    record("close")
}
#[no_mangle]
pub unsafe extern "C" fn Pa_WriteStream(
    _: *mut c_void,
    bytes: *const c_void,
    frames: c_ulong,
) -> c_int {
    let count = (frames as usize)
        .checked_mul(CHANNELS.load(Ordering::Relaxed))
        .unwrap();
    // SAFETY: v19 caller retains exactly frames*channels initialized float32 samples.
    let samples = unsafe { std::slice::from_raw_parts(bytes.cast::<f32>(), count) };
    let output: Vec<_> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(std::env::var_os("REBOOT_AUDIO_PCM").unwrap(), output).unwrap();
    let code = record("write");
    if std::env::var("REBOOT_AUDIO_FAULT").as_deref() == Ok("underflow") {
        -9980
    } else {
        code
    }
}
