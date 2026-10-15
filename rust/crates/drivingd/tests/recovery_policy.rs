use openpilot_driving_modeld::recovery_policy::{Budget, Policy, State};

#[test]
fn retries_require_continuous_native_health_and_persisted_bounded_budget() {
    let mut policy = Policy::new(Budget::default());
    policy.failed(0., true, false, true);
    assert_eq!(policy.state, State::Waiting);
    for i in 0..=220 {
        policy.observe(i as f64 * 0.05, true, 0.03, false, false);
    }
    assert!(!policy.begin(11., false, false));
    assert!(policy.begin(11., true, false));
    assert_eq!(policy.budget.attempts, 1);
    assert!(!policy.begin(11., true, true));
    policy.completed(12., true);
    assert_eq!(policy.next_attempt, 42.);
    assert!(!policy.observe(12.1, false, 0.01, false, false));
    for i in 0..=610 {
        policy.observe(12.2 + i as f64 * 0.05, true, 0.03, false, false);
    }
    assert!(policy.begin(12.2 + 610. * 0.05, true, false));
    assert!(policy.observe(42.75, true, 0.051, false, true));
    assert_eq!(policy.state, State::Stopped);
    assert_eq!(policy.reason, "internal_delayed");
}

#[test]
fn malformed_budget_and_unknown_or_rejected_failures_never_retry() {
    for bytes in [
        b"null".as_slice(),
        b"{}",
        b"{\"attempts\":4,\"stopped\":false}",
        b"{\"attempts\":0,\"stopped\":0}",
    ] {
        assert!(Budget::parse(bytes).stopped);
    }
    for (retry, rejected, generic) in [
        (false, false, true),
        (true, true, true),
        (true, false, false),
    ] {
        let mut policy = Policy::new(Budget::default());
        policy.failed(0., retry, rejected, generic);
        assert_eq!(policy.state, State::Stopped);
    }
}

#[test]
fn lifecycle_decisions_match_pinned_original_class_4290_source_steps() {
    use openpilot_driving_modeld::recovery_policy::{Budget, Policy};
    let fixture: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/recovery_policy_source.json")).unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "f816013406886e4e7aaaf557864a39a41d5da47fdbccd8974f7a149cf9ac54a4"
    );
    let mut count = 0;
    for (trial_index, trial) in fixture["trials"].as_array().unwrap().iter().enumerate() {
        let mut policy =
            Policy::new(serde_json::from_value::<Budget>(trial["budget"].clone()).unwrap());
        let mut task = false;
        for (step_index, step) in trial["steps"].as_array().unwrap().iter().enumerate() {
            let now = step["now"].as_f64().unwrap();
            match step["action"].as_str().unwrap() {
                "failed" => policy.failed(
                    now,
                    step["retryable"].as_bool().unwrap(),
                    step["rejected"].as_bool().unwrap(),
                    step["generic"].as_bool().unwrap(),
                ),
                "observe" => {
                    policy.observe(
                        now,
                        step["published"].as_bool().unwrap(),
                        step["work"].as_f64().unwrap(),
                        step["dropped"].as_bool().unwrap(),
                        task,
                    );
                }
                "begin" => {
                    if policy.begin(now, step["present"].as_bool().unwrap(), task) {
                        task = true;
                    }
                }
                "complete" => {
                    assert!(task);
                    task = false;
                    policy.completed(now, step["retryable"].as_bool().unwrap());
                }
                _ => panic!("unknown source action"),
            }
            let expected = &step["expected"];
            assert_eq!(
                serde_json::to_value(policy.state).unwrap(),
                expected["state"],
                "{trial_index}:{step_index} {step}"
            );
            assert_eq!(
                policy.reason,
                expected["reason"].as_str().unwrap(),
                "{trial_index}:{step_index}"
            );
            assert_eq!(
                policy.budget.attempts,
                expected["attempts"].as_u64().unwrap() as usize,
                "{trial_index}:{step_index}"
            );
            assert_eq!(
                policy.budget.stopped,
                expected["stopped"].as_bool().unwrap(),
                "{trial_index}:{step_index}"
            );
            assert!(
                (policy.next_attempt - expected["next_attempt"].as_f64().unwrap()).abs() < 1e-9,
                "{trial_index}:{step_index}"
            );
            assert_eq!(
                task,
                expected["task"].as_bool().unwrap(),
                "{trial_index}:{step_index}"
            );
            count += 1;
        }
    }
    assert_eq!(count, 4290);
}
