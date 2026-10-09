use super::fmp4_buffer::{read, seek, write, Buffer};
use ffmpeg_next::{self as av, ffi};
use std::{
    cell::UnsafeCell,
    ptr::{self, NonNull},
};

pub(super) struct MemoryIo {
    context: NonNull<ffi::AVIOContext>,
    buffer: Box<UnsafeCell<Buffer>>,
}
impl MemoryIo {
    #[expect(
        unsafe_code,
        reason = "FFmpeg custom AVIO requires synchronous C callbacks and FFmpeg-owned buffer allocations"
    )]
    fn new(data: Vec<u8>, writable: bool) -> Result<Self, av::Error> {
        let buffer = Box::new(UnsafeCell::new(Buffer {
            data,
            base: 0,
            position: 0,
        }));
        // SAFETY: Allocations are checked before use. The boxed UnsafeCell remains
        // alive until AVIO is freed; callbacks execute synchronously on its owner.
        unsafe {
            let bytes = ffi::av_malloc(4096).cast::<u8>();
            if bytes.is_null() {
                return Err(av::Error::from(-12));
            }
            let context = ffi::avio_alloc_context(
                bytes,
                4096,
                i32::from(writable),
                buffer.get().cast(),
                if writable { None } else { Some(read) },
                if writable { Some(write) } else { None },
                Some(seek),
            );
            let Some(context) = NonNull::new(context) else {
                ffi::av_free(bytes.cast());
                return Err(av::Error::from(-12));
            };
            Ok(Self { context, buffer })
        }
    }
    #[expect(
        unsafe_code,
        reason = "Access an exclusively owned callback sink only between synchronous FFmpeg calls"
    )]
    pub fn drain(&mut self) -> Result<Vec<u8>, av::Error> {
        // SAFETY: No FFmpeg call is in progress; the exclusive self borrow prevents
        // a concurrent callback. UnsafeCell preserves the callback pointer provenance.
        let data = unsafe { &mut *self.buffer.get() };
        let end = data
            .base
            .checked_add(i64::try_from(data.data.len()).map_err(|_| av::Error::from(-75))?)
            .ok_or_else(|| av::Error::from(-75))?;
        if data.position != end {
            return Err(av::Error::from(-22));
        }
        data.base = end;
        Ok(std::mem::take(&mut data.data))
    }
}
#[expect(
    unsafe_code,
    reason = "Release the current AVIO buffer (which FFmpeg may replace) and its context exactly once"
)]
impl Drop for MemoryIo {
    fn drop(&mut self) {
        // SAFETY: Owner contexts have already detached this custom IO. Its buffer
        // came from av_malloc and its opaque Box outlives avio_context_free.
        unsafe {
            let mut context = self.context.as_ptr();
            ffi::av_free((*context).buffer.cast());
            (*context).buffer = ptr::null_mut();
            ffi::avio_context_free(&mut context);
        }
    }
}
pub(super) struct Output {
    pub format: av::format::context::Output,
    io: MemoryIo,
}
impl Output {
    #[expect(
        unsafe_code,
        reason = "The caller owns residual AVDictionary options on both successful and failed header writes"
    )]
    pub fn header(&mut self, options: av::Dictionary<'_>) -> Result<(), av::Error> {
        // SAFETY: disown transfers this live dictionary to the local pointer.
        // FFmpeg updates it to residual options; free accepts null and releases
        // that remaining allocation exactly once on every return path.
        unsafe {
            let mut options = options.disown();
            let result = ffi::avformat_write_header(self.format.as_mut_ptr(), &mut options);
            ffi::av_dict_free(&mut options);
            if result < 0 {
                Err(av::Error::from(result))
            } else {
                Ok(())
            }
        }
    }
    #[expect(
        unsafe_code,
        reason = "Attach owned custom AVIO to an FFmpeg MP4 format context"
    )]
    pub fn new() -> Result<Self, av::Error> {
        let io = MemoryIo::new(Vec::new(), true)?;
        // SAFETY: FFmpeg allocates the checked context; the IO Box remains owned
        // beside its wrapper and is detached before the wrapper destructor runs.
        unsafe {
            let mut context = ptr::null_mut();
            let result = ffi::avformat_alloc_output_context2(
                &mut context,
                ptr::null_mut(),
                c"mp4".as_ptr(),
                ptr::null(),
            );
            if result < 0 {
                return Err(av::Error::from(result));
            }
            if context.is_null() {
                return Err(av::Error::from(-12));
            }
            (*context).pb = io.context.as_ptr();
            (*context).flags |= ffi::AVFMT_FLAG_CUSTOM_IO;
            Ok(Self {
                format: av::format::context::Output::wrap(context),
                io,
            })
        }
    }
    #[expect(
        unsafe_code,
        reason = "Flush synchronously before extracting the exclusively owned incremental sink"
    )]
    pub fn drain(&mut self) -> Result<Vec<u8>, av::Error> {
        // SAFETY: The non-null IO context is live and exclusively owned here.
        unsafe {
            ffi::avio_flush(self.io.context.as_ptr());
        }
        self.io.drain()
    }
}
#[expect(
    unsafe_code,
    reason = "ffmpeg-next Output destructor uses avio_close; custom AVIO must instead be detached and freed by MemoryIo"
)]
impl Drop for Output {
    fn drop(&mut self) {
        // SAFETY: The live context is exclusively owned; clearing pb prevents the
        // wrapper from using the protocol-only avio_close on our custom AVIO.
        unsafe {
            (*self.format.as_mut_ptr()).pb = ptr::null_mut();
        }
    }
}
#[expect(
    unsafe_code,
    reason = "Probe Annex-B H264 using owned custom input AVIO, as the original stream-template path"
)]
pub(super) fn probe(bytes: Vec<u8>) -> Result<av::codec::Parameters, av::Error> {
    let io = MemoryIo::new(bytes, false)?;
    // SAFETY: Checked allocation is transferred to avformat_open_input. On failure
    // FFmpeg frees it; on success Input owns it. The CUSTOM_IO flag keeps io owned
    // here, and Input drops before io on every return path.
    unsafe {
        let mut context = ffi::avformat_alloc_context();
        if context.is_null() {
            return Err(av::Error::from(-12));
        }
        (*context).pb = io.context.as_ptr();
        (*context).flags |= ffi::AVFMT_FLAG_CUSTOM_IO;
        let result = ffi::avformat_open_input(
            &mut context,
            ptr::null(),
            ffi::av_find_input_format(c"h264".as_ptr()),
            ptr::null_mut(),
        );
        if result < 0 {
            return Err(av::Error::from(result));
        }
        let input = av::format::context::Input::wrap(context);
        let result = ffi::avformat_find_stream_info(context, ptr::null_mut());
        if result < 0 {
            return Err(av::Error::from(result));
        }
        let stream = input
            .streams()
            .best(av::media::Type::Video)
            .ok_or(av::Error::StreamNotFound)?;
        Ok(stream.parameters().clone())
    }
}
