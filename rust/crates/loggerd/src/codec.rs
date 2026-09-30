use crate::Error;
use ffmpeg_next::{codec::Context, ffi};

#[expect(
    unsafe_code,
    reason = "Observe the AVIO close result which ffmpeg-next's destructor discards"
)]
pub fn close_output(
    output: &mut ffmpeg_next::format::context::Output,
) -> Result<(), ffmpeg_next::Error> {
    // SAFETY: Output exclusively owns the live AVFormatContext and its AVIOContext.
    // avio_closep releases pb and sets it to null; the wrapper's later avio_close(null)
    // is a no-op, and its destructor still owns and frees the format context.
    let result = unsafe { ffi::avio_closep(&mut (*output.as_mut_ptr()).pb) };
    if result == 0 {
        Ok(())
    } else {
        Err(ffmpeg_next::Error::from(result))
    }
}

#[expect(
    unsafe_code,
    reason = "H264 muxing needs codec identity but does not invoke a video encoder"
)]
pub fn video_context(
    encoder: Option<ffmpeg_next::Codec>,
) -> Result<ffmpeg_next::codec::encoder::video::Video, Error> {
    let mut context = match encoder {
        Some(encoder) => Context::new_with_codec(encoder),
        None => Context::new(),
    };
    // SAFETY: Context exclusively owns this FFmpeg allocation. Check allocation
    // before assigning the scalar codec identity used by codec-parameter export.
    unsafe {
        let pointer = context.as_mut_ptr();
        if pointer.is_null() {
            return Err(std::io::Error::from(std::io::ErrorKind::OutOfMemory).into());
        }
        if encoder.is_none() {
            (*pointer).codec_id = ffi::AVCodecID::AV_CODEC_ID_H264;
        }
    }
    Ok(context.encoder().video()?)
}

#[expect(
    unsafe_code,
    reason = "FFmpeg exposes codec extradata only through AVCodecContext; copied allocation is owned by the context"
)]
pub fn set_extradata(context: &mut Context, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() {
        return Ok(());
    }
    let size = i32::try_from(bytes.len())?;
    let allocation = bytes
        .len()
        .checked_add(usize::try_from(ffi::AV_INPUT_BUFFER_PADDING_SIZE)?)
        .ok_or(Error::Invalid("codec header too large"))?;
    // SAFETY: the live context is exclusively borrowed. FFmpeg allocates a padded,
    // zeroed buffer; a checked non-null allocation receives exactly bytes.len()
    // initialized bytes. Ownership transfers to avcodec_free_context. Existing
    // extradata came from FFmpeg and is freed using the same allocator.
    unsafe {
        let context = context.as_mut_ptr();
        if context.is_null() {
            return Err(std::io::Error::from(std::io::ErrorKind::OutOfMemory).into());
        }
        let data = ffi::av_mallocz(allocation).cast::<u8>();
        if data.is_null() {
            return Err(std::io::Error::from(std::io::ErrorKind::OutOfMemory).into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
        ffi::av_free((*context).extradata.cast());
        (*context).extradata = data;
        (*context).extradata_size = size;
    }
    Ok(())
}
