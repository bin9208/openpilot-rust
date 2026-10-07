use crate::{
    controller::{Controller, Input, Options},
    wire, Error,
};
use num_traits::ToPrimitive;
use openpilot_cereal::{car_capnp, log_capnp};
use openpilot_messaging::state::{self, Poll, State};

pub const SERVICES: [&str; 4] = ["modelV2", "carState", "liveTracks", "livePose"];
pub fn options() -> state::Options {
    state::Options {
        poll: Poll::One("modelV2".into()),
        ignore_alive: vec!["livePose".into()],
        ignore_valid: vec!["livePose".into()],
        ..state::Options::default()
    }
}

pub struct Radar {
    pub controller: Controller,
}
impl Radar {
    pub fn new(options: Options) -> Self {
        Self {
            controller: Controller::new(options),
        }
    }
    pub fn process(&mut self, state: &State, now: u64) -> Result<Option<Vec<u8>>, Error> {
        if !state.topic("modelV2")?.updated {
            return Ok(None);
        }
        let views = Views(state);
        let model = views.model()?;
        let car = views.car()?;
        let tracks = views.tracks()?;
        let model_ns = state.topic("modelV2")?.log_mono_time;
        let car_ns = state.topic("carState")?.log_mono_time;
        let eof = model.get_timestamp_eof();
        let measured = if eof > 0 { eof } else { model_ns };
        let model_time = seconds(measured)?;
        let time = if model_time > 0. {
            model_time
        } else {
            seconds(
                state
                    .topics()
                    .iter()
                    .map(|topic| topic.log_mono_time)
                    .max()
                    .unwrap_or(0),
            )?
        };
        let output = self.controller.update(&Input {
            time_s: time,
            v_ego: f64::from(car.get_v_ego()),
            points: wire::points(tracks)?,
            model: wire::model(model)?,
            yaw_rate_rad_s: wire::yaw(views.pose()?)?,
            radar_to_model_time_s: model_time - seconds(state.topic("liveTracks")?.log_mono_time)?,
        })?;
        Ok(Some(wire::publication(
            &output,
            tracks.get_errors()?,
            model_ns,
            car_ns,
            state.all_checks(&SERVICES)?,
            now,
        )?))
    }
}
fn seconds(value: u64) -> Result<f64, Error> {
    Ok(value
        .to_f64()
        .ok_or(Error::Contract("message timestamp range"))?
        * 1e-9)
}
struct Views<'a>(&'a State);
macro_rules! view {
    ($method:ident,$topic:literal,$variant:ident,$reader:ty) => {
        fn $method(&self) -> Result<$reader, Error> {
            match self.0.topic($topic)?.event()?.which()? {
                log_capnp::event::$variant(value) => Ok(value?),
                _ => Err(Error::Contract("radard service/event mismatch")),
            }
        }
    };
}
impl<'a> Views<'a> {
    view!(
        model,
        "modelV2",
        ModelV2,
        log_capnp::model_data_v2::Reader<'a>
    );
    view!(car, "carState", CarState, car_capnp::car_state::Reader<'a>);
    view!(
        tracks,
        "liveTracks",
        LiveTracks,
        car_capnp::radar_data::Reader<'a>
    );
    view!(pose, "livePose", LivePose, log_capnp::live_pose::Reader<'a>);
}
