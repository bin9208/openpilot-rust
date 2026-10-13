//! Opaque librtmp ABI from youtube_live_transport.py; the Library outlives every handle.
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_void},
    sync::{Arc, Mutex},
};

// RTMP_Init lazily allocates the provider's process-global RTMP_TLS_ctx.
// Keep its library loaded, matching ctypes.CDLL; only handles are per client.
static LOADED: Mutex<Option<Arc<Api>>> = Mutex::new(None);

#[derive(Debug, thiserror::Error)]
pub(super) enum LoadError {
    #[error(transparent)]
    Library(#[from] libloading::Error),
    #[error("missing librtmp symbols: {}", .0.join(", "))]
    Missing(Vec<String>),
    #[error("librtmp provider lock poisoned")]
    Poisoned,
}

#[repr(C)]
pub(super) struct AVal {
    pub value: *mut c_char,
    pub length: c_int,
}
type Handle = *mut c_void;
pub(super) struct Api {
    _library: Library,
    pub alloc: unsafe extern "C" fn() -> Handle,
    pub init: unsafe extern "C" fn(Handle),
    pub setup: unsafe extern "C" fn(Handle, *mut c_char) -> c_int,
    pub option: unsafe extern "C" fn(Handle, *const AVal, *const AVal) -> c_int,
    pub enable: unsafe extern "C" fn(Handle),
    pub connect: unsafe extern "C" fn(Handle, *mut c_void) -> c_int,
    pub stream: unsafe extern "C" fn(Handle, c_int) -> c_int,
    pub write: unsafe extern "C" fn(Handle, *const c_char, c_int) -> c_int,
    pub connected: unsafe extern "C" fn(Handle) -> c_int,
    pub close: unsafe extern "C" fn(Handle),
    pub free: unsafe extern "C" fn(Handle),
}
impl Api {
    pub fn load() -> Result<Arc<Self>, LoadError> {
        let mut loaded = LOADED.lock().map_err(|_| LoadError::Poisoned)?;
        if let Some(api) = loaded.as_ref() {
            return Ok(Arc::clone(api));
        }
        let api = Arc::new(Self::resolve()?);
        *loaded = Some(Arc::clone(&api));
        Ok(api)
    }
    #[expect(
        unsafe_code,
        reason = "Resolve the original eleven librtmp C signatures while retaining their Library"
    )]
    fn resolve() -> Result<Self, LoadError> {
        // SAFETY: librtmp.so.1 is the existing platform provider. These exact ABI
        // signatures match librtmp/rtmp.h and the original ctypes declarations;
        // function pointers cannot outlive _library, including handle destruction.
        unsafe {
            let library = Library::new("librtmp.so.1")?;
            let names: [&[u8]; 11] = [
                b"RTMP_Alloc\0",
                b"RTMP_Init\0",
                b"RTMP_SetupURL\0",
                b"RTMP_SetOpt\0",
                b"RTMP_EnableWrite\0",
                b"RTMP_Connect\0",
                b"RTMP_ConnectStream\0",
                b"RTMP_Write\0",
                b"RTMP_IsConnected\0",
                b"RTMP_Close\0",
                b"RTMP_Free\0",
            ];
            let missing: Vec<_> = names
                .iter()
                .filter(|name| library.get::<*const c_void>(name).is_err())
                .map(|name| String::from_utf8_lossy(&name[..name.len() - 1]).into_owned())
                .collect();
            if !missing.is_empty() {
                return Err(LoadError::Missing(missing));
            }
            let loaded = Self {
                alloc: *library.get(b"RTMP_Alloc\0")?,
                init: *library.get(b"RTMP_Init\0")?,
                setup: *library.get(b"RTMP_SetupURL\0")?,
                option: *library.get(b"RTMP_SetOpt\0")?,
                enable: *library.get(b"RTMP_EnableWrite\0")?,
                connect: *library.get(b"RTMP_Connect\0")?,
                stream: *library.get(b"RTMP_ConnectStream\0")?,
                write: *library.get(b"RTMP_Write\0")?,
                connected: *library.get(b"RTMP_IsConnected\0")?,
                close: *library.get(b"RTMP_Close\0")?,
                free: *library.get(b"RTMP_Free\0")?,
                _library: library,
            };
            if let Ok(log) = loaded
                ._library
                .get::<unsafe extern "C" fn(c_int)>(b"RTMP_LogSetLevel\0")
            {
                log(1);
            }
            Ok(loaded)
        }
    }
}
