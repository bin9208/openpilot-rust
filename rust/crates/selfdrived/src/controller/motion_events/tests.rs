use super::*;
use crate::{
    alerts::Alert,
    callbacks::{self, AlertParams},
    car_specific::CarSpecificParams,
    controller::{
        config::{Config, Mode},
        effects::Personality,
    },
    state::EventType,
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_logging::Fields;
use openpilot_messaging::state::Options;
use openpilot_msgq::VisionStream;

#[derive(Default)]
struct Params {
    model: bool,
    reads: usize,
    fail: bool,
    fail_text: bool,
}
impl AlertParams for Params {
    fn text(&mut self, _: &str) -> Result<Option<String>, callbacks::Error> {
        if self.fail_text {
            return Err(callbacks::Error::MissingText("NNFFModelName"));
        }
        Ok(self.model.then(|| "test model".into()))
    }
    fn integer(&mut self, _: &str) -> Result<i32, callbacks::Error> {
        Ok(0)
    }
    fn boolean(&mut self, _: &str) -> Result<bool, callbacks::Error> {
        Ok(false)
    }
}
impl CarSpecificParams for Params {
    type Error = callbacks::Error;
    fn get_bool(&mut self, _: &str) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn put_bool(&mut self, _: &str, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
}
impl Effects for Params {
    fn presence(&mut self, key: &str) -> Result<bool, Error> {
        if key != "NNFFModelName" {
            return Ok(false);
        }
        self.reads += 1;
        if self.fail {
            return Err(Error::Contract("test read failure"));
        }
        Ok(self.model)
    }
    fn wide_camera(&mut self) -> Result<bool, Error> {
        Ok(false)
    }
    fn personality(&mut self) -> Result<Personality, Error> {
        Ok(Personality::standard())
    }
    fn remove(&mut self, _: &str) -> Result<(), Error> {
        Ok(())
    }
    fn offroad(&mut self, _: &str, _: Option<&str>) -> Result<(), Error> {
        Ok(())
    }
    fn event(&mut self, _: &str, _: Fields) -> Result<(), Error> {
        Ok(())
    }
    fn monotonic(&mut self) -> f64 {
        0.0
    }
    fn timestamp(&mut self) -> Result<u64, Error> {
        Ok(0)
    }
    fn available_streams(&mut self) -> Result<Vec<VisionStream>, Error> {
        Ok(Vec::new())
    }
}
fn fixture() -> (
    Controller,
    Params,
    State,
    capnp::message::Builder<capnp::message::HeapAllocator>,
) {
    let mut cp = capnp::message::Builder::new_default();
    cp.init_root::<car_params::Builder<'_>>();
    let config = Config::read(cp.get_root_as_reader().unwrap()).unwrap();
    let mode = Mode {
        replay: false,
        simulation: false,
        testing_closet: false,
        device_type: "tici".into(),
        nvme_present: false,
        branch: "dev-md".into(),
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../openpilot/selfdrive/ui/translations");
    let language = openpilot_ui_framework::multilang::Multilang::new(&root, Some("en")).unwrap();
    let mut params = Params {
        model: true,
        ..Params::default()
    };
    let mut controller = Controller::new(config, (mode, language), &mut params).unwrap();
    controller.initialized = true;
    controller.events.clear();
    let names = vec!["deviceState".to_owned()];
    let options = Options {
        ignore_alive: names.clone(),
        ignore_frequency: names.clone(),
        ignore_valid: names,
        ..Options::default()
    };
    let mut state = State::new(&["deviceState"], options).unwrap();
    let mut message = capnp::message::Builder::new_default();
    message
        .init_root::<log_capnp::event::Builder<'_>>()
        .init_device_state()
        .set_started(true);
    state
        .update(0.0, &[capnp::serialize::write_message_to_words(&message)])
        .unwrap();
    let mut car = capnp::message::Builder::new_default();
    car.init_root::<car_state::Builder<'_>>()
        .set_can_valid(true);
    (controller, params, state, car)
}
fn advance(state: &mut State, frame: i64) {
    while state.frame() < frame {
        state.update(state.frame() as f64 * 0.01, &[]).unwrap();
    }
}
fn poll(
    controller: &mut Controller,
    params: &mut Params,
    state: &mut State,
    car: &capnp::message::Builder<capnp::message::HeapAllocator>,
    frame: i64,
) -> Result<(), Error> {
    advance(state, frame);
    controller.update_nnff_alert(car.get_root_as_reader().unwrap(), state, params)
}
fn noticed(controller: &Controller) -> bool {
    controller.events.names().contains(&E::TorqueNNLoad)
}

#[test]
fn nnff_retries_after_late_readiness_and_delivers_selected_alert_once() {
    let (mut c, mut p, mut s, car) = fixture();
    c.initialized = false;
    poll(&mut c, &mut p, &mut s, &car, 550).unwrap();
    assert!(!noticed(&c));
    assert_eq!(p.reads, 0);
    c.initialized = true;
    poll(&mut c, &mut p, &mut s, &car, 600).unwrap();
    assert!(noticed(&c));
    assert_eq!(c.alerts.current().alert_type, "torqueNNLoad/permanent");
    c.events.clear();
    c.alerts.process_alerts(10000, &[]);
    poll(&mut c, &mut p, &mut s, &car, 650).unwrap();
    assert!(!noticed(&c));
    assert_eq!(p.reads, 1);
}
#[test]
fn nnff_absent_model_and_read_failure_remain_retryable_at_two_hz() {
    let (mut c, mut p, mut s, car) = fixture();
    p.model = false;
    for frame in [549, 550, 551, 599] {
        poll(&mut c, &mut p, &mut s, &car, frame).unwrap();
    }
    assert_eq!(p.reads, 1);
    assert!(!noticed(&c));
    p.model = true;
    p.fail = true;
    assert!(poll(&mut c, &mut p, &mut s, &car, 600).is_err());
    p.fail = false;
    poll(&mut c, &mut p, &mut s, &car, 650).unwrap();
    assert!(noticed(&c));
}
#[test]
fn nnff_preserves_existing_alert_and_no_entry_until_clear() {
    let (mut c, mut p, mut s, car) = fixture();
    c.alerts.add_many(
        550,
        [Alert {
            alert_type: "warning/warning".into(),
            duration: 10,
            ..Alert::default()
        }],
    );
    c.alerts.process_alerts(550, &[]);
    poll(&mut c, &mut p, &mut s, &car, 550).unwrap();
    assert!(!noticed(&c));
    c.alerts.process_alerts(600, &[]);
    c.events.add(E::DoorOpen, false);
    assert!(c.events.contains(EventType::NoEntry));
    poll(&mut c, &mut p, &mut s, &car, 600).unwrap();
    assert!(!noticed(&c));
    c.events.clear();
    poll(&mut c, &mut p, &mut s, &car, 650).unwrap();
    assert!(noticed(&c));
}
#[test]
fn nnff_rejects_passive_invalid_can_timeout_and_failed_health() {
    for gate in 0..4 {
        let (mut c, mut p, mut s, mut car) = fixture();
        match gate {
            0 => c.config.passive = true,
            1 => car
                .get_root::<car_state::Builder<'_>>()
                .unwrap()
                .set_can_valid(false),
            2 => car
                .get_root::<car_state::Builder<'_>>()
                .unwrap()
                .set_can_timeout(true),
            _ => {
                s = State::new(&["deviceState"], Options::default()).unwrap();
                let mut message = capnp::message::Builder::new_default();
                message
                    .init_root::<log_capnp::event::Builder<'_>>()
                    .init_device_state()
                    .set_started(true);
                s.update(0.0, &[capnp::serialize::write_message_to_words(&message)])
                    .unwrap();
            }
        }
        poll(&mut c, &mut p, &mut s, &car, 550).unwrap();
        assert!(!noticed(&c), "gate {gate}");
        assert_eq!(p.reads, 0);
    }
}

#[test]
fn nnff_alert_selection_failure_does_not_consume_one_shot() {
    let (mut c, mut p, mut s, car) = fixture();
    p.fail_text = true;
    assert!(poll(&mut c, &mut p, &mut s, &car, 550).is_err());
    c.events.clear();
    p.fail_text = false;
    poll(&mut c, &mut p, &mut s, &car, 600).unwrap();
    assert!(noticed(&c));
    assert_eq!(c.alerts.current().alert_type, "torqueNNLoad/permanent");
}

#[test]
fn nnff_offroad_does_not_poll_or_consume_notice() {
    let (mut c, mut p, mut s, car) = fixture();
    let mut message = capnp::message::Builder::new_default();
    message
        .init_root::<log_capnp::event::Builder<'_>>()
        .init_device_state()
        .set_started(false);
    s.update(0.01, &[capnp::serialize::write_message_to_words(&message)])
        .unwrap();
    poll(&mut c, &mut p, &mut s, &car, 550).unwrap();
    assert!(!noticed(&c));
    assert_eq!(p.reads, 0);
}
