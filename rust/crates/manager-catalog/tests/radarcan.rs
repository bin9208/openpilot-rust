use openpilot_manager_catalog::{
    catalog, ImportConfig, Predicate, RustAvailability, SourceProcess,
};

#[test]
fn radar_candidate_retains_original_source_and_onroad_descriptor() {
    let descriptors = catalog(ImportConfig {
        pc: true,
        tici: false,
        webcam: false,
        carrot_web_external: false,
        darwin: false,
        bodyteleop_available: false,
    });
    let radar = descriptors
        .iter()
        .find(|entry| entry.name == "radarcan")
        .unwrap();
    assert!(matches!(
        radar.rust,
        RustAvailability::Candidate {
            package: "openpilot-radarcan",
            binary: "openpilot-radarcan",
            ..
        }
    ));
    assert_eq!(
        radar.source,
        SourceProcess::Python {
            module: "openpilot.selfdrive.carrot.radar.radarcan"
        }
    );
    assert_eq!(radar.predicate, Predicate::Onroad);
    assert!(radar.enabled);
    assert!(!radar.restart_if_crash && !radar.sigkill && !radar.daemon);
}
