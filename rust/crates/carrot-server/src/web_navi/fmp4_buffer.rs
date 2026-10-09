use ffmpeg_next::ffi;
use std::{ffi::c_void, ptr};

pub(super) struct Buffer {
    pub(super) data: Vec<u8>,
    pub(super) base: i64,
    pub(super) position: i64,
}
#[expect(
    unsafe_code,
    reason = "FFmpeg supplies valid read buffer and the live exclusively accessed opaque Buffer"
)]
pub(super) unsafe extern "C" fn read(opaque: *mut c_void, target: *mut u8, size: i32) -> i32 {
    let Ok(size) = usize::try_from(size) else {
        return -22;
    };
    // SAFETY: AVIO receives this exact Box pointer; size is the writable C buffer
    // extent. Calls are synchronous, nonoverlapping, and cannot outlive MemoryIo.
    unsafe {
        let data = &mut *opaque.cast::<Buffer>();
        let Ok(offset) = usize::try_from(data.position) else {
            return -22;
        };
        let Some(bytes) = data.data.get(offset..) else {
            return ffi::AVERROR_EOF;
        };
        let count = size.min(bytes.len());
        if count == 0 {
            return ffi::AVERROR_EOF;
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), target, count);
        data.position += i64::try_from(count).unwrap_or(0);
        i32::try_from(count).unwrap_or(-75)
    }
}
#[expect(
    unsafe_code,
    reason = "FFmpeg supplies a valid packet buffer and live exclusively accessed output Buffer"
)]
pub(super) unsafe extern "C" fn write(opaque: *mut c_void, source: *mut u8, size: i32) -> i32 {
    let Ok(size) = usize::try_from(size) else {
        return -22;
    };
    // SAFETY: The C buffer contains size initialized bytes. Only this synchronous
    // callback accesses the Box; checked offsets and reservation prevent unwinding.
    unsafe {
        let data = &mut *opaque.cast::<Buffer>();
        let Some(offset) = data
            .position
            .checked_sub(data.base)
            .and_then(|v| usize::try_from(v).ok())
        else {
            return -22;
        };
        let Some(end) = offset.checked_add(size) else {
            return -75;
        };
        if end > data.data.len() {
            if data.data.try_reserve(end - data.data.len()).is_err() {
                return -12;
            }
            data.data.resize(end, 0);
        }
        let Some(destination) = data.data.get_mut(offset..end) else {
            return -22;
        };
        ptr::copy_nonoverlapping(source, destination.as_mut_ptr(), size);
        let Some(position) = data
            .position
            .checked_add(i64::from(i32::try_from(size).unwrap_or(0)))
        else {
            return -75;
        };
        data.position = position;
        i32::try_from(size).unwrap_or(-75)
    }
}
#[expect(
    unsafe_code,
    reason = "FFmpeg invokes seek synchronously on its owned live opaque Buffer"
)]
pub(super) unsafe extern "C" fn seek(opaque: *mut c_void, offset: i64, whence: i32) -> i64 {
    // SAFETY: The opaque pointer refers to the owner Box and no concurrent caller
    // accesses it. Every position calculation is checked before assignment.
    let data = unsafe { &mut *opaque.cast::<Buffer>() };
    let Some(end) = i64::try_from(data.data.len())
        .ok()
        .and_then(|v| data.base.checked_add(v))
    else {
        return -75;
    };
    if whence == ffi::AVSEEK_SIZE {
        return end;
    }
    let origin = match whence & !ffi::AVSEEK_FORCE {
        0 => 0,
        1 => data.position,
        2 => end,
        _ => return -22,
    };
    match origin.checked_add(offset) {
        Some(position) if position >= data.base => {
            data.position = position;
            position
        }
        Some(_) | None => -22,
    }
}
