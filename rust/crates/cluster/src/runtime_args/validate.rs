use super::{Codec, H264Backend, InputSource, ParseError, RuntimeArgs};
fn checked(values: &[(bool, &str)]) -> Result<(), ParseError> {
    for &(invalid, message) in values {
        if invalid {
            return Err(ParseError::Invalid(message.into()));
        }
    }
    Ok(())
}
impl RuntimeArgs {
    pub(super) fn validate(&self) -> Result<(), ParseError> {
        self.validate_general()?;
        self.validate_h264()?;
        self.validate_runtime()
    }
    fn validate_general(&self) -> Result<(), ParseError> {
        checked(&[
            (self.fps < 0.0, "--fps must be 0 or greater"),
            (
                !(1..=65535).contains(&self.navi_port),
                "--navi-port must be between 1 and 65535",
            ),
            (
                self.navi.navi_publish_cereal
                    && self.input != InputSource::Navi
                    && !self.navi.navi_overlay,
                "--navi-publish-cereal requires --input navi or --navi-overlay",
            ),
            (
                self.width.is_some_and(|v| v <= 0) || self.height.is_some_and(|v| v <= 0),
                "--width and --height must be greater than 0",
            ),
            (
                self.usb_brightness.is_some_and(|v| !(0..=100).contains(&v)),
                "--usb-brightness must be between 0 and 100",
            ),
            (
                self.usb_display_fps
                    .is_some_and(|v| !(0..=255).contains(&v)),
                "--usb-display-fps must be between 0 and 255",
            ),
            (
                !(1..=95).contains(&self.usb_jpeg_quality),
                "--usb-jpeg-quality must be between 1 and 95",
            ),
            (
                self.transport.usb_async && self.usb_codec != Codec::Jpeg,
                "--usb-async only supports --usb-codec jpeg",
            ),
            (
                self.usb_h264_fps <= 0,
                "--usb-h264-fps must be greater than 0",
            ),
            (
                self.usb_h264_gop <= 0,
                "--usb-h264-gop must be greater than 0",
            ),
            (
                self.usb_h264_chunk_size < 0,
                "--usb-h264-chunk-size must be 0 or greater",
            ),
        ])
    }
    fn validate_h264(&self) -> Result<(), ParseError> {
        checked(&[
            (
                self.usb_h264_slice_max_bytes < 0,
                "--usb-h264-slice-max-bytes must be 0 or greater",
            ),
            (
                self.usb_h264_align <= 0,
                "--usb-h264-align must be greater than 0",
            ),
            (
                self.usb_h264_encoder_align <= 0,
                "--usb-h264-encoder-align must be greater than 0",
            ),
            (
                self.ack.usb_h264_wait_ack && self.ack.usb_h264_no_ack,
                "--usb-h264-wait-ack and --usb-h264-no-ack cannot be used together",
            ),
            (
                self.ack.usb_h264_soft_ack && self.ack.usb_h264_no_ack,
                "--usb-h264-soft-ack and --usb-h264-no-ack cannot be used together",
            ),
            (
                self.usb_h264_bitrate.trim().is_empty(),
                "--usb-h264-bitrate must not be empty",
            ),
            (
                self.usb_h264_diagnose_interval < 0.0,
                "--usb-h264-diagnose-interval must be 0 or greater",
            ),
            (
                self.patterns.usb_h264_test_pattern && self.patterns.usb_h264_test_pattern_nv12,
                "--usb-h264-test-pattern and --usb-h264-test-pattern-nv12 cannot be used together",
            ),
            (
                self.patterns.usb_h264_test_pattern_nv12
                    && self.usb_h264_backend == H264Backend::Ffmpeg,
                "--usb-h264-test-pattern-nv12 requires --usb-h264-backend native or auto",
            ),
            (
                self.usb_h264_library.is_empty(),
                "--usb-h264-library must not be empty",
            ),
            (
                self.usb_h264_ffmpeg.is_empty(),
                "--usb-h264-ffmpeg must not be empty",
            ),
        ])
    }
    fn validate_runtime(&self) -> Result<(), ParseError> {
        checked(&[
            (
                self.usb_h264_ffmpeg_encoder.is_empty(),
                "--usb-h264-ffmpeg-encoder must not be empty",
            ),
            (
                self.usb_h264_device.is_empty(),
                "--usb-h264-device must not be empty",
            ),
            (
                self.usb_frame_drain_attempts < 0 || self.usb_fast_drain_attempts < 0,
                "USB drain attempts must be 0 or greater",
            ),
            (
                self.usb_frame_drain_timeout_ms < 0 || self.usb_fast_drain_timeout_ms < 0,
                "USB drain timeouts must be 0 or greater",
            ),
            (
                self.input == InputSource::Route && self.route_replay_speed <= 0.0,
                "--route-replay-speed must be greater than 0",
            ),
            (
                self.route_start_segment.is_some_and(|v| v < 0),
                "--route-start-segment must be 0 or greater",
            ),
            (
                self.route_start_time < 0.0,
                "--route-start-time must be 0 or greater",
            ),
            (
                self.route_max_segments.is_some_and(|v| v <= 0),
                "--route-max-segments must be greater than 0",
            ),
            (
                self.profile_interval <= 0.0,
                "--profile-interval must be greater than 0",
            ),
        ])
    }
}
