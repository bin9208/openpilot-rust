use openpilot_cluster_wifi::{self as wifi, Registration};
use serde_json::json;
fn config(codec: &str) -> Registration {
    Registration::from_value(
        json!({"version":1,"device":"001122334455","key":"ab".repeat(32),"codec":codec}),
    )
    .unwrap()
}
#[test]
fn registration_rejects_unknown_codec_and_forced_h264_requires_authenticated_support() {
    assert!(Registration::from_value(
        json!({"version":1,"device":"001122334455","key":"ab".repeat(32),"codec":"raw"})
    )
    .is_err());
    assert_eq!(
        wifi::select_codec_with(&config("auto"), false, || Ok(0)).unwrap(),
        wifi::Codec::Jpeg
    );
    assert_eq!(
        wifi::select_codec_with(&config("auto"), false, || Ok(1)).unwrap(),
        wifi::Codec::H264
    );
    assert!(wifi::select_codec_with(&config("h264"), false, || Ok(0)).is_err());
    for codec in ["jpeg", "auto"] {
        assert_eq!(
            wifi::select_codec_with(&config(codec), true, || panic!(
                "JPEG fallback must not query"
            ))
            .unwrap(),
            wifi::Codec::Jpeg
        );
    }
}
#[test]
fn capability_reply_binds_hmac_device_nonce_and_feature_word() {
    let config = config("auto");
    let nonce = [4; 16];
    let request = wifi::capability_request(&config, nonce);
    assert_eq!(request.len(), 58);
    assert_eq!(&request[..4], b"S3C1");
    let mut reply = b"S3C2".to_vec();
    reply.extend(config.device);
    reply.extend(nonce);
    reply.extend(1u32.to_be_bytes());
    reply.extend(wifi::mac(&config.key, &reply));
    assert_eq!(wifi::capability_reply(&config, nonce, &reply), Some(1));
    for index in [0, 4, 10, 26, 30, 61] {
        let mut corrupt = reply.clone();
        corrupt[index] ^= 1;
        assert_eq!(wifi::capability_reply(&config, nonce, &corrupt), None);
    }
    assert_eq!(wifi::capability_reply(&config, nonce, &reply[..61]), None);
    assert_eq!(wifi::capability_reply(&config, [5; 16], &reply), None);
}
