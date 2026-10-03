use openpilot_logging::{producer::Logger, Fields};
use openpilot_msgq::VisionStream;
use openpilot_params::Params;
use openpilot_selfdrived::{
    callbacks::{self, AlertParams},
    car_specific::CarSpecificParams,
    controller::{
        effects::{Effects, Personality, Publications},
        native_effects::NativeEffects,
        Error,
    },
};
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Effect {
    GetBool { key: String },
    GetInt { key: String },
    Get { key: String, return_default: bool },
    PutBool { key: String, value: bool },
    Remove { key: String },
    Offroad { key: String, extra: Option<String> },
    Event { name: String, fields: String },
    Monotonic { value: f64 },
    AvailableStreams,
}
pub struct Trace<'a> {
    pub directory: std::path::PathBuf,
    pub params: &'a Params,
    pub logger: &'a mut Logger,
    pub now: f64,
    pub streams: Vec<VisionStream>,
    pub effects: Vec<Effect>,
}
impl Trace<'_> {
    fn native(&mut self) -> NativeEffects<'_> {
        NativeEffects {
            params: self.params,
            logger: self.logger,
        }
    }
}
impl AlertParams for Trace<'_> {
    fn text(&mut self, key: &str) -> Result<Option<String>, callbacks::Error> {
        self.effects.push(Effect::Get {
            key: key.into(),
            return_default: false,
        });
        self.native().text(key)
    }
    fn integer(&mut self, key: &str) -> Result<i32, callbacks::Error> {
        self.effects.push(Effect::GetInt { key: key.into() });
        self.native().integer(key)
    }
    fn boolean(&mut self, key: &str) -> Result<bool, callbacks::Error> {
        self.effects.push(Effect::GetBool { key: key.into() });
        self.native().boolean(key)
    }
}
impl CarSpecificParams for Trace<'_> {
    type Error = callbacks::Error;
    fn get_bool(&mut self, key: &str) -> Result<bool, callbacks::Error> {
        self.boolean(key)
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), callbacks::Error> {
        self.effects.push(Effect::PutBool {
            key: key.into(),
            value,
        });
        self.native().put_bool(key, value)
    }
}
impl Effects for Trace<'_> {
    fn presence(&mut self, key: &str) -> Result<bool, Error> {
        self.effects.push(Effect::Get {
            key: key.into(),
            return_default: false,
        });
        self.native().presence(key)
    }
    fn wide_camera(&mut self) -> Result<bool, Error> {
        self.effects.push(Effect::Get {
            key: "UseWideCamera".into(),
            return_default: true,
        });
        self.native().wide_camera()
    }
    fn personality(&mut self) -> Result<Personality, Error> {
        self.effects.push(Effect::Get {
            key: "LongitudinalPersonality".into(),
            return_default: false,
        });
        self.native().personality()
    }
    fn remove(&mut self, key: &str) -> Result<(), Error> {
        self.effects.push(Effect::Remove { key: key.into() });
        self.native().remove(key)
    }
    fn offroad(&mut self, key: &str, extra: Option<&str>) -> Result<(), Error> {
        self.effects.push(Effect::Offroad {
            key: key.into(),
            extra: extra.map(str::to_owned),
        });
        self.native().offroad(key, extra)
    }
    fn event(&mut self, name: &str, fields: Fields) -> Result<(), Error> {
        self.effects.push(Effect::Event {
            name: name.into(),
            fields: fields.to_json()?,
        });
        self.native().event(name, fields)
    }
    fn monotonic(&mut self) -> f64 {
        self.effects.push(Effect::Monotonic { value: self.now });
        self.now
    }
    fn timestamp(&mut self) -> Result<u64, Error> {
        Ok((self.now * 1e9) as u64)
    }
    fn available_streams(&mut self) -> Result<Vec<VisionStream>, Error> {
        self.effects.push(Effect::AvailableStreams);
        Ok(self.streams.clone())
    }
}
#[derive(Default, Serialize)]
pub struct Messages(pub Vec<Message>);
#[derive(Serialize)]
pub struct Message {
    topic: String,
    bytes: Vec<u8>,
}
impl Publications for Messages {
    fn send(&mut self, topic: &str, bytes: &[u8]) -> Result<(), Error> {
        self.0.push(Message {
            topic: topic.into(),
            bytes: bytes.to_vec(),
        });
        Ok(())
    }
}
