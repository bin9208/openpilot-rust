use openpilot_cereal::{
    car_capnp::{car_control, car_params, car_state},
    log_capnp::onroad_event::EventName,
};
use openpilot_selfdrived::{
    car_specific::{
        CarControl, CarInputs, CarParams, CarSpecificEvents, CarSpecificParams, CarState,
    },
    events::Catalog,
};
struct Params;
impl CarSpecificParams for Params {
    type Error = ();
    fn get_bool(&mut self, _: &str) -> Result<bool, ()> {
        Ok(false)
    }
    fn put_bool(&mut self, _: &str, _: bool) -> Result<(), ()> {
        Ok(())
    }
}
#[test]
fn tesla_stop_inhibit_debounces_but_moving_or_permanent_faults_do_not() {
    let mut cp = capnp::message::Builder::new_default();
    cp.init_root::<car_params::Builder>().set_brand("tesla");
    let cp = CarParams::read(cp.get_root_as_reader().unwrap()).unwrap();
    let mut cs = capnp::message::Builder::new_default();
    cs.init_root::<car_state::Builder>();
    let mut current = CarState::read(cs.get_root_as_reader().unwrap()).unwrap();
    current.steer_fault_temporary = true;
    current.v_ego = 0.4;
    let mut previous = current.clone();
    previous.steer_fault_temporary = false;
    let mut cc = capnp::message::Builder::new_default();
    cc.init_root::<car_control::Builder>();
    let control = CarControl::read(cc.get_root_as_reader().unwrap()).unwrap();
    let catalog = Catalog::load(false).unwrap();
    let mut policy = CarSpecificEvents::new(cp);
    let mut params = Params;
    for i in 1..220 {
        let input = CarInputs {
            current: current.clone(),
            previous: previous.clone(),
            control: control.clone(),
        };
        let events = policy.update(&input, &mut params, &catalog).unwrap();
        if i < 219 {
            assert!(
                !events.contains(&EventName::SteerTempUnavailableSilent),
                "debounce frame {i}"
            );
        } else {
            assert!(events.contains(&EventName::SteerTempUnavailableSilent));
        }
        previous = current.clone();
    }
    current.steer_fault_temporary = false;
    policy
        .update(
            &CarInputs {
                current: current.clone(),
                previous: previous.clone(),
                control: control.clone(),
            },
            &mut params,
            &catalog,
        )
        .unwrap();
    current.steer_fault_temporary = true;
    current.steer_fault_permanent = true;
    let events = policy
        .update(
            &CarInputs {
                current: current.clone(),
                previous: previous.clone(),
                control: control.clone(),
            },
            &mut params,
            &catalog,
        )
        .unwrap();
    assert!(events.contains(&EventName::SteerUnavailable));
    assert!(!events.contains(&EventName::SteerTempUnavailableSilent));
    current.v_ego = 2.;
    current.steer_fault_permanent = false;
    for _ in 0..25 {
        previous = current.clone();
        let _ = policy
            .update(
                &CarInputs {
                    current: current.clone(),
                    previous: previous.clone(),
                    control: control.clone(),
                },
                &mut params,
                &catalog,
            )
            .unwrap();
    }
    let events = policy
        .update(
            &CarInputs {
                current,
                previous,
                control,
            },
            &mut params,
            &catalog,
        )
        .unwrap();
    assert!(events.contains(&EventName::SteerTempUnavailable));
}
