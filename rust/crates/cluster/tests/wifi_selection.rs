use openpilot_cluster::runtime_args::{Codec, InputSource, RuntimeArgs};
use openpilot_cluster_wifi::Registration;
use serde_json::{json, Value};
#[test]
fn actual_source_s3_selection_overrides_codec_quality_async_and_live_egpu_cap() {
    let fixture: Value = serde_json::from_str(include_str!("data/wifi_selection.json")).unwrap();
    let cases = fixture["input"].as_array().unwrap();
    assert_eq!(cases.len(), 120);
    for (index, case) in cases.iter().enumerate() {
        let registration = Registration::from_value(json!({"version":1,"device":"001122334455","key":"ab".repeat(32),"fps":case["fps"],"quality":case["quality"]})).unwrap();
        let mut args = RuntimeArgs {
            input: if case["live"] == true {
                InputSource::Live
            } else {
                InputSource::Random
            },
            usb_codec: Codec::H264,
            ..RuntimeArgs::default()
        };
        let encoder_source = args.apply_wifi(&registration);
        let rate = args
            .output_rate(
                case["tici"].as_bool().unwrap(),
                case["egpu"].as_bool().unwrap(),
            )
            .unwrap();
        let result = json!({"codec":if args.usb_codec == Codec::Jpeg {"jpeg"} else {"wrong"},"quality":args.usb_jpeg_quality,"asynchronous":args.transport.usb_async,"encoder_source":encoder_source,"fps":rate.target_fps,"source":rate.fps_source,"display_fps":rate.usb_display_fps,"automatic":rate.usb_display_fps_auto});
        assert_eq!(
            result, fixture["expected"][index],
            "source selection {index}"
        );
        // Pairing secrets are not included in diagnostic RuntimeArgs JSON.
        assert!(!serde_json::to_string(&args)
            .unwrap()
            .contains(&"ab".repeat(32)));
    }
}
