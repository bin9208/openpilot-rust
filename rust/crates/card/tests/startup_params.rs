use openpilot_card::{
    identification::{FingerprintSource, Identification},
    startup, vehicle_params,
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use std::sync::Arc;

fn identification() -> Identification {
    Identification {
        candidate: "HYUNDAI_IONIQ_5".into(),
        observed: vec![(0, vec![(123, 8)])],
        vin: "00000000000000000".into(),
        firmware: vec![],
        source: FingerprintSource::Fixed,
        exact_match: true,
        cached: false,
        vin_rx_address: None,
        vin_rx_bus: None,
        ecu_responses: vec![],
        fw_query_time: 0.,
        packets: 202,
    }
}

#[test]
fn startup_storage_and_passive_safety_preserve_route_handoff() {
    let root = tempfile::tempdir().unwrap();
    let settings = Arc::new(Params::open(root.path(), "d").unwrap());
    settings
        .put("CarParamsPersistent", b"previous route")
        .unwrap();
    let identification = identification();
    startup::save_identification(&settings, &identification).unwrap();
    let message = vehicle_params::baseline(&identification.candidate).unwrap();
    let prepared = startup::prepare(&settings, &identification, message, true, None).unwrap();
    assert!(settings.get("CarParamsCache").unwrap().is_none());
    let mut writes = openpilot_card::async_params::AsyncParams::new(Arc::clone(&settings));
    writes.put("CarParamsCache", &prepared.bytes).unwrap();
    writes.put("CarParamsPersistent", &prepared.bytes).unwrap();
    writes.finish();
    assert!(writes.failures().is_empty());
    let cp = prepared
        .params
        .get_root_as_reader::<car_params::Reader>()
        .unwrap();
    assert!(cp.get_passive());
    assert_eq!(
        cp.get_safety_configs()
            .unwrap()
            .get(0)
            .get_safety_model()
            .unwrap(),
        car_params::SafetyModel::NoOutput
    );
    assert_eq!(cp.get_alternative_experience(), 1);
    assert_eq!(
        settings.get("CarParamsPrevRoute").unwrap().unwrap(),
        b"previous route"
    );
    assert_eq!(settings.get("CarParams").unwrap().unwrap(), prepared.bytes);
    assert_eq!(
        settings.get("CarParamsCache").unwrap().unwrap(),
        prepared.bytes
    );
    assert_eq!(
        settings.get("CarName").unwrap().unwrap(),
        b"HYUNDAI_IONIQ_5"
    );
    assert_eq!(
        settings.get("FingerPrints").unwrap().unwrap(),
        b"{0: {123: 8}}"
    );
    assert!(settings.get_bool("FirmwareQueryDone").unwrap());
}

#[test]
fn secoc_key_is_supplied_to_vehicle_only_after_validation() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    settings.put_bool("OpenpilotEnabledToggle", true).unwrap();
    let identification = identification();
    let mut message = vehicle_params::baseline(&identification.candidate).unwrap();
    message
        .get_root::<car_params::Builder>()
        .unwrap()
        .set_sec_oc_required(true);
    let prepared = startup::prepare(
        &settings,
        &identification,
        message,
        true,
        Some("00112233445566778899aabbccddeeff"),
    )
    .unwrap();
    assert_eq!(
        prepared.secoc_key.unwrap(),
        [0, 17, 34, 51, 68, 85, 102, 119, 136, 153, 170, 187, 204, 221, 238, 255]
    );
    assert!(!prepared
        .params
        .get_root_as_reader::<car_params::Reader>()
        .unwrap()
        .get_passive());
    assert!(prepared.warnings.is_empty());
}

#[test]
fn invalid_saved_secoc_length_warns_before_route_params_are_written() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    settings.put("SecOCKey", b"0011").unwrap();
    settings
        .put("CarParamsPersistent", b"previous route")
        .unwrap();
    let identification = identification();
    let mut message = vehicle_params::baseline(&identification.candidate).unwrap();
    message
        .get_root::<car_params::Builder>()
        .unwrap()
        .set_sec_oc_required(true);
    let mut warnings = Vec::new();
    let prepared = startup::prepare_logged(
        startup::Preparation {
            settings: &settings,
            identification: &identification,
            message,
            has_controller: true,
            user_key: None,
        },
        |warning| {
            assert!(settings.get("CarParams").unwrap().is_none());
            assert!(settings.get("CarParamsPrevRoute").unwrap().is_none());
            warnings.push(warning.to_owned());
        },
    )
    .unwrap();
    assert_eq!(warnings, ["Saved SecOC key is invalid"]);
    assert_eq!(prepared.warnings, ["Saved SecOC key is invalid"]);
    assert!(prepared.secoc_key.is_none());
    assert!(!prepared
        .params
        .get_root_as_reader::<car_params::Reader>()
        .unwrap()
        .get_sec_oc_key_available());
    assert_eq!(
        settings.get("CarParamsPrevRoute").unwrap().unwrap(),
        b"previous route"
    );
}

#[test]
fn malformed_saved_secoc_hex_fails_without_a_length_warning() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    settings.put("SecOCKey", b"badkey").unwrap();
    let identification = identification();
    let mut message = vehicle_params::baseline(&identification.candidate).unwrap();
    message
        .get_root::<car_params::Builder>()
        .unwrap()
        .set_sec_oc_required(true);
    let mut warnings = Vec::new();
    let result = startup::prepare_logged(
        startup::Preparation {
            settings: &settings,
            identification: &identification,
            message,
            has_controller: true,
            user_key: None,
        },
        |warning| warnings.push(warning.to_owned()),
    );
    assert!(matches!(result, Err(startup::Error::SecocHex)));
    assert!(warnings.is_empty());
    assert!(settings.get("CarParams").unwrap().is_none());
}
