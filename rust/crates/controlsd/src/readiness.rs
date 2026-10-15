use crate::Error;
use openpilot_cereal::log_capnp::{event, onroad_event::EventName};
use openpilot_control_policy::lateral_readiness::{Service, Services, State as StartupState};
use openpilot_messaging::state::State;

fn service(state: &State, name: &str) -> Result<Service, Error> {
    let topic = state.topic(name)?;
    Ok(Service {
        seen: topic.seen,
        valid: topic.valid,
        alive: topic.alive,
        checks: state.all_checks(&[name])?,
    })
}
pub fn decode(state: &State) -> Result<StartupState, Error> {
    let event::LivePose(pose) = state.topic("livePose")?.event()?.which()? else {
        return Err(Error::Contract("unexpected livePose"));
    };
    let pose = pose?;
    let event::OnroadEvents(events) = state.topic("onroadEvents")?.event()?.which()? else {
        return Err(Error::Contract("unexpected onroadEvents"));
    };
    Ok(StartupState {
        services: Services {
            boundary: service(state, "carState")?,
            model: service(state, "modelV2")?,
            live: service(state, "liveParameters")?,
            pose: service(state, "livePose")?,
            selfdrive: service(state, "selfdriveState")?,
            events: service(state, "onroadEvents")?,
            lane: service(state, "lateralPlan")?,
        },
        pose: [
            pose.get_inputs_o_k(),
            pose.get_posenet_o_k(),
            pose.get_sensors_o_k(),
        ],
        initializing: events?
            .iter()
            .any(|event| event.get_name() == Ok(EventName::SelfdriveInitializing)),
    })
}
