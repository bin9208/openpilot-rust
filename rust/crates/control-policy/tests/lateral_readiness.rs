use openpilot_control_policy::lateral_readiness::{
    Car, Gate, Inputs, Lane, LiveParameters, Service, Services, State,
};

fn ready_service() -> Service {
    Service {
        seen: true,
        valid: true,
        alive: true,
        checks: true,
    }
}
fn services() -> Services {
    Services {
        boundary: ready_service(),
        model: ready_service(),
        live: ready_service(),
        pose: ready_service(),
        selfdrive: ready_service(),
        events: ready_service(),
        lane: ready_service(),
    }
}
fn live() -> LiveParameters {
    LiveParameters {
        ratio: 15.,
        stiffness: 1.,
        offset: 0.,
        roll: 0.,
        valid: true,
        sensor_valid: true,
        posenet_valid: true,
    }
}
fn car() -> Car {
    Car {
        can_valid: true,
        fault_temporary: false,
        fault_permanent: false,
        values: [15., 1., 0., 0.],
    }
}
fn state() -> State {
    State {
        services: services(),
        pose: [true; 3],
        initializing: false,
    }
}
fn input<'a>(
    state: &'a State,
    live: &'a LiveParameters,
    car: &'a Car,
    values: &'a [f64],
) -> Inputs<'a> {
    Inputs {
        state,
        live,
        car: *car,
        desired_curvature: 0.01,
        lane: Lane {
            use_lane_lines: true,
            solution_valid: true,
            psis: values,
            curvatures: values,
            distances: values,
        },
    }
}
#[test]
fn readiness_is_a_process_lifetime_latch_including_boundary_receipt() {
    let mut gate = Gate::default();
    let mut state = state();
    let live = live();
    let car = car();
    let values = [0.; 17];
    state.services.boundary.seen = false;
    assert!(!gate.update(input(&state, &live, &car, &values)));
    state.services.boundary.seen = true;
    assert!(gate.update(input(&state, &live, &car, &values)));
    state.services.model.checks = false;
    state.initializing = true;
    assert!(gate.update(input(&state, &live, &car, &values)));
    assert!(!Gate::default().update(input(&state, &live, &car, &values)));
}
#[test]
fn event_bursts_ignore_rate_only_and_lane_plans_require_exact_dimensions() {
    let mut state = state();
    let live = live();
    let car = car();
    let values = [0.; 17];
    state.services.events.checks = false;
    assert!(Gate::default().update(input(&state, &live, &car, &values)));
    state.services.events.alive = false;
    assert!(!Gate::default().update(input(&state, &live, &car, &values)));
    state.services.events.alive = true;
    for length in [0, 16, 18] {
        assert!(!Gate::default().update(input(&state, &live, &car, &vec![0.; length])));
    }
    let mut request = input(&state, &live, &car, &[]);
    request.lane.use_lane_lines = false;
    assert!(Gate::default().update(request));
}
#[test]
fn invalid_live_values_use_nominal_inactive_geometry_and_do_not_grant_readiness() {
    let state = state();
    let mut live = live();
    let car = car();
    let values = [0.; 17];
    for invalid in [f64::NAN, f64::INFINITY, 0., 1.] {
        live.ratio = invalid;
        assert!(!Gate::default().update(input(&state, &live, &car, &values)));
        let fallback = live.for_inactive(state.services.live, 17.);
        assert_eq!(
            (
                fallback.ratio,
                fallback.stiffness,
                fallback.offset,
                fallback.roll
            ),
            (17., 1., 0., 0.)
        );
    }
}
#[test]
fn every_car_model_and_lane_value_must_be_finite_before_startup() {
    let state = state();
    let live = live();
    let mut car = car();
    let values = [0.; 17];
    for index in 0..4 {
        car.values[index] = f64::NAN;
        assert!(!Gate::default().update(input(&state, &live, &car, &values)));
        car.values[index] = 0.;
    }
    let mut request = input(&state, &live, &car, &values);
    request.desired_curvature = f64::INFINITY;
    assert!(!Gate::default().update(request));
    for index in 0..17 {
        let mut bad = values;
        bad[index] = f64::NAN;
        assert!(!Gate::default().update(input(&state, &live, &car, &bad)));
    }
}
