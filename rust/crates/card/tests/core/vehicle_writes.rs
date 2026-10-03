use super::{car_params, event, Builder, Card, Driver, Error, Io, Options, Params, State, Tail};

struct WriteRun {
    result: Result<(), Error>,
    operations: Vec<String>,
    persisted: Option<Vec<u8>>,
}

fn run_vehicle_write(queue: bool, fail_apply: bool, fail_write: bool) -> WriteRun {
    let root = tempfile::tempdir().unwrap();
    let settings = std::sync::Arc::new(Params::open(root.path(), "d").unwrap());
    let mut params = Builder::new_default();
    params.init_root::<car_params::Builder>().set_passive(false);
    let mut state = State::new(&openpilot_card::core::SERVICES, Options::default()).unwrap();
    let mut control = Builder::new_default();
    control
        .init_root::<event::Builder>()
        .init_car_control()
        .set_enabled(true);
    let mut ready = Builder::new_default();
    ready.init_root::<event::Builder>().init_onroad_events(0);
    state
        .update(
            0.,
            &[
                capnp::serialize::write_message_to_words(&control),
                capnp::serialize::write_message_to_words(&ready),
            ],
        )
        .unwrap();
    let mut io = Io {
        state,
        sent: vec![],
        now: 1,
        writes: vec![],
        logs: vec![],
        operations: vec![],
        fail_vehicle_write: fail_write,
        writer: Some(openpilot_card::async_params::AsyncParams::new(
            std::sync::Arc::clone(&settings),
        )),
    };
    let mut driver = Driver {
        calls: vec![],
        logs: vec![],
        writes: vec![],
        queue_write: queue,
        fail_apply,
    };
    let mut card = Card::new(params, Params::open(root.path(), "d").unwrap(), false, true).unwrap();
    let result = card.step(&mut driver, &mut Tail { value: 0. }, &mut io);
    let operations = std::mem::take(&mut io.operations);
    drop(io);
    WriteRun {
        result,
        operations,
        persisted: settings.get("ActivateCruiseAfterBrake").unwrap(),
    }
}

#[test]
fn vehicle_write_is_enqueued_before_sendcan_and_drained_on_shutdown() {
    let run = run_vehicle_write(true, false, false);
    run.result.unwrap();
    assert_eq!(
        run.operations,
        [
            "publish:carParams",
            "publish:carOutput",
            "publish:carState",
            "put:ControlsReady",
            "put:ActivateCruiseAfterBrake",
            "publish:sendcan"
        ]
    );
    assert_eq!(run.persisted.as_deref(), Some(b"1".as_slice()));
}

#[test]
fn vehicle_write_survives_apply_failure_and_is_drained_on_shutdown() {
    let run = run_vehicle_write(true, true, false);
    assert!(matches!(run.result, Err(Error::Event("apply failure"))));
    assert!(!run.operations.iter().any(|op| op == "publish:sendcan"));
    assert_eq!(run.persisted.as_deref(), Some(b"1".as_slice()));
}

#[test]
fn vehicle_write_enqueue_failure_prevents_sendcan_and_propagates() {
    let run = run_vehicle_write(true, false, true);
    assert!(matches!(run.result, Err(Error::Event("write failure"))));
    assert!(!run.operations.iter().any(|op| op == "publish:sendcan"));
    assert!(run.persisted.is_none());
}

#[test]
fn vehicle_without_writes_keeps_existing_publication_order() {
    let run = run_vehicle_write(false, false, false);
    run.result.unwrap();
    assert_eq!(
        run.operations,
        [
            "publish:carParams",
            "publish:carOutput",
            "publish:carState",
            "put:ControlsReady",
            "publish:sendcan"
        ]
    );
    assert!(run.persisted.is_none());
}
