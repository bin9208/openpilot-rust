use super::{Encoder, Result};
use openpilot_encoderd::native::v4l::primitives::{EncoderControl as C, EncoderValue as V};
impl Encoder {
    fn control(&self, kind: C, value: i32) -> Result<()> {
        self.device()?.encoder_control(kind, value)
    }
    fn optional(&self, kind: C, value: i32) -> bool {
        match self.control(kind, value) {
            Ok(()) => true,
            Err(error) => {
                if self.config.debug {
                    eprintln!("cluster H264 optional control {kind:?}: {error}");
                }
                false
            }
        }
    }
    pub(super) fn configure_controls(&self) -> Result<()> {
        for (kind, value) in [
            (C::Bitrate, self.config.bitrate),
            (C::PFrames, (self.config.gop - 1).max(0)),
            (C::BFrames, 0),
            (C::HeaderMode, V::SeparateHeader.code()?),
            (C::RateControl, self.config.rate_control),
            (
                C::Priority,
                if self.config.realtime_priority {
                    V::PriorityRealtime
                } else {
                    V::PriorityNormal
                }
                .code()?,
            ),
            (C::IdrPeriod, 1),
            (C::Level, V::LevelUnknown.code()?),
            (C::LoopFilterMode, 0),
            (C::LoopFilterAlpha, 0),
            (C::LoopFilterBeta, 0),
        ] {
            self.control(kind, value)?;
        }
        let max_bytes = V::SliceMaxBytes.code()?;
        if self.config.slice_max_bytes > 0 {
            let mut mode = self.optional(C::SliceMode, max_bytes);
            let mut bytes = self.optional(C::SliceBytes, self.config.slice_max_bytes);
            if !mode || !bytes {
                bytes = self.optional(C::SliceBytes, self.config.slice_max_bytes);
                mode = self.optional(C::SliceMode, max_bytes);
            }
            if mode && bytes {
                self.optional(C::SliceDelivery, 1);
            } else {
                self.optional(C::SliceMode, V::SliceSingle.code()?);
            }
        } else {
            self.optional(C::SliceMode, V::SliceSingle.code()?);
        }
        self.optional(C::RepeatHeader, 1);
        self.optional(C::VuiTiming, V::VuiTimingEnabled.code()?);
        self.optional(C::VuiRestriction, V::VuiRestrictionEnabled.code()?);
        let baseline = self.optional(C::Profile, V::ProfileConstrainedBaseline.code()?)
            || self.optional(C::Profile, V::ProfileBaseline.code()?);
        let low_complexity = baseline && self.optional(C::Entropy, V::EntropyCavlc.code()?);
        if !low_complexity {
            self.control(C::Profile, V::ProfileHigh.code()?)?;
            self.control(C::Entropy, V::EntropyCabac.code()?)?;
            self.control(C::CabacModel, V::CabacModelZero.code()?)?;
        }
        Ok(())
    }
}
