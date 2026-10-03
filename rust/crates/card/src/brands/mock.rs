use crate::{
    core::{ApplyInput, ApplyOutput, Error, Message, Vehicle},
    firmware::Firmware,
    firmware_query::StartupIo,
    vehicle_params::{self, FinishOptions},
};
use openpilot_can::Packet;
use openpilot_cereal::{
    car_capnp::{car_params, car_state},
    log_capnp::event,
};
use openpilot_messaging::state::{Options, State};
use openpilot_params::Params;

const GPS: [&str; 2] = ["gpsLocation", "gpsLocationExternal"];

pub fn parameters(
    candidate: &str,
    firmware: &[Firmware],
    settings: &Params,
) -> Result<Message, Error> {
    let mut message = vehicle_params::baseline(candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("mock");
    cp.set_mass(1700.);
    cp.set_wheelbase(2.70);
    let wheelbase = cp.reborrow_as_reader().get_wheelbase();
    cp.set_center_to_front(wheelbase * 0.5);
    cp.set_steer_ratio(13.);
    cp.set_dashcam_only(true);
    vehicle_params::finish(cp, settings, FinishOptions { firmware })?;
    Ok(message)
}

pub struct Mock {
    gps: State,
    out: Message,
    #[cfg(feature = "native")]
    source: Option<openpilot_messaging::runtime::SubMaster>,
}
impl Mock {
    pub fn new() -> Result<Self, Error> {
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            gps: State::new(&GPS, Options::default())?,
            out,
            #[cfg(feature = "native")]
            source: None,
        })
    }
    pub fn update_gps(&mut self, now: f64, messages: &[Vec<u8>]) -> Result<(), Error> {
        Ok(self.gps.update(now, messages)?)
    }
    #[cfg(feature = "native")]
    pub fn connect_runtime(&mut self) -> Result<(), Error> {
        self.source = Some(openpilot_messaging::runtime::SubMaster::for_runtime(
            &GPS,
            Options::default(),
        )?);
        Ok(())
    }
}
impl Vehicle for Mock {
    fn update(&mut self, _: &[Packet], _: u64) -> Result<Message, Error> {
        #[cfg(feature = "native")]
        if let Some(source) = &mut self.source {
            source.update(std::time::Duration::ZERO)?;
        }
        #[cfg(feature = "native")]
        let gps = self
            .source
            .as_ref()
            .map_or(&self.gps, |source| &source.state);
        #[cfg(not(feature = "native"))]
        let gps = &self.gps;
        let external = gps.topic("gpsLocationExternal")?.receive_frame > 1;
        let location = gps
            .topic(if external {
                "gpsLocationExternal"
            } else {
                "gpsLocation"
            })?
            .event()?;
        let speed = match location.which()? {
            event::Which::GpsLocation(location) if !external => location?.get_speed(),
            event::Which::GpsLocationExternal(location) if external => location?.get_speed(),
            _ => return Err(Error::Event("GPS")),
        };
        let mut message = Message::new_default();
        let mut state = message.init_root::<car_state::Builder>();
        state.set_can_valid(true);
        state.set_v_ego(speed);
        state.set_v_ego_raw(speed);
        state.reborrow().init_cruise_state();
        self.out.set_root(state.into_reader())?;
        Ok(message)
    }
    fn init(&mut self, _: &mut impl StartupIo) -> Result<(), Error> {
        Ok(())
    }
    fn apply(&mut self, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let mut output = Message::new_default();
        output.set_root(input.control.get_actuators()?)?;
        Ok(ApplyOutput {
            actuators: output,
            can: vec![],
        })
    }
    fn commit_state(&mut self, state: car_state::Reader<'_>) -> Result<(), Error> {
        self.out.set_root(state)?;
        Ok(())
    }
    fn set_soft_hold(&mut self, _: i16) {}
}
