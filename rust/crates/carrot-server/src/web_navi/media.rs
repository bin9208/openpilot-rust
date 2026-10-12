use super::wire;
use crate::{Error, Value};
use capnp::message::ReaderOptions;
use openpilot_cereal::log_capnp::event;

pub(super) struct Media {
    pub metadata: Value,
    pub payload: Vec<u8>,
    pub raw_wire: Vec<u8>,
    pub kind: String,
    pub name: String,
    pub session: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub present: bool,
    pub message_type: u8,
    pub flags: u16,
    pub dimensions: (u32, u32),
}
impl Media {
    pub fn is_map(&self) -> bool {
        self.kind == "render" && self.name == "map_main"
    }
    pub fn keyframe(&self) -> bool {
        self.flags & 1 != 0
    }
}
pub(super) fn parse(bytes: &[u8]) -> Result<Option<Media>, Error> {
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(None);
    let message = capnp::serialize::read_message(&mut std::io::Cursor::new(bytes), options)
        .map_err(|error| Error::Source(error.to_string()))?;
    let root: event::Reader<'_> = message
        .get_root()
        .map_err(|error| Error::Source(error.to_string()))?;
    let event::Which::CarrotNaviMedia(data) = root
        .which()
        .map_err(|error| Error::Source(error.to_string()))?
    else {
        return Err(Error::Source("expected CarrotNaviMedia event".into()));
    };
    let data = data.map_err(|error| Error::Source(error.to_string()))?;
    let read = || -> Result<Option<Media>, capnp::Error> {
        let kind = data.get_kind()?.to_str()?;
        let name = data.get_name()?.to_str()?;
        if !(matches!(kind, "image" | "web_image")
            || (matches!(kind, "render" | "web_render") && name == "map_main"))
        {
            return Ok(None);
        }
        let bootstrap = matches!(kind, "web_render" | "web_image");
        let kind = match kind {
            "web_render" => "render",
            "web_image" => "image",
            value => value,
        }
        .to_owned();
        let session = data.get_session_id()?.to_str()?.to_owned();
        let payload = data.get_payload()?.to_vec();
        let dimensions = (u32::from(data.get_width()), u32::from(data.get_height()));
        let metadata = Value::object([
            ("type", Value::text("carrotNaviMedia")),
            ("version", Value::integer(1)),
            ("sessionId", Value::text(&session)),
            ("kind", Value::text(&kind)),
            ("name", Value::text(name)),
            ("sequence", Value::integer(data.get_sequence())),
            (
                "sourceTimestampMillis",
                Value::integer(data.get_source_timestamp_millis()),
            ),
            (
                "receivedMonoTimeNanos",
                Value::integer(data.get_received_mono_time_nanos()),
            ),
            ("present", Value::Bool(data.get_present())),
            ("messageType", Value::integer(data.get_message_type())),
            (
                "formatOrReason",
                Value::integer(data.get_format_or_reason()),
            ),
            ("flags", Value::integer(data.get_flags())),
            ("width", Value::integer(dimensions.0)),
            ("height", Value::integer(dimensions.1)),
            ("reason", Value::text(data.get_reason()?.to_str()?)),
            ("bootstrap", Value::Bool(bootstrap)),
        ]);
        Ok(Some(Media {
            metadata,
            payload,
            raw_wire: Vec::new(),
            kind,
            name: name.into(),
            session,
            sequence: data.get_sequence(),
            timestamp_ms: data.get_source_timestamp_millis(),
            present: data.get_present(),
            message_type: data.get_message_type(),
            flags: data.get_flags(),
            dimensions,
        }))
    };
    let Some(mut media) = read().map_err(|error| Error::Source(error.to_string()))? else {
        return Ok(None);
    };
    media.raw_wire = wire::frame(&media.metadata, &media.payload)?;
    Ok(Some(media))
}
