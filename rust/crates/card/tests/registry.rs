use openpilot_card::{
    core::Error,
    identification::{FingerprintSource, Identification},
    registry, startup,
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

fn identification(candidate: &str) -> Identification {
    Identification {
        candidate: candidate.into(),
        observed: vec![],
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
fn psa_registry_preserves_source_fatal_boundaries() {
    // Given the pinned PSA identity, without invented torque values or a DBC.
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let identification = identification("PSA_PEUGEOT_208");

    // When normal parameter construction goes through the shared registry.
    let result = registry::parameters(&identification, &settings, false);

    // Then the original missing torque boundary is retained as a typed failure.
    assert!(matches!(result, Err(Error::Psa(
        openpilot_card::brands::psa::Error::Baseline(
            openpilot_card::vehicle_params::Error::MissingTorque(candidate)
        )
    )) if candidate == "PSA_PEUGEOT_208"));
    assert!(settings.get("NNFFModelName").unwrap().is_none());
}

#[test]
fn psa_registry_constructor_retains_missing_dbc_error() {
    // Given candidate-only CarParams, matching the source boundary probe.
    let root = tempfile::tempdir().unwrap();
    let mut params = openpilot_card::core::Message::new_default();
    params
        .init_root::<car_params::Builder>()
        .set_car_fingerprint("PSA_PEUGEOT_208");
    let bytes = capnp::serialize::write_message_to_words(&params);
    let identification = identification("PSA_PEUGEOT_208");

    // When the registry selects the explicit PSA fatal constructor boundary.
    let result = registry::Interface::new(registry::Setup {
        identification: &identification,
        params_bytes: &bytes,
        dbc_root: root.path(),
        settings: Params::open(root.path(), "d").unwrap(),
        now_ns: 0,
    });

    // Then there is no constructible successful interface or fallback model.
    assert!(matches!(result, Err(Error::Psa(
        openpilot_card::brands::psa::Error::DbcLoad {
            source: openpilot_can::Error::Io(ref cause), ..
        }
    )) if cause.kind() == std::io::ErrorKind::NotFound));
}

#[test]
fn native_registry_preserves_brand_parameters_and_rejects_unknown_models() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    for (candidate, brand) in [
        ("COMMA_BODY", "body"),
        ("MOCK", "mock"),
        ("HYUNDAI_IONIQ_5", "hyundai"),
        ("TESLA_MODEL_3", "tesla"),
        ("MAZDA_CX5_2022", "mazda"),
        ("NISSAN_XTRAIL", "nissan"),
        ("CHRYSLER_PACIFICA_2018", "chrysler"),
        ("RIVIAN_R1_GEN1", "rivian"),
        ("FORD_F_150_MK14", "ford"),
        ("SUBARU_ASCENT", "subaru"),
        ("TOYOTA_PRIUS", "toyota"),
        ("CHEVROLET_VOLT", "gm"),
        ("HONDA_CIVIC", "honda"),
        ("VOLKSWAGEN_GOLF_MK7", "volkswagen"),
        ("VOLKSWAGEN_PASSAT_NMS", "volkswagen"),
        ("VOLKSWAGEN_ID4_MK1", "volkswagen"),
    ] {
        let identification = identification(candidate);
        let mut message = registry::parameters(&identification, &settings, false).unwrap();
        startup::decorate(
            &identification,
            message.get_root::<car_params::Builder>().unwrap(),
        )
        .unwrap();
        let cp = message.get_root_as_reader::<car_params::Reader>().unwrap();
        assert_eq!(cp.get_brand().unwrap().to_str().unwrap(), brand);
        assert_eq!(
            cp.get_car_vin().unwrap().to_str().unwrap(),
            identification.vin
        );
        assert!(!settings.get_bool("FirmwareQueryDone").unwrap());
    }
    assert!(registry::parameters(&identification("invented-car"), &settings, false).is_err());
    assert!(matches!(
        registry::parameters(&identification("TESLA_MODEL_X"), &settings, false),
        Err(Error::Tesla(
            openpilot_card::brands::tesla::Error::Baseline(
                openpilot_card::vehicle_params::Error::MissingTorque(_)
            )
        ))
    ));
}
