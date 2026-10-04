use openpilot_radarcan::native::parse;

#[test]
fn constructor_barrier_rejects_unbounded_or_incomplete_fixture() {
    for arguments in [
        vec!["--fixture-constructor-ready", "ready"],
        vec![
            "--fixture-constructor-ready",
            "ready",
            "--fixture-constructor-start",
            "start",
        ],
        vec!["--steps", "3", "--fixture-constructor-start", "start"],
        vec![
            "--steps",
            "3",
            "--fixture-constructor-ready",
            "same",
            "--fixture-constructor-start",
            "same",
        ],
    ] {
        assert!(parse(arguments.into_iter().map(str::to_owned)).is_err());
    }
}

#[test]
fn constructor_barrier_preserves_distinct_paths_and_positive_bound() {
    let arguments = [
        "--steps",
        "7",
        "--fixture-constructor-ready",
        "ready",
        "--fixture-constructor-start",
        "start",
    ];
    let options = parse(arguments.into_iter().map(str::to_owned))
        .unwrap()
        .unwrap();
    assert_eq!(options.steps.unwrap().get(), 7);
    let fixture = options.fixture.unwrap();
    assert_eq!(fixture.ready, std::path::Path::new("ready"));
    assert_eq!(fixture.start, std::path::Path::new("start"));
}
