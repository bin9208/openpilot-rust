use openpilot_cereal::car_capnp::car_params;
use openpilot_pandad::firmware::{tesla_wake::enabled, Mcu};

fn parameters(brand: &str, fingerprint: &str) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut cp = message.init_root::<car_params::Builder>();
    cp.set_brand(brand);
    cp.set_car_fingerprint(fingerprint);
    capnp::serialize::write_message_to_words(&message)
}

#[test]
fn wake_requires_opt_in_and_supported_vehicle_evidence() {
    let supported = parameters("tesla", "TESLA_MODEL_3");
    let unsupported = parameters("tesla", "TESLA_MODEL_X");
    let other = parameters("hyundai", "TESLA_MODEL_3");
    assert!(enabled(true, Some(&supported), None, None));
    assert!(!enabled(false, Some(&supported), None, None));
    assert!(!enabled(
        true,
        Some(&unsupported),
        None,
        Some(b"TESLA_MODEL_3")
    ));
    assert!(!enabled(true, Some(&other), None, None));
    assert!(!enabled(
        true,
        Some(b"invalid"),
        None,
        Some(b"TESLA_MODEL_3")
    ));
    assert!(enabled(true, None, None, Some(b"TESLA_MODEL_Y")));
    assert!(enabled(true, None, Some(&supported), Some(b"MOCK")));
    assert!(!enabled(true, None, None, Some(b"TESLA_MODEL_X")));
    assert!(!enabled(true, None, None, Some(b"TESLA_MODEL_3\xff")));
}

#[test]
fn wake_names_preserve_each_mcu_and_stock_defaults() {
    assert_eq!(Mcu::F4.app_filename_for_wake(false), "panda.bin.signed");
    assert_eq!(Mcu::H7.app_filename_for_wake(false), "panda_h7.bin.signed");
    assert_eq!(
        Mcu::F4.app_filename_for_wake(true),
        "panda_tesla_wake.bin.signed"
    );
    assert_eq!(
        Mcu::H7.app_filename_for_wake(true),
        "panda_h7_tesla_wake.bin.signed"
    );
}
