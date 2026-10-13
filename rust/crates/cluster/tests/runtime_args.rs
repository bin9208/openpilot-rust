use openpilot_cluster::runtime_args::{Codec, InputSource, Output, RuntimeArgs};
use std::path::Path;

#[test]
fn parser_keeps_demo_default_and_exact_option_presence_with_abbreviations() {
    let root = Path::new("/owned/openpilot");
    let defaults = RuntimeArgs::parse_from(root, &[] as &[&str]).unwrap();
    assert_eq!(defaults.input, InputSource::Random);
    assert_eq!(defaults.output, Output::Usb);
    assert_eq!(defaults.usb_codec, Codec::Jpeg);
    assert!(defaults.is_metric.is_none());
    let parsed = RuntimeArgs::parse_from(
        root,
        &[
            "--input=live",
            "--usb-code=h264",
            "--imperial",
            "--fps",
            "1_0",
        ],
    )
    .unwrap();
    assert_eq!(parsed.input, InputSource::Live);
    assert_eq!(parsed.usb_codec, Codec::H264);
    assert!(!parsed.codec_from_cli());
    assert_eq!(parsed.is_metric, Some(false));
    assert_eq!(parsed.fps.to_bits(), 10.0_f64.to_bits());
    assert!(parsed.fps_from_cli());
}

#[test]
fn parser_rejects_conflicting_ack_and_codec_policies() {
    let root = Path::new("/owned/openpilot");
    assert!(RuntimeArgs::parse_from(root, &["--metric", "--imperial"]).is_err());
    for args in [
        vec!["--usb-h264-no-ack", "--usb-h264-wait-ack"],
        vec!["--usb-async", "--usb-codec", "h264"],
        vec!["--navi-port", "65536"],
        vec!["--width", "0"],
        vec!["--input", "route", "--route-replay-speed", "0"],
    ] {
        assert!(RuntimeArgs::parse_from(root, &args).is_err(), "{args:?}");
    }
}

#[test]
fn live_usb_policy_overrides_requested_rates_but_window_replay_keeps_them() {
    let root = Path::new("/owned/openpilot");
    let args = RuntimeArgs::parse_from(root, &["--input=live", "--fps=60", "--usb-display-fps=90"])
        .unwrap();
    let normal = args.output_rate(true, false).unwrap();
    let egpu = args.output_rate(true, true).unwrap();
    assert_eq!(normal.target_fps.to_bits(), 10.0_f64.to_bits());
    assert_eq!(normal.usb_display_fps, 10);
    assert_eq!(egpu.target_fps.to_bits(), 5.0_f64.to_bits());
    assert_eq!(egpu.usb_display_fps, 5);
    assert!(normal.fixed_usb_policy && normal.usb_display_fps_auto);
    let window = RuntimeArgs::parse_from(root, &["--input=live", "--output=window", "--fps=60"])
        .unwrap()
        .output_rate(true, true)
        .unwrap();
    assert_eq!(window.target_fps.to_bits(), 60.0_f64.to_bits());
    assert!(!window.fixed_usb_policy);
    assert_eq!(window.usb_display_fps, 0);
}

#[test]
fn parser_preserves_python_paths_and_unicode_negative_numbers() {
    let root = Path::new("/owned/openpilot");
    assert_eq!(
        RuntimeArgs::parse_from(root, &["--route="]).unwrap().route,
        Path::new(".")
    );
    assert_eq!(
        RuntimeArgs::parse_from(root, &["--route=./a//b/."])
            .unwrap()
            .route,
        Path::new("a/b")
    );
    assert_eq!(
        RuntimeArgs::parse_from(root, &["--duration", "-１２.５"])
            .unwrap()
            .duration
            .unwrap()
            .to_bits(),
        (-12.5_f64).to_bits()
    );
}
