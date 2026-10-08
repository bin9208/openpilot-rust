use openpilot_manager_catalog::{
    catalog, ImportConfig, Predicate, RustAvailability, SourceProcess,
};

#[test]
fn xiaoge_candidate_preserves_source_registration_and_share_data_predicate() {
    for tici in [false, true] {
        let processes = catalog(ImportConfig {
            pc: !tici,
            tici,
            webcam: false,
            carrot_web_external: false,
            darwin: false,
            bodyteleop_available: false,
        });
        let descriptor = processes
            .iter()
            .find(|entry| entry.name == "xiaoge_data")
            .unwrap();
        assert_eq!(
            descriptor.source,
            SourceProcess::Python {
                module: "openpilot.selfdrive.carrot.xiaoge_data",
            }
        );
        assert!(matches!(descriptor.predicate, Predicate::ShareData));
        assert!(descriptor.enabled);
        assert!(!descriptor.restart_if_crash && !descriptor.sigkill && !descriptor.daemon);
        assert!(matches!(
            descriptor.rust,
            RustAvailability::Candidate {
                package: "openpilot-xiaoge",
                binary: "openpilot-xiaoge",
                ..
            }
        ));
    }
}
