use super::{read_log_bytes, Error, Frame, Metadata};
use crate::{
    live::{projection::Data, Projection},
    runtime_args::CornerSource,
};
use capnp::{dynamic_value::Reader, message::ReaderOptions, serialize};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;
use std::{collections::BTreeMap, io::Cursor, path::Path};

pub struct Parser {
    pub(super) projection: Projection,
    metadata: Metadata,
    car_brand: String,
    pub corner_source: CornerSource,
    pub(super) raw_tracker: super::corner::StableTracker,
    pub(super) raw_objects: BTreeMap<(String, u32), super::corner::RawObject>,
}
impl Parser {
    pub fn set_front_radar_only(&mut self, enabled: bool) {
        self.projection.set_front_radar_only(enabled);
    }
    #[must_use]
    pub fn new(corner_source: CornerSource) -> Self {
        Self {
            projection: Projection::default(),
            metadata: Metadata::parser_initial(),
            car_brand: String::new(),
            corner_source,
            raw_tracker: super::corner::StableTracker::default(),
            raw_objects: BTreeMap::new(),
        }
    }
    /// # Errors
    /// Returns file decoding or malformed Cereal message errors.
    pub fn parse_file(&mut self, path: &Path) -> Result<Vec<Frame>, Error> {
        self.parse_bytes(&read_log_bytes(path)?)
    }
    /// # Errors
    /// Returns malformed Cereal messages or unrepresentable message timestamps.
    pub fn parse_bytes(&mut self, bytes: &[u8]) -> Result<Vec<Frame>, Error> {
        if self.car_brand.is_empty() {
            visit(bytes, |event| {
                if matches!(event.which(), Ok(event::CarParams(_))) {
                    self.apply(event)?;
                    return Ok(false);
                }
                Ok(true)
            })?;
        }
        let mut frames = Vec::new();
        visit(bytes, |event| {
            if let Ok(event::CarState(car)) = event.which() {
                let time = event
                    .get_log_mono_time()
                    .to_f64()
                    .ok_or_else(|| Error::Invalid("invalid log timestamp".into()))?
                    / 1e9;
                let mut state = self.projection.car_frame(car?.into(), time);
                let points = match self.corner_source {
                    CornerSource::Live => Vec::new(),
                    CornerSource::Stable => self.raw_tracker.points_at(time, state.speed_kph),
                    CornerSource::Raw => {
                        self.raw_objects.retain(|_, object| time - object.t <= 0.55);
                        super::corner::sorted_points(
                            self.raw_objects
                                .values()
                                .map(|object| object.point(state.speed_kph)),
                        )
                    }
                };
                if !points.is_empty() {
                    state.radar_points = points;
                }
                let (lanes, initial) = self.projection.replay_geometry();
                let mut metadata = self.metadata.clone();
                metadata.recenter_start = initial;
                frames.push(Frame {
                    t: time,
                    state,
                    lanes,
                    metadata,
                });
            } else {
                self.apply(event)?;
            }
            Ok(true)
        })?;
        Ok(frames)
    }
    fn apply(&mut self, event: event::Reader<'_>) -> Result<(), Error> {
        let Reader::Struct(root) = event.into() else {
            return Ok(());
        };
        let Ok(Some(field)) = root.which() else {
            return Ok(());
        };
        let name = field
            .get_proto()
            .get_name()?
            .to_str()
            .map_err(|error| Error::Invalid(error.to_string()))?;
        let data = Data(root.get(field)?);
        let time = event
            .get_log_mono_time()
            .to_f64()
            .ok_or_else(|| Error::Invalid("invalid log timestamp".into()))?
            / 1e9;
        if name == "carParams" {
            self.car_brand = data
                .field("brand")
                .text()
                .unwrap_or_default()
                .to_lowercase();
        }
        if matches!(name, "can" | "sendcan") {
            self.can(data, time, name)?;
        }
        self.projection
            .apply_data(name, data.0, time, event.get_valid());
        self.metadata.observe(name, data, &self.projection);
        Ok(())
    }
}
fn visit(
    bytes: &[u8],
    mut callback: impl FnMut(event::Reader<'_>) -> Result<bool, Error>,
) -> Result<(), Error> {
    let mut cursor = Cursor::new(bytes);
    let length = u64::try_from(bytes.len()).map_err(|error| Error::Invalid(error.to_string()))?;
    while cursor.position() < length {
        let message = serialize::read_message(&mut cursor, ReaderOptions::new())?;
        if !callback(message.get_root::<event::Reader<'_>>()?)? {
            break;
        }
    }
    Ok(())
}
