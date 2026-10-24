use openpilot_dev_tools::argparse_scan::scan;

#[test]
fn short_help_clusters_keep_action_order_and_reject_explicit_values() {
    for argument in ["-h", "-hh", "-hhh", "-hfoo", "-hfoo=bar"] {
        assert!(scan(&[argument.into()], &[]).unwrap().help, "{argument}");
    }
    for argument in ["-h=foo", "-hh=foo", "-hh=", "-h-h", "--help=foo"] {
        assert!(scan(&[argument.into()], &[]).is_err(), "{argument}");
        assert!(
            scan(&["-h".into(), argument.into()], &[]).unwrap().help,
            "earlier help action: {argument}"
        );
    }
}

#[test]
fn deferred_ambiguity_preserves_help_order_without_changing_legacy_scans() {
    use openpilot_dev_tools::argparse_scan::{
        scan_with_positionals, scan_with_positionals_and_value_validation,
        scan_with_positionals_deferred_ambiguity, ArgumentError, OptionSpec,
    };
    let specs = [OptionSpec::value("--host", false)];
    let help_first = vec!["--help".into(), "--h".into()];
    assert!(scan_with_positionals(&help_first, &specs, 1).is_err());
    assert!(
        scan_with_positionals_deferred_ambiguity(&help_first, &specs, 1)
            .unwrap()
            .help
    );
    let ambiguous_first = vec!["--h".into(), "--help".into()];
    assert_eq!(
        scan_with_positionals_deferred_ambiguity(&ambiguous_first, &specs, 1)
            .err()
            .unwrap()
            .0,
        "ambiguous option: --h"
    );
    let missing_value = vec!["--host".into(), "--h".into(), "--help".into()];
    assert_eq!(
        scan_with_positionals_deferred_ambiguity(&missing_value, &specs, 1)
            .err()
            .unwrap()
            .0,
        "--host requires a value"
    );
    let invalid_value = vec!["--host=bad".into(), "--h".into(), "--help".into()];
    let conversion =
        scan_with_positionals_and_value_validation(&invalid_value, &specs, 1, |name, value| {
            assert_eq!((name, value), ("--host", "bad"));
            Err(ArgumentError("conversion failed first".into()))
        })
        .err()
        .unwrap();
    assert_eq!(conversion.0, "conversion failed first");
    assert!(
        scan_with_positionals_and_value_validation(&help_first, &specs, 1, |_, _| panic!(
            "help must not consume later values"
        ),)
        .unwrap()
        .help
    );
    let literal = vec!["--".into(), "--h".into()];
    assert_eq!(
        scan_with_positionals_deferred_ambiguity(&literal, &specs, 1)
            .unwrap()
            .positionals,
        ["--h"]
    );
}
