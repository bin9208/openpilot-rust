//! Short interleaved playback for an isolated, parent-deadlined chime child.
//! PortAudio v19 ABI: https://portaudio.com/docs/v19-doxydocs/portaudio_8h.html
use crate::Error;
use libloading::Library;
use std::{
    ffi::{c_int, c_ulong, c_void},
    path::Path,
    ptr,
};
type Operation = unsafe extern "C" fn(*mut c_void) -> c_int;
type Callback = unsafe extern "C" fn(
    *const c_void,
    *mut c_void,
    c_ulong,
    *const c_void,
    c_ulong,
    *mut c_void,
) -> c_int;
type Open = unsafe extern "C" fn(
    *mut *mut c_void,
    c_int,
    c_int,
    c_ulong,
    f64,
    c_ulong,
    Option<Callback>,
    *mut c_void,
) -> c_int;
type Write = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_int;
struct Owner {
    _library: Library,
    stream: *mut c_void,
    terminate: unsafe extern "C" fn() -> c_int,
    close: Operation,
    abort: Operation,
    initialized: bool,
    started: bool,
}
fn check(code: c_int, operation: &'static str) -> Result<(), Error> {
    if code < 0 {
        Err(Error::Api { operation, code })
    } else {
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: only this owner can close its opened stream; symbols remain
        // valid until _library drops after this method. Initialization is paired.
        unsafe {
            if !self.stream.is_null() {
                if self.started {
                    (self.abort)(self.stream);
                }
                (self.close)(self.stream);
            }
            if self.initialized {
                (self.terminate)();
            }
        }
    }
}
/// Blocking playback must run only in the isolated chime process. Its parent
/// owns the four-second deadline, including any stuck audio driver call.
pub fn play(path: &Path, samples: &[f32], channels: u16, rate: u32) -> Result<(), Error> {
    if channels == 0 || rate == 0 || !samples.len().is_multiple_of(usize::from(channels)) {
        return Err(Error::Contract("invalid interleaved PCM shape"));
    }
    let frames = c_ulong::try_from(samples.len() / usize::from(channels))
        .map_err(|_| Error::Contract("PCM frame count overflow"))?;
    // SAFETY: the selected external library must implement the documented v19
    // ABI. Function pointers never outlive the retained library owner.
    let library = unsafe { Library::new(path)? };
    // SAFETY: these are the exact public C signatures; no callback is installed.
    let (initialize, terminate, open, start, stop, abort, close, write) = unsafe {
        (
            *library.get::<unsafe extern "C" fn() -> c_int>(b"Pa_Initialize\0")?,
            *library.get::<unsafe extern "C" fn() -> c_int>(b"Pa_Terminate\0")?,
            *library.get::<Open>(b"Pa_OpenDefaultStream\0")?,
            *library.get::<Operation>(b"Pa_StartStream\0")?,
            *library.get::<Operation>(b"Pa_StopStream\0")?,
            *library.get::<Operation>(b"Pa_AbortStream\0")?,
            *library.get::<Operation>(b"Pa_CloseStream\0")?,
            *library.get::<Write>(b"Pa_WriteStream\0")?,
        )
    };
    let mut owner = Owner {
        _library: library,
        stream: ptr::null_mut(),
        terminate,
        close,
        abort,
        initialized: false,
        started: false,
    };
    // SAFETY: samples are live, aligned, interleaved float32 for exactly frames;
    // all API calls are synchronous, and stream ownership is exclusive.
    unsafe {
        check(initialize(), "initialize")?;
        owner.initialized = true;
        let mut stream = ptr::null_mut();
        check(
            open(
                &mut stream,
                0,
                c_int::from(channels),
                1,
                f64::from(rate),
                0,
                None,
                ptr::null_mut(),
            ),
            "open blocking output",
        )?;
        if stream.is_null() {
            return Err(Error::Contract("successful open returned null stream"));
        }
        owner.stream = stream;
        check(start(stream), "start")?;
        owner.started = true;
        let code = write(stream, samples.as_ptr().cast(), frames);
        // The callback source records underflow without failing playback. This
        // blocking code likewise drains frames already accepted by PortAudio.
        if code != -9980 {
            check(code, "write")?;
        }
        check(stop(stream), "stop")?;
        owner.started = false;
        let closed = close(stream);
        owner.stream = ptr::null_mut();
        let terminated = terminate();
        owner.initialized = false;
        check(closed, "close")?;
        check(terminated, "terminate")?;
    }
    Ok(())
}
