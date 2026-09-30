use openpilot_logging::{
    communication::snapshot_at, runtime::RuntimeDiagnostics, Fields, Number, Value,
};
use openpilot_messaging::state::{Options, State};

#[test]
fn unseen_snapshot_preserves_filters_and_ignore_flags() {
    let state = State::new(
        &["modelV2", "carState"],
        Options {
            ignore_alive: vec!["modelV2".into()],
            ignore_valid: vec!["carState".into()],
            ..Options::default()
        },
    )
    .unwrap();
    let result = snapshot_at(&state, &["missing", "modelV2", "carState"], 17.0).unwrap();
    assert_eq!(result.len(), 2);
    let Value::Object(model) = &result["modelV2"] else {
        panic!("object expected")
    };
    assert_eq!(model["avg_hz"], Value::Null);
    assert_eq!(model["recv_age_ms"], Value::Null);
    assert_eq!(model["ignore_alive"], Value::Bool(true));
    assert_eq!(model["ignore_freq"], Value::Bool(true));
    let Value::Object(car) = &result["carState"] else {
        panic!("object expected")
    };
    assert_eq!(car["ignore_valid"], Value::Bool(true));
    assert_eq!(car["ignore_freq"], Value::Bool(false));
}

#[test]
fn live_scheduler_and_failed_emit_do_not_poison_next_interval() {
    let mut diagnostics = RuntimeDiagnostics::new("test", 0.0);
    let mut calls = 0;
    diagnostics
        .record_with(
            [("work".into(), Number::Integer(1))],
            Fields::new(),
            |fields| {
                calls += 1;
                assert_eq!(
                    fields["pid"],
                    Value::Integer(i128::from(std::process::id()))
                );
                let Value::Object(scheduler) = &fields["scheduler"] else {
                    panic!("object expected")
                };
                assert_eq!(scheduler.len(), 3);
                Err::<(), _>(std::io::Error::other("sink unavailable"))
            },
        )
        .unwrap();
    diagnostics
        .record_with([], Fields::new(), |fields| {
            calls += 1;
            assert_eq!(fields["frames"], Value::Integer(1));
            assert_eq!(fields["metrics"], Value::Object(Fields::new()));
            Ok::<(), std::io::Error>(())
        })
        .unwrap();
    assert_eq!(calls, 2);
}
