//! OEM/vision provenance is display-only and never changes the merged warning.
use super::{
    boolean, field, timestamp, Error, JsonValue, JsonView, Nanoseconds, BLINDSPOT_TIMEOUT_NS,
};
use openpilot_cereal::{car_capnp::car_state, log_capnp::event};
use openpilot_messaging::state::State;
#[derive(Debug)]
pub struct Sources {
    pub sides: [(bool, bool); 2],
    received: Nanoseconds,
}
impl Sources {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let data = JsonValue::parse(&super::encoding::decode(payload)?)?;
        let version = data.get("version").is_some_and(|v| match v.view() {
            JsonView::Integer(v) => v == "1",
            JsonView::Float(v) => v == 1.,
            JsonView::Bool(v) => v,
            _ => false,
        });
        if !data.is_object()
            || !data
                .get("type")
                .is_some_and(|v| v.text_eq("xiaogeBlindspotSources"))
            || !version
        {
            return Err(Error::Field("xiaogeBlindspotSources"));
        }
        let left = field(&data, "left")?;
        let right = field(&data, "right")?;
        if !left.is_object() || !right.is_object() {
            return Err(Error::Field("left/right object"));
        }
        Ok(Self {
            sides: [
                (
                    boolean(field(&left, "oem")?)?,
                    boolean(field(&left, "vision")?)?,
                ),
                (
                    boolean(field(&right, "oem")?)?,
                    boolean(field(&right, "vision")?)?,
                ),
            ],
            received: timestamp(field(&data, "receivedMonoTimeNanos")?)?,
        })
    }
    pub fn fresh(&self, now: i128) -> bool {
        // Source packet accepts timestamp zero during the initial timeout window.
        self.received
            .decimal()
            .parse::<i128>()
            .ok()
            .and_then(|stamp| now.checked_sub(stamp))
            .is_some_and(|age| (0..=BLINDSPOT_TIMEOUT_NS).contains(&age))
    }
}
pub fn packet(messages: &State, now: i128) -> Option<Sources> {
    let topic = messages.topic("customReservedRawData1").ok()?;
    if !topic.valid {
        return None;
    }
    let event::Which::CustomReservedRawData1(data) = topic.event().ok()?.which().ok()? else {
        return None;
    };
    let packet = Sources::parse(data.ok()?).ok()?;
    packet.fresh(now).then_some(packet)
}
pub fn sides(car: car_state::Reader<'_>, packet: Option<&Sources>) -> [(bool, bool); 2] {
    if car.get_blindspot_split_sources_valid() {
        [
            (car.get_left_blindspot_oem(), car.get_left_blindspot_onnx()),
            (
                car.get_right_blindspot_oem(),
                car.get_right_blindspot_onnx(),
            ),
        ]
    } else if let Some(packet) = packet {
        packet.sides
    } else {
        [
            (car.get_left_blindspot(), false),
            (car.get_right_blindspot(), false),
        ]
    }
}
pub fn tint(sources: (bool, bool)) -> u32 {
    match sources {
        (true, true) => crate::paint::color(255, 128, 128, 190),
        (_, true) => crate::paint::color(64, 156, 255, 165),
        _ => crate::paint::color(255, 215, 0, 150),
    }
}
