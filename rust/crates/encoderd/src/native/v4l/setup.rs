#![allow(unsafe_code)]
use super::{abi::*, io, warn};
use crate::{
    config::{Codec, Settings},
    native::platform,
    profile::EncoderInfo,
    Error,
};
use openpilot_logging::{log_site, record::Level};
use std::os::fd::{AsRawFd, OwnedFd};

fn control(fd: &OwnedFd, id: u32, value: u32) -> Result<(), Error> {
    let mut value = v4l2_control {
        id,
        value: value as i32,
    };
    // SAFETY: [FFI boundary] S_CTRL receives the matching initialized ABI type.
    unsafe { io::ioctl(fd, ENCODER_VIDIOC_S_CTRL, &mut value) }
}
fn read_control(fd: &OwnedFd, id: u32) -> Result<i32, Error> {
    let mut value = v4l2_control { id, value: 0 };
    // SAFETY: [FFI boundary] G_CTRL receives the matching initialized ABI type.
    unsafe {
        io::ioctl(fd, ENCODER_VIDIOC_G_CTRL, &mut value)?;
    }
    Ok(value.value)
}
pub fn configure(
    fd: &OwnedFd,
    info: &EncoderInfo,
    input: (i32, i32),
    output: (i32, i32),
    settings: Settings,
) -> Result<usize, Error> {
    let mut capability = v4l2_capability::default();
    // SAFETY: [FFI boundary] QUERYCAP receives a fully sized output structure.
    unsafe {
        io::ioctl(fd, ENCODER_VIDIOC_QUERYCAP, &mut capability)?;
    }
    let cbytes = |bytes: &[u8]| {
        bytes
            .split(|&value| value == 0)
            .next()
            .unwrap_or_default()
            .to_vec()
    };
    platform::emit(
        log_site!(),
        Level::Debug,
        format!(
            "opened encoder device {} {} = {}",
            String::from_utf8_lossy(&cbytes(&capability.driver)),
            String::from_utf8_lossy(&cbytes(&capability.card)),
            fd.as_raw_fd()
        ),
    );
    if cbytes(&capability.driver) != b"msm_vidc_driver"
        || cbytes(&capability.card) != b"msm_vidc_venc"
    {
        return Err(Error::Contract(
            "source msm_vidc encoder capability assertion",
        ));
    }
    let hevc = settings.codec == Codec::FullHevc;
    let youtube = info.publish == "youtubeRoadEncodeData";
    let livestream = info.publish == "livestreamRoadEncodeData";
    let format = |kind, dimensions: (i32, i32), pixel, color| -> Result<v4l2_format, Error> {
        let mut value = v4l2_format {
            type_: kind,
            ..Default::default()
        };
        value.fmt.pix_mp = v4l2_pix_format_mplane {
            width: dimensions.0.try_into()?,
            height: dimensions.1.try_into()?,
            pixelformat: pixel,
            field: V4L2_FIELD_ANY,
            colorspace: color,
            ..Default::default()
        };
        Ok(value)
    };
    let mut capture = format(
        V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
        output,
        if hevc {
            ENCODER_V4L2_PIX_FMT_HEVC
        } else {
            ENCODER_V4L2_PIX_FMT_H264
        },
        V4L2_COLORSPACE_DEFAULT,
    )?;
    // SAFETY: [FFI boundary] S_FMT selects the initialized multi-planar union arm.
    unsafe {
        io::ioctl(fd, ENCODER_VIDIOC_S_FMT, &mut capture)?;
    }
    // SAFETY: [FFI boundary] the queue type selects pix_mp, initialized before S_FMT.
    let capture = unsafe { capture.fmt.pix_mp };
    if youtube
        && (capture.width != u32::try_from(output.0)? || capture.height != u32::try_from(output.1)?)
    {
        return Err(Error::Contract("YouTube encoder output contract rejected"));
    }
    let mut timing = v4l2_streamparm {
        type_: V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
        ..Default::default()
    };
    timing.parm.output = v4l2_outputparm {
        timeperframe: v4l2_fract {
            numerator: 1,
            denominator: info.fps.try_into()?,
        },
        ..Default::default()
    };
    // SAFETY: [FFI boundary] S_PARM selects the initialized output union arm.
    unsafe {
        io::ioctl(fd, ENCODER_VIDIOC_S_PARM, &mut timing)?;
    }
    let mut source = format(
        V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
        input,
        ENCODER_V4L2_PIX_FMT_NV12,
        V4L2_COLORSPACE_470_SYSTEM_BG,
    )?;
    // SAFETY: [FFI boundary] the input format has its matching initialized union.
    unsafe {
        io::ioctl(fd, ENCODER_VIDIOC_S_FMT, &mut source)?;
    }
    if youtube {
        let mut width = input.0 & !1;
        let mut height = ((width * 9) / 16) & !1;
        if height > input.1 {
            height = input.1 & !1;
            width = ((height * 16) / 9) & !1;
        }
        let left = ((input.0 - width) / 2) & !1;
        let top = ((input.1 - height) / 2) & !1;
        let mut selection = v4l2_selection {
            type_: V4L2_BUF_TYPE_VIDEO_OUTPUT,
            target: V4L2_SEL_TGT_CROP,
            r: v4l2_rect {
                left,
                top,
                width: width.try_into()?,
                height: height.try_into()?,
            },
            ..Default::default()
        };
        // SAFETY: [FFI boundary] S_SELECTION receives its initialized structure.
        let applied = unsafe { io::ioctl(fd, ENCODER_VIDIOC_S_SELECTION, &mut selection) }.is_ok();
        let rect = selection.r;
        if applied
            && rect.left == left
            && rect.top == top
            && rect.width == width as u32
            && rect.height == height as u32
        {
            warn(format!(
                "YouTube encoder crop={width}x{height}+{left}+{top} output={}x{} bitrate={}",
                output.0, output.1, settings.bitrate
            ));
        } else if applied {
            warn(format!(
                "YouTube encoder adjusted crop to {}x{}+{}+{}; scaling full frame to exact {}x{}",
                rect.width, rect.height, rect.left, rect.top, output.0, output.1
            ));
        } else {
            warn(format!(
                "YouTube encoder crop unavailable; scaling full frame to exact {}x{}",
                output.0, output.1
            ));
        }
    }
    // SAFETY: [FFI boundary] S_FMT initialized the pix_mp arm selected by type_.
    let input_size = unsafe { source.fmt.pix_mp.plane_fmt[0].sizeimage };
    platform::emit(
        log_site!(),
        Level::Debug,
        format!(
            "in buffer size {}, out buffer size {}",
            input_size as i32, capture.plane_fmt[0].sizeimage as i32
        ),
    );
    for (id, value) in [
        (V4L2_CID_MPEG_VIDEO_BITRATE, settings.bitrate as u32),
        (
            V4L2_CID_MPEG_VIDC_VIDEO_NUM_P_FRAMES,
            (settings.gop - settings.b_frames - 1) as u32,
        ),
        (
            V4L2_CID_MPEG_VIDC_VIDEO_NUM_B_FRAMES,
            settings.b_frames as u32,
        ),
        (
            V4L2_CID_MPEG_VIDEO_HEADER_MODE,
            V4L2_MPEG_VIDEO_HEADER_MODE_SEPARATE,
        ),
        (
            V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL,
            if settings.cbr {
                V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL_CBR_CFR
            } else {
                V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL_VBR_CFR
            },
        ),
        (
            V4L2_CID_MPEG_VIDC_VIDEO_PRIORITY,
            V4L2_MPEG_VIDC_VIDEO_PRIORITY_REALTIME_DISABLE,
        ),
        (V4L2_CID_MPEG_VIDC_VIDEO_IDR_PERIOD, 1),
    ] {
        control(fd, id, value)?;
    }
    if hevc {
        for (id, value) in [
            (
                V4L2_CID_MPEG_VIDC_VIDEO_HEVC_PROFILE,
                V4L2_MPEG_VIDC_VIDEO_HEVC_PROFILE_MAIN,
            ),
            (
                V4L2_CID_MPEG_VIDC_VIDEO_HEVC_TIER_LEVEL,
                V4L2_MPEG_VIDC_VIDEO_HEVC_LEVEL_HIGH_TIER_LEVEL_5,
            ),
            (
                V4L2_CID_MPEG_VIDC_VIDEO_VUI_TIMING_INFO,
                V4L2_MPEG_VIDC_VIDEO_VUI_TIMING_INFO_ENABLED,
            ),
        ] {
            control(fd, id, value)?;
        }
    } else {
        for (id, value) in [
            (
                V4L2_CID_MPEG_VIDEO_H264_PROFILE,
                V4L2_MPEG_VIDEO_H264_PROFILE_HIGH,
            ),
            (
                V4L2_CID_MPEG_VIDEO_H264_LEVEL,
                V4L2_MPEG_VIDEO_H264_LEVEL_UNKNOWN,
            ),
            (
                V4L2_CID_MPEG_VIDEO_H264_ENTROPY_MODE,
                V4L2_MPEG_VIDEO_H264_ENTROPY_MODE_CABAC,
            ),
            (
                V4L2_CID_MPEG_VIDC_VIDEO_H264_CABAC_MODEL,
                V4L2_CID_MPEG_VIDC_VIDEO_H264_CABAC_MODEL_0,
            ),
            (V4L2_CID_MPEG_VIDEO_H264_LOOP_FILTER_MODE, 0),
            (V4L2_CID_MPEG_VIDEO_H264_LOOP_FILTER_ALPHA, 0),
            (V4L2_CID_MPEG_VIDEO_H264_LOOP_FILTER_BETA, 0),
            (V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MODE, 0),
        ] {
            control(fd, id, value)?;
        }
    }
    if livestream {
        warn(format!("H264 compatibility profile enabled for {}: output={}x{} bitrate={} multi-slice-max-bytes=1200", info.publish, output.0, output.1, settings.bitrate));
        let _ = control(
            fd,
            V4L2_CID_MPEG_VIDEO_H264_PROFILE,
            V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE,
        );
        let _ = control(
            fd,
            V4L2_CID_MPEG_VIDEO_H264_ENTROPY_MODE,
            V4L2_MPEG_VIDEO_H264_ENTROPY_MODE_CAVLC,
        );
        if control(fd, V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MAX_BYTES, 1200).is_ok()
            && control(
                fd,
                V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MODE,
                V4L2_MPEG_VIDEO_MULTI_SICE_MODE_MAX_BYTES,
            )
            .is_ok()
        {
            warn(format!(
                "multi-slice enabled for {}: max-bytes=1200",
                info.publish
            ));
        } else {
            let _ = control(
                fd,
                V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MODE,
                V4L2_MPEG_VIDEO_MULTI_SLICE_MODE_SINGLE,
            );
            warn(format!(
                "multi-slice unavailable for {}; falling back to single-slice H264",
                info.publish
            ));
        }
    }
    if settings.cbr {
        let rate = read_control(fd, V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL);
        let bitrate = read_control(fd, V4L2_CID_MPEG_VIDEO_BITRATE);
        if rate
            .as_ref()
            .is_ok_and(|&value| value != V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL_CBR_CFR as i32)
        {
            return Err(Error::Contract(
                "V4L2 encoder did not retain CBR_CFR rate control",
            ));
        }
        if rate.is_err() {
            warn(format!(
                "V4L2 rate-control readback unavailable for {}; CBR_CFR set was accepted",
                info.publish
            ));
        }
        if let Ok(value) = bitrate.as_ref() {
            if *value != settings.bitrate {
                warn(format!(
                    "V4L2 encoder adjusted bitrate for {}: requested={} applied={value}",
                    info.publish, settings.bitrate
                ));
            }
        }
        warn(format!(
            "CBR_CFR {} for {}: bitrate={} gop={} fps={}",
            if rate.is_ok() {
                "verified"
            } else {
                "configured"
            },
            info.publish,
            bitrate.unwrap_or(settings.bitrate),
            settings.gop,
            info.fps
        ));
    }
    Ok(capture.plane_fmt[0].sizeimage.try_into()?)
}
