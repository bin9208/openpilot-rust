use super::RuntimeArgs;
use serde::Serialize;

macro_rules! flag_groups {
    ($($group:ident : $name:ident { $($field:ident => $option:literal),+ $(,)? }),+ $(,)?) => {
        $(#[derive(Clone, Debug, Default, Serialize)]
        pub struct $name { $(pub $field: bool),+ })+
        pub(super) const OPTIONS: &[&str] = &[$($($option),+),+];
        pub(super) fn apply(args: &mut RuntimeArgs, option: &str) {
            match option {
                $($($option => args.$group.$field = true),+,)+
                _ => unreachable!("resolved flag"),
            }
        }
    };
}
flag_groups! {
    navi: NaviFlags { navi_no_beacon => "--navi-no-beacon", navi_overlay => "--navi-overlay", navi_publish_cereal => "--navi-publish-cereal" },
    transport: TransportFlags { usb_fast => "--usb-fast", usb_wait_frame_ack => "--usb-wait-frame-ack", usb_async => "--usb-async" },
    ack: AckFlags { usb_h264_no_ack => "--usb-h264-no-ack", usb_h264_wait_ack => "--usb-h264-wait-ack", usb_h264_soft_ack => "--usb-h264-soft-ack" },
    patterns: PatternFlags { usb_h264_test_pattern => "--usb-h264-test-pattern", usb_h264_test_pattern_nv12 => "--usb-h264-test-pattern-nv12" },
    replay: ReplayFlags { route_loop => "--route-loop", route_pause_on_cutin => "--route-pause-on-cutin" },
    live: LiveFlags { live_no_can => "--live-no-can", live_include_can => "--live-include-can" },
    diagnostics: DiagnosticFlags { cluster_core_usage_debug => "--cluster-core-usage-debug", profile_render => "--profile-render", usb_h264_debug => "--usb-h264-debug" },
    runtime: RuntimeFlags { no_cluster_core_usage => "--no-cluster-core-usage", no_gc_freeze => "--no-gc-freeze", usb_h264_realtime_priority => "--usb-h264-realtime-priority" },
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct RatePresence {
    pub fps_from_cli: bool,
    pub usb_brightness_from_cli: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct EncoderPresence {
    pub usb_codec_from_cli: bool,
    pub usb_h264_backend_from_cli: bool,
    pub usb_h264_ffmpeg_encoder_from_cli: bool,
}
