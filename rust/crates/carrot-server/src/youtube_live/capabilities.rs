use super::rtmp_api::{Api, LoadError};
use crate::Value;
use ffmpeg_next as av;

pub(super) struct Capabilities {
    pub transport: Value,
    pub muxer: Value,
    pub ready: bool,
}
impl Capabilities {
    pub fn discover() -> Self {
        let failure = Api::load().err();
        let missing = match &failure {
            Some(LoadError::Missing(names)) => names.iter().map(|name| Value::text(name)).collect(),
            _ => Vec::new(),
        };
        let transport_error = failure.map(|error| error.to_string());
        let transport = Value::object([
            ("available", Value::Bool(transport_error.is_none())),
            ("path", Value::text("librtmp.so.1")),
            ("rtmps_mode", Value::text("rustls-tls-tunnel")),
            ("missing_symbols", Value::Array(missing)),
            (
                "error",
                Value::text(transport_error.as_deref().unwrap_or("")),
            ),
        ]);
        let initialized = av::init();
        let h264 = av::decoder::find(av::codec::Id::H264).is_some();
        let aac = av::encoder::find(av::codec::Id::AAC).is_some();
        let version = av::codec::version();
        let muxer = Value::object([
            ("available", Value::Bool(initialized.is_ok())),
            (
                "version",
                Value::text(&format!(
                    "FFmpeg {}.{}.{}",
                    version >> 16,
                    (version >> 8) & 255,
                    version & 255
                )),
            ),
            ("flv", Value::Bool(true)),
            ("h264", Value::Bool(h264)),
            ("aac", Value::Bool(aac)),
            (
                "error",
                Value::text(
                    &initialized
                        .err()
                        .map(|error| error.to_string())
                        .unwrap_or_default(),
                ),
            ),
        ]);
        let ready =
            transport.get("available").truth() && muxer.get("available").truth() && h264 && aac;
        Self {
            transport,
            muxer,
            ready,
        }
    }
}
