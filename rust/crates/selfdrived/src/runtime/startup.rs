use crate::{
    callbacks::{self, AlertParams},
    car_specific::CarSpecificParams,
    controller::{
        effects::{Effects, Personality, Publications},
        native_effects::NativeEffects,
        Error,
    },
};
use openpilot_logging::Fields;
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options,
};
use openpilot_msgq::{Subscriber, VisionStream};

pub struct Sockets {
    pub publisher: PubMaster,
    pub subscriber: SubMaster,
    pub car_state: Subscriber,
}
pub struct Startup<'a> {
    pub effects: NativeEffects<'a>,
    publisher: Option<PubMaster>,
    subscriber: Option<SubMaster>,
    car_state: Option<Subscriber>,
}
impl<'a> Startup<'a> {
    pub fn new(effects: NativeEffects<'a>) -> Self {
        Self {
            effects,
            publisher: None,
            subscriber: None,
            car_state: None,
        }
    }
    pub fn sockets(self) -> Result<Sockets, Error> {
        Ok(Sockets {
            publisher: self
                .publisher
                .ok_or(Error::Contract("publisher startup omitted"))?,
            subscriber: self
                .subscriber
                .ok_or(Error::Contract("subscriber startup omitted"))?,
            car_state: self
                .car_state
                .ok_or(Error::Contract("carState startup omitted"))?,
        })
    }
}
impl Publications for PubMaster {
    fn send(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        Ok(PubMaster::send(self, name, bytes)?)
    }
}
impl AlertParams for Startup<'_> {
    fn text(&mut self, key: &str) -> Result<Option<String>, callbacks::Error> {
        self.effects.text(key)
    }
    fn boolean(&mut self, key: &str) -> Result<bool, callbacks::Error> {
        self.effects.boolean(key)
    }
    fn integer(&mut self, key: &str) -> Result<i32, callbacks::Error> {
        self.effects.integer(key)
    }
}
impl CarSpecificParams for Startup<'_> {
    type Error = callbacks::Error;
    fn get_bool(&mut self, key: &str) -> Result<bool, callbacks::Error> {
        self.effects.get_bool(key)
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), callbacks::Error> {
        self.effects.put_bool(key, value)
    }
}
impl Effects for Startup<'_> {
    fn setup_publishers(&mut self) -> Result<(), Error> {
        self.publisher = Some(PubMaster::for_runtime(&["selfdriveState", "onroadEvents"])?);
        Ok(())
    }
    fn setup_subscribers(&mut self, names: &[&str], options: Options) -> Result<(), Error> {
        self.car_state = Some(Subscriber::for_runtime(
            "carState",
            false,
            openpilot_messaging::services::lookup("carState")
                .ok_or(Error::Contract("carState service"))?
                .queue_size,
        )?);
        self.subscriber = Some(SubMaster::for_runtime(names, options)?);
        Ok(())
    }
    fn presence(&mut self, key: &str) -> Result<bool, Error> {
        self.effects.presence(key)
    }
    fn wide_camera(&mut self) -> Result<bool, Error> {
        self.effects.wide_camera()
    }
    fn personality(&mut self) -> Result<Personality, Error> {
        self.effects.personality()
    }
    fn remove(&mut self, key: &str) -> Result<(), Error> {
        self.effects.remove(key)
    }
    fn offroad(&mut self, key: &str, extra: Option<&str>) -> Result<(), Error> {
        self.effects.offroad(key, extra)
    }
    fn event(&mut self, name: &str, fields: Fields) -> Result<(), Error> {
        self.effects.event(name, fields)
    }
    fn monotonic(&mut self) -> f64 {
        self.effects.monotonic()
    }
    fn timestamp(&mut self) -> Result<u64, Error> {
        self.effects.timestamp()
    }
    fn available_streams(&mut self) -> Result<Vec<VisionStream>, Error> {
        self.effects.available_streams()
    }
}
