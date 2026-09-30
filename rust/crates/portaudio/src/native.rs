use crate::{
    callback::{self, Callback, Capture, Handler, Render},
    Error,
};
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_ulong, c_void},
    path::Path,
    ptr,
    sync::Mutex,
};
#[repr(C)]
struct DeviceInfo {
    version: c_int,
    name: *const c_char,
    host_api: c_int,
    input_channels: c_int,
    output_channels: c_int,
    low_input_latency: f64,
    low_output_latency: f64,
    high_input_latency: f64,
    high_output_latency: f64,
    sample_rate: f64,
}
#[repr(C)]
struct Parameters {
    device: c_int,
    channels: c_int,
    format: c_ulong,
    latency: f64,
    host_info: *mut c_void,
}
type Operation = unsafe extern "C" fn(*mut c_void) -> c_int;
type CallbackFn = unsafe extern "C" fn(
    *const c_void,
    *mut c_void,
    c_ulong,
    *const c_void,
    c_ulong,
    *mut c_void,
) -> c_int;
type Open = unsafe extern "C" fn(
    *mut *mut c_void,
    *const Parameters,
    *const Parameters,
    f64,
    c_ulong,
    c_ulong,
    CallbackFn,
    *mut c_void,
) -> c_int;
pub struct Stream {
    library: Library,
    stream: *mut c_void,
    callback: Box<Callback>,
    terminate: unsafe extern "C" fn() -> c_int,
    close: Operation,
    stop: Operation,
    active: Operation,
    initialized: bool,
    input: bool,
    initialize: unsafe extern "C" fn() -> c_int,
    device: unsafe extern "C" fn() -> c_int,
    info: unsafe extern "C" fn(c_int) -> *const DeviceInfo,
    open_stream: Open,
}
fn check(code: c_int, operation: &'static str) -> Result<(), Error> {
    if code < 0 {
        Err(Error::Api { operation, code })
    } else {
        Ok(())
    }
}
impl Stream {
    pub fn load(path: &Path, render: Render) -> Result<Self, Error> {
        Self::load_handler(path, Handler::Output(render), false)
    }
    pub fn load_input(path: &Path, capture: Capture) -> Result<Self, Error> {
        Self::load_handler(path, Handler::Input(capture), true)
    }
    fn load_handler(path: &Path, handler: Handler, input: bool) -> Result<Self, Error> {
        // SAFETY: caller selects the external PortAudio v19 ABI library. Symbols
        // use its published C declarations and are retained by the library owner.
        let library = unsafe { Library::new(path)? };
        // SAFETY: exact v19 C ABI signatures; library remains loaded through Drop.
        let (initialize, terminate, close, stop, active, device, info, open) = unsafe {
            (
                *library.get::<unsafe extern "C" fn() -> c_int>(b"Pa_Initialize\0")?,
                *library.get::<unsafe extern "C" fn() -> c_int>(b"Pa_Terminate\0")?,
                *library.get::<Operation>(b"Pa_CloseStream\0")?,
                *library.get::<Operation>(b"Pa_StopStream\0")?,
                *library.get::<Operation>(b"Pa_IsStreamActive\0")?,
                *library.get::<unsafe extern "C" fn() -> c_int>(if input {
                    b"Pa_GetDefaultInputDevice\0"
                } else {
                    b"Pa_GetDefaultOutputDevice\0"
                })?,
                *library.get::<unsafe extern "C" fn(c_int) -> *const DeviceInfo>(
                    b"Pa_GetDeviceInfo\0",
                )?,
                *library.get::<Open>(b"Pa_OpenStream\0")?,
            )
        };
        let mut owner = Self {
            library,
            stream: ptr::null_mut(),
            callback: Box::new(Callback(Mutex::new(handler))),
            terminate,
            close,
            stop,
            active,
            initialized: false,
            input,
            initialize,
            device,
            info,
            open_stream: open,
        };
        // SAFETY: no stream exists; this matches sounddevice import initialization.
        unsafe {
            check(initialize(), "initialize")?;
        }
        owner.initialized = true;
        Ok(owner)
    }
    pub fn open(&mut self) -> Result<(), Error> {
        if !self.stream.is_null() {
            return Err(Error::Contract("stream already open"));
        }
        // SAFETY: reinitialization only occurs before a successful open; the
        // library owns device information and parameters live through the call.
        unsafe {
            if self.initialized {
                let code = (self.terminate)();
                if code < 0 {
                    eprintln!("soundd PortAudio terminate before open: {code}");
                }
                self.initialized = false;
            }
            check((self.initialize)(), "initialize before open")?;
            self.initialized = true;
            let index = (self.device)();
            if index < 0 {
                return Err(Error::Api {
                    operation: "default output device",
                    code: index,
                });
            }
            let info = (self.info)(index)
                .as_ref()
                .ok_or(Error::Contract("missing device information"))?;
            let parameters = Parameters {
                device: index,
                channels: 1,
                format: 1,
                latency: if self.input {
                    info.high_input_latency
                } else {
                    info.high_output_latency
                },
                host_info: ptr::null_mut(),
            };
            let mut stream = ptr::null_mut();
            check(
                (self.open_stream)(
                    &mut stream,
                    if self.input { &parameters } else { ptr::null() },
                    if self.input { ptr::null() } else { &parameters },
                    if self.input { 16000. } else { 48000. },
                    if self.input { 800 } else { 4096 },
                    0,
                    callback::render,
                    ptr::from_ref(self.callback.as_ref()).cast_mut().cast(),
                ),
                "open",
            )?;
            if stream.is_null() {
                return Err(Error::Contract("successful open returned null stream"));
            }
            self.stream = stream;
        }
        Ok(())
    }
    pub fn start(&self) -> Result<(), Error> {
        // SAFETY: stream was opened successfully and is owned until Drop.
        unsafe {
            check(
                self.library.get::<Operation>(b"Pa_StartStream\0")?(self.stream),
                "start",
            )
        }
    }
    pub fn active(&self) -> Result<bool, Error> {
        // SAFETY: live owned stream; function pointer belongs to retained library.
        let result = unsafe { (self.active)(self.stream) };
        check(result, "active")?;
        Ok(result == 1)
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: stop/close finish callbacks before callback allocation and
        // library are released. No other owner can close this stream.
        unsafe {
            if !self.stream.is_null() {
                for (operation, code) in [
                    ("stop", (self.stop)(self.stream)),
                    ("close", (self.close)(self.stream)),
                ] {
                    if code < 0 && code != -9983 {
                        eprintln!("soundd PortAudio {operation}: {code}");
                    }
                }
            }
            if self.initialized {
                let code = (self.terminate)();
                if code < 0 {
                    eprintln!("soundd PortAudio terminate: {code}");
                }
            }
        }
    }
}
