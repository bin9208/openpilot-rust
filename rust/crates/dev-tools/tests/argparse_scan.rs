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
