/// Original transport defaults from `TuringUsbDisplay.__init__`.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub brightness: i32,
    pub display_fps: i32,
    pub fast_write: bool,
    pub wait_for_frame_ack: bool,
    pub frame_drain_attempts: i32,
    pub frame_drain_timeout_ms: i32,
    pub fast_frame_drain_attempts: i32,
    pub fast_frame_drain_timeout_ms: i32,
    pub expected_product_id: Option<u16>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            brightness: 80,
            display_fps: 60,
            fast_write: false,
            wait_for_frame_ack: false,
            frame_drain_attempts: 2,
            frame_drain_timeout_ms: 2,
            fast_frame_drain_attempts: 3,
            fast_frame_drain_timeout_ms: 2,
            expected_product_id: None,
        }
    }
}
