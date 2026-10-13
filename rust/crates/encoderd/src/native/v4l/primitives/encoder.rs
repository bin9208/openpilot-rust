use super::{abi, io, Device, Queue};
use crate::Error;

#[derive(Clone, Copy, Debug)]
pub enum EncoderControl {
    Bitrate,
    PFrames,
    BFrames,
    HeaderMode,
    RateControl,
    Priority,
    IdrPeriod,
    Level,
    LoopFilterMode,
    LoopFilterAlpha,
    LoopFilterBeta,
    SliceMode,
    SliceBytes,
    SliceDelivery,
    RepeatHeader,
    VuiTiming,
    VuiRestriction,
    Profile,
    Entropy,
    CabacModel,
}
impl EncoderControl {
    fn id(self) -> u32 {
        match self {
            Self::Bitrate => abi::V4L2_CID_MPEG_VIDEO_BITRATE,
            Self::PFrames => abi::V4L2_CID_MPEG_VIDC_VIDEO_NUM_P_FRAMES,
            Self::BFrames => abi::V4L2_CID_MPEG_VIDC_VIDEO_NUM_B_FRAMES,
            Self::HeaderMode => abi::V4L2_CID_MPEG_VIDEO_HEADER_MODE,
            Self::RateControl => abi::V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL,
            Self::Priority => abi::V4L2_CID_MPEG_VIDC_VIDEO_PRIORITY,
            Self::IdrPeriod => abi::V4L2_CID_MPEG_VIDC_VIDEO_IDR_PERIOD,
            Self::Level => abi::V4L2_CID_MPEG_VIDEO_H264_LEVEL,
            Self::LoopFilterMode => abi::V4L2_CID_MPEG_VIDEO_H264_LOOP_FILTER_MODE,
            Self::LoopFilterAlpha => abi::V4L2_CID_MPEG_VIDEO_H264_LOOP_FILTER_ALPHA,
            Self::LoopFilterBeta => abi::V4L2_CID_MPEG_VIDEO_H264_LOOP_FILTER_BETA,
            Self::SliceMode => abi::V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MODE,
            Self::SliceBytes => abi::V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MAX_BYTES,
            Self::SliceDelivery => abi::V4L2_CID_MPEG_VIDEO_MULTI_SLICE_DELIVERY_MODE,
            Self::RepeatHeader => abi::V4L2_CID_MPEG_VIDEO_REPEAT_SEQ_HEADER,
            Self::VuiTiming => abi::V4L2_CID_MPEG_VIDC_VIDEO_H264_VUI_TIMING_INFO,
            Self::VuiRestriction => abi::V4L2_CID_MPEG_VIDC_VIDEO_H264_VUI_BITSTREAM_RESTRICT,
            Self::Profile => abi::V4L2_CID_MPEG_VIDEO_H264_PROFILE,
            Self::Entropy => abi::V4L2_CID_MPEG_VIDEO_H264_ENTROPY_MODE,
            Self::CabacModel => abi::V4L2_CID_MPEG_VIDC_VIDEO_H264_CABAC_MODEL,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum EncoderValue {
    SeparateHeader,
    PriorityRealtime,
    PriorityNormal,
    LevelUnknown,
    SliceMaxBytes,
    SliceSingle,
    VuiTimingEnabled,
    VuiRestrictionEnabled,
    ProfileConstrainedBaseline,
    ProfileBaseline,
    ProfileHigh,
    EntropyCavlc,
    EntropyCabac,
    CabacModelZero,
}
impl EncoderValue {
    /// # Errors
    /// Rejects a vendor ABI value outside the kernel control's signed field.
    pub fn code(self) -> Result<i32, Error> {
        let value = match self {
            Self::SeparateHeader => abi::V4L2_MPEG_VIDEO_HEADER_MODE_SEPARATE,
            Self::PriorityRealtime => abi::V4L2_MPEG_VIDC_VIDEO_PRIORITY_REALTIME_ENABLE,
            Self::PriorityNormal => abi::V4L2_MPEG_VIDC_VIDEO_PRIORITY_REALTIME_DISABLE,
            Self::LevelUnknown => abi::V4L2_MPEG_VIDEO_H264_LEVEL_UNKNOWN,
            Self::SliceMaxBytes => abi::V4L2_MPEG_VIDEO_MULTI_SICE_MODE_MAX_BYTES,
            Self::SliceSingle => abi::V4L2_MPEG_VIDEO_MULTI_SLICE_MODE_SINGLE,
            Self::VuiTimingEnabled => abi::V4L2_MPEG_VIDC_VIDEO_H264_VUI_TIMING_INFO_ENABLED,
            Self::VuiRestrictionEnabled => {
                abi::V4L2_MPEG_VIDC_VIDEO_H264_VUI_BITSTREAM_RESTRICT_ENABLED
            }
            Self::ProfileConstrainedBaseline => {
                abi::V4L2_MPEG_VIDEO_H264_PROFILE_CONSTRAINED_BASELINE
            }
            Self::ProfileBaseline => abi::V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE,
            Self::ProfileHigh => abi::V4L2_MPEG_VIDEO_H264_PROFILE_HIGH,
            Self::EntropyCavlc => abi::V4L2_MPEG_VIDEO_H264_ENTROPY_MODE_CAVLC,
            Self::EntropyCabac => abi::V4L2_MPEG_VIDEO_H264_ENTROPY_MODE_CABAC,
            Self::CabacModelZero => abi::V4L2_CID_MPEG_VIDC_VIDEO_H264_CABAC_MODEL_0,
        };
        Ok(i32::try_from(value)?)
    }
}
impl Device {
    /// # Errors
    /// Preserves kernel errors enumerating multiplane formats except end-of-list EINVAL.
    pub fn formats(&self, queue: Queue) -> Result<Vec<u32>, Error> {
        let mut values = Vec::new();
        for index in 0..=u32::MAX {
            let mut value = abi::v4l2_fmtdesc {
                index,
                type_: queue.kind(),
                ..Default::default()
            };
            // SAFETY: ENUM_FMT writes one initialized matching format descriptor.
            match unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_ENUM_FMT, &mut value) } {
                Ok(()) => values.push(value.pixelformat),
                Err(Error::Io(error)) if error.raw_os_error() == Some(libc::EINVAL) => {
                    return Ok(values);
                }
                Err(error) => return Err(error),
            }
        }
        Err(Error::Contract("V4L format enumeration overflow"))
    }
    /// # Errors
    /// Preserves this typed encoder control's kernel error.
    pub fn encoder_control(&self, kind: EncoderControl, value: i32) -> Result<(), Error> {
        let mut control = abi::v4l2_control {
            id: kind.id(),
            value,
        };
        // SAFETY: S_CTRL reads its initialized matching kernel control structure.
        unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_S_CTRL, &mut control) }
    }
    /// # Errors
    /// Preserves the encoder-stop command error from the kernel.
    pub fn encoder_stop(&self) -> Result<(), Error> {
        let mut command = abi::v4l2_encoder_cmd {
            cmd: abi::V4L2_ENC_CMD_STOP,
            ..Default::default()
        };
        // SAFETY: ENCODER_CMD reads its initialized matching command structure.
        unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_ENCODER_CMD, &mut command) }
    }
}
impl super::Dequeued {
    #[must_use]
    pub fn codec_config(&self) -> bool {
        self.flags & abi::V4L2_QCOM_BUF_FLAG_CODECCONFIG != 0
    }
    #[must_use]
    pub fn keyframe(&self) -> bool {
        self.flags & abi::V4L2_BUF_FLAG_KEYFRAME != 0
    }
    #[must_use]
    pub fn eos(&self) -> bool {
        self.flags & abi::V4L2_QCOM_BUF_FLAG_EOS != 0
    }
}
