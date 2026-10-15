use super::{input, Error, Message};
use openpilot_cereal::{
    car_capnp::{car_control, car_state},
    log_capnp::{event, onroad_event::EventName},
};
use openpilot_control_policy::lateral_readiness::{
    Car, Gate, Inputs, Lane, LiveParameters, Service, Services, State as StartupState,
};
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
macro_rules! value {
    ($state:expr,$name:literal,$variant:ident) => {
        match $state.topic($name)?.event()?.which()? {
            event::$variant(value) => value?,
            _ => return Err(Error::Event($name)),
        }
    };
}
fn update(gate: &mut Gate, state: &State, car: car_state::Reader<'_>) -> Result<bool, Error> {
    if gate.ready {
        return Ok(true);
    }
    let services = Services {
        boundary: service(state, "carControl")?,
        model: service(state, "modelV2")?,
        live: service(state, "liveParameters")?,
        pose: service(state, "livePose")?,
        selfdrive: service(state, "selfdriveState")?,
        events: service(state, "onroadEvents")?,
        lane: service(state, "lateralPlan")?,
    };
    if !services.boundary.ready()
        || ![
            services.model,
            services.live,
            services.pose,
            services.selfdrive,
        ]
        .into_iter()
        .all(Service::ready)
        || !services.events.events_ready()
    {
        return Ok(false);
    }
    let lp = value!(state, "liveParameters", LiveParameters);
    let live = LiveParameters {
        ratio: f64::from(lp.get_steer_ratio()),
        stiffness: f64::from(lp.get_stiffness_factor()),
        offset: f64::from(lp.get_angle_offset_deg()),
        roll: f64::from(lp.get_roll()),
        valid: lp.get_valid(),
        sensor_valid: lp.get_sensor_valid(),
        posenet_valid: lp.get_posenet_valid(),
    };
    if !live.ready(services.live) {
        return Ok(false);
    }
    let pose = value!(state, "livePose", LivePose);
    let startup = StartupState {
        services,
        pose: [
            pose.get_inputs_o_k(),
            pose.get_posenet_o_k(),
            pose.get_sensors_o_k(),
        ],
        initializing: value!(state, "onroadEvents", OnroadEvents)
            .iter()
            .any(|e| e.get_name() == Ok(EventName::SelfdriveInitializing)),
    };
    let plan = value!(state, "lateralPlan", LateralPlan);
    let use_lane_lines = plan.get_use_lane_lines();
    if use_lane_lines && (!services.lane.ready() || !plan.get_mpc_solution_valid()) {
        return Ok(false);
    }
    let floats = |list: capnp::primitive_list::Reader<'_, f32>| {
        list.iter().map(f64::from).collect::<Vec<_>>()
    };
    let (psis, curvatures, distances) = if use_lane_lines {
        (
            floats(plan.get_psis()?),
            floats(plan.get_curvatures()?),
            floats(plan.get_distances()?),
        )
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    Ok(gate.update(Inputs {
        state: &startup,
        live: &live,
        car: Car {
            can_valid: car.get_can_valid(),
            fault_temporary: car.get_steer_fault_temporary(),
            fault_permanent: car.get_steer_fault_permanent(),
            values: [
                car.get_v_ego(),
                car.get_steering_angle_deg(),
                car.get_steering_rate_deg(),
                car.get_steering_torque(),
            ]
            .map(f64::from),
        },
        desired_curvature: f64::from(
            value!(state, "modelV2", ModelV2)
                .get_action()?
                .get_desired_curvature(),
        ),
        lane: Lane {
            use_lane_lines,
            solution_valid: plan.get_mpc_solution_valid(),
            psis: &psis,
            curvatures: &curvatures,
            distances: &distances,
        },
    }))
}

/// An independent final application check protects against queued pre-startup commands.
pub(super) fn guard(
    gate: &mut Gate,
    state: &State,
    car: car_state::Reader<'_>,
) -> Result<Option<Message>, Error> {
    let ready = update(gate, state, car)?;
    let control = input::control(state)?;
    if !control.get_lat_active() || ready {
        return Ok(None);
    }
    let mut message = Message::new_default();
    message.set_root(control)?;
    let mut control = message.get_root::<car_control::Builder>()?;
    control.set_lat_active(false);
    let mut actuators = control.get_actuators()?;
    actuators.set_torque(0.);
    actuators.set_steering_angle_deg(car.get_steering_angle_deg());
    actuators.set_curvature(0.);
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_messaging::state::Options;
    fn packet(name: &str) -> Vec<u8> {
        let mut message = Message::new_default();
        let mut event = message.init_root::<event::Builder>();
        event.set_valid(true);
        match name {
            "carControl" => {
                let mut cc = event.init_car_control();
                cc.set_enabled(true);
                cc.set_lat_active(true);
                cc.set_long_active(true);
                let mut a = cc.init_actuators();
                a.set_torque(0.7);
                a.set_steering_angle_deg(99.);
                a.set_curvature(0.01);
                a.set_accel(0.5);
            }
            "modelV2" => {
                event
                    .init_model_v2()
                    .init_action()
                    .set_desired_curvature(0.01);
            }
            "liveParameters" => {
                let mut lp = event.init_live_parameters();
                lp.set_valid(true);
                lp.set_sensor_valid(true);
                lp.set_posenet_valid(true);
                lp.set_steer_ratio(15.);
                lp.set_stiffness_factor(1.);
            }
            "livePose" => {
                let mut p = event.init_live_pose();
                p.set_inputs_o_k(true);
                p.set_posenet_o_k(true);
                p.set_sensors_o_k(true);
            }
            "selfdriveState" => {
                event.init_selfdrive_state();
            }
            "onroadEvents" => {
                event.init_onroad_events(0);
            }
            _ => panic!("unsupported fixture service"),
        }
        capnp::serialize::write_message_to_words(&message)
    }
    #[test]
    fn final_boundary_sanitizes_only_lateral_until_its_independent_latch_then_never_rearms() {
        let mut state = State::new(
            &crate::core::SERVICES,
            Options {
                simulation: true,
                ..Options::default()
            },
        )
        .unwrap();
        state.update(0., &[packet("carControl")]).unwrap();
        let mut car = Message::new_default();
        let mut cs = car.init_root::<car_state::Builder>();
        cs.set_can_valid(true);
        cs.set_v_ego(10.);
        cs.set_steering_angle_deg(5.);
        let mut gate = Gate::default();
        let blocked = guard(
            &mut gate,
            &state,
            car.get_root_as_reader::<car_state::Reader>().unwrap(),
        )
        .unwrap()
        .unwrap();
        let cc = blocked.get_root_as_reader::<car_control::Reader>().unwrap();
        let a = cc.get_actuators().unwrap();
        assert!(!cc.get_lat_active());
        assert!(cc.get_enabled());
        assert!(cc.get_long_active());
        assert_eq!(
            (
                a.get_torque(),
                a.get_steering_angle_deg(),
                a.get_curvature(),
                a.get_accel()
            ),
            (0., 5., 0., 0.5)
        );
        let packets = [
            "modelV2",
            "liveParameters",
            "livePose",
            "selfdriveState",
            "onroadEvents",
        ]
        .map(packet);
        state.update(0.01, &packets).unwrap();
        assert!(guard(
            &mut gate,
            &state,
            car.get_root_as_reader::<car_state::Reader>().unwrap()
        )
        .unwrap()
        .is_none());
        assert!(gate.ready);
        state.update(100., &[]).unwrap();
        assert!(guard(
            &mut gate,
            &state,
            car.get_root_as_reader::<car_state::Reader>().unwrap()
        )
        .unwrap()
        .is_none());
    }
}
