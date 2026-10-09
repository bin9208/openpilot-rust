#![expect(unsafe_code, reason = "approved system zlib C ABI encoder boundary")]
use super::error;
use crate::Error;
use libc::{c_int, c_ulong};
use libloading::Library;

type Bound = unsafe extern "C" fn(c_ulong) -> c_ulong;
type Compress = unsafe extern "C" fn(*mut u8, *mut c_ulong, *const u8, c_ulong, c_int) -> c_int;

pub(crate) fn compress(source: &[u8]) -> Result<Vec<u8>, Error> {
    // SAFETY: libz.so.1 is the system zlib ABI from zlib.h; it retains no Rust
    // callbacks. The handle remains alive until the calls below finish.
    let library =
        unsafe { Library::new("libz.so.1") }.map_err(|failure| error(failure.to_string()))?;
    // SAFETY: these two symbol signatures use the exact zlib.h C ABI widths.
    let bound = unsafe { library.get::<Bound>(b"compressBound\0") }
        .map_err(|failure| error(failure.to_string()))?;
    // SAFETY: compress2 matches zlib.h and is called while its Library lives.
    let compress = unsafe { library.get::<Compress>(b"compress2\0") }
        .map_err(|failure| error(failure.to_string()))?;
    let source_size =
        c_ulong::try_from(source.len()).map_err(|_| error("zlib source size overflow"))?;
    // SAFETY: compressBound accepts every uLong size and retains no pointers.
    let capacity = usize::try_from(unsafe { bound(source_size) })
        .map_err(|_| error("zlib compressed size overflow"))?;
    let mut output = vec![0; capacity];
    let mut size =
        c_ulong::try_from(capacity).map_err(|_| error("zlib compressed size overflow"))?;
    // SAFETY: source is readable for source_size bytes; output is exclusively
    // writable for size bytes; size is initialized and valid. Level9 is valid.
    let status = unsafe {
        compress(
            output.as_mut_ptr(),
            &mut size,
            source.as_ptr(),
            source_size,
            9,
        )
    };
    if status != 0 || size as u128 > output.len() as u128 {
        return Err(error(format!("Error {status} while compressing data")));
    }
    output.truncate(size as usize);
    Ok(output)
}
