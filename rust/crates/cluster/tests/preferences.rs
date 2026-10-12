use openpilot_cluster::{
    runtime_args::{Codec, H264Backend, RuntimeArgs},
    runtime_preferences::Reader,
};

#[test]
fn unavailable_owner_and_missing_values_preserve_distinct_source_defaults() {
    let unavailable = Reader::new(None).snapshot();
    assert_eq!(unavailable.display.language, "ko");
    assert!(unavailable.display.is_metric);
    assert!(unavailable.clock_visible);
    assert!(unavailable.gate.allowed);
    assert!(unavailable.hud_mode.is_none());
    assert!(unavailable.encoder.is_none());
    let directory = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(directory.path(), "cluster").unwrap();
    let empty = Reader::new(Some(params)).snapshot();
    assert_eq!(empty.display.language, "en");
    assert!(empty.display.is_metric);
    assert!(!empty.clock_visible);
    assert!(!empty.gate.allowed);
    assert_eq!(empty.hud_mode, Some(0));
    assert_eq!(empty.encoder, Some(0));
    assert_eq!(empty.radar.info, 4);
    println!(
        "unavailable {}; missing {}",
        serde_json::to_string(&unavailable).unwrap(),
        serde_json::to_string(&empty).unwrap()
    );
}

#[test]
fn current_values_and_cli_presence_control_encoder_selection() {
    let directory = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(directory.path(), "cluster").unwrap();
    params.put("ClusterHudEncoder", b"3").unwrap();
    params.put("ClusterHudBrightness", b" +101tail").unwrap();
    params.put("ClusterHudDebug", b"2").unwrap();
    params.put_bool("UsbGpuActive", true).unwrap();
    let reader = Reader::new(Some(params.clone()));
    let mut args = RuntimeArgs::default();
    assert_eq!(reader.apply_encoder(&mut args), "ClusterHudEncoder");
    assert_eq!(args.usb_codec, Codec::H264);
    assert_eq!(args.usb_h264_backend, H264Backend::Ffmpeg);
    let value = reader.snapshot();
    assert_eq!(value.brightness, 100);
    assert_eq!(value.usb_fps.to_bits(), 5.0_f64.to_bits());
    assert!(value.gate.allowed);
    assert_eq!(value.gate.mode, 2);
    args.encoder_presence.usb_codec_from_cli = true;
    args.usb_codec = Codec::Png;
    assert_eq!(reader.apply_encoder(&mut args), "--usb-codec");
    assert_eq!(args.usb_codec, Codec::Png);
    params.put_bool("UsbGpuActive", false).unwrap();
    assert_eq!(reader.snapshot().usb_fps.to_bits(), 10.0_f64.to_bits());
    println!("current {}", serde_json::to_string(&value).unwrap());
}
