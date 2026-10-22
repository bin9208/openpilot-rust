use super::{EventName, State};

fn warnings(state: &mut State, names: &[EventName]) {
    let mut message = capnp::message::Builder::new_default();
    let root = message.init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>();
    let mut events = root.init_onroad_events(u32::try_from(names.len()).unwrap());
    for (index, name) in names.iter().enumerate() {
        events
            .reborrow()
            .get(u32::try_from(index).unwrap())
            .set_name(*name);
    }
    // Exercise the real serialized full-cereal segment reader, including state carry.
    state
        .segment(&capnp::serialize::write_message_to_words(&message))
        .unwrap();
}

#[test]
fn numbered_driver_warnings_each_start_a_report_episode() {
    let mut state = State::new();
    for (index, name) in [
        EventName::DriverDistracted1,
        EventName::DriverDistracted2,
        EventName::DriverDistracted3,
        EventName::DriverUnresponsive1,
        EventName::DriverUnresponsive2,
        EventName::DriverUnresponsive3,
    ]
    .into_iter()
    .enumerate()
    {
        warnings(&mut state, &[name, name]);
        warnings(&mut state, &[name]);
        assert_eq!(state.warn_counts, [0, 0, index + 1], "{name:?}");
        warnings(&mut state, &[]);
    }
    assert_eq!(
        state.warnings().get("driverDistracted"),
        &crate::Value::integer(6)
    );
}

#[test]
fn escalation_and_segment_boundaries_do_not_duplicate_warning_episodes() {
    let mut state = State::new();
    warnings(&mut state, &[EventName::DriverDistracted1]);
    warnings(
        &mut state,
        &[EventName::DriverDistracted2, EventName::DriverDistracted3],
    );
    warnings(
        &mut state,
        &[
            EventName::DriverUnresponsive1,
            EventName::DriverUnresponsive2,
        ],
    );
    warnings(
        &mut state,
        &[EventName::DriverUnresponsive3, EventName::TooDistracted],
    );
    warnings(&mut state, &[EventName::TooDistracted]);
    assert_eq!(state.warn_counts, [0, 0, 1]);
    warnings(&mut state, &[]);
    warnings(&mut state, &[EventName::DriverDistracted1]);
    assert_eq!(state.warn_counts, [0, 0, 2]);
}

#[test]
fn existing_fcw_ldw_and_too_distracted_categories_keep_independent_edges() {
    let mut state = State::new();
    warnings(
        &mut state,
        &[
            EventName::Fcw,
            EventName::StockFcw,
            EventName::Ldw,
            EventName::TooDistracted,
        ],
    );
    warnings(
        &mut state,
        &[
            EventName::StockFcw,
            EventName::Ldw,
            EventName::TooDistracted,
        ],
    );
    assert_eq!(state.warn_counts, [1, 1, 1]);
    warnings(&mut state, &[EventName::Ldw]);
    warnings(&mut state, &[EventName::StockFcw, EventName::TooDistracted]);
    warnings(&mut state, &[EventName::Ldw, EventName::TooDistracted]);
    assert_eq!(state.warn_counts, [2, 2, 2]);
}
