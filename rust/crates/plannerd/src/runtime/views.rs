use crate::Error;
use openpilot_cereal::{car_capnp, custom_capnp, log_capnp};
use openpilot_messaging::state::State;

pub struct Views<'a>(pub &'a State);
macro_rules! view {
    ($method:ident, $topic:literal, $variant:ident, $reader:ty) => {
        pub fn $method(&self) -> Result<$reader, Error> {
            match self.0.topic($topic)?.event()?.which()? {
                log_capnp::event::$variant(value) => Ok(value?),
                _ => Err(Error::Contract("planner service/event mismatch")),
            }
        }
    };
}
impl<'a> Views<'a> {
    view!(car, "carState", CarState, car_capnp::car_state::Reader<'a>);
    view!(
        control,
        "carControl",
        CarControl,
        car_capnp::car_control::Reader<'a>
    );
    view!(
        controls,
        "controlsState",
        ControlsState,
        log_capnp::controls_state::Reader<'a>
    );
    view!(
        model,
        "modelV2",
        ModelV2,
        log_capnp::model_data_v2::Reader<'a>
    );
    view!(
        radar,
        "radarState",
        RadarState,
        log_capnp::radar_state::Reader<'a>
    );
    view!(
        tracks,
        "liveTracks",
        LiveTracks,
        car_capnp::radar_data::Reader<'a>
    );
    view!(
        selfdrive,
        "selfdriveState",
        SelfdriveState,
        log_capnp::selfdrive_state::Reader<'a>
    );
    view!(
        navigation,
        "carrotMan",
        CarrotMan,
        custom_capnp::carrot_man::Reader<'a>
    );
    view!(pose, "livePose", LivePose, log_capnp::live_pose::Reader<'a>);

    pub fn available(&self, names: &[&str]) -> Result<bool, Error> {
        for name in names {
            let topic = self.0.topic(name)?;
            if !topic.valid || !topic.alive {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
