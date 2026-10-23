use openpilot_card::{brands::ford, core::Error, query::DiagnosticLevel};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

#[test]
fn secoc_error_is_emitted_before_parameter_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let fingerprints = [(2, vec![(0x3d6, 16)])];
    let mut logs = Vec::new();
    let cp = ford::parameters_logged(
        ford::ParamsInput {
            candidate: "FORD_F_150_MK14",
            fingerprints: &fingerprints,
            firmware: &[],
            alpha_long: false,
            settings: &settings,
        },
        |log| {
            assert!(settings.get("NNFFModelName")?.is_none());
            assert!(matches!(log.level, DiagnosticLevel::Error));
            logs.push(log.message.clone());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(logs, ["dashcamOnly: SecOC is unsupported"]);
    assert!(cp
        .get_root_as_reader::<car_params::Reader>()
        .unwrap()
        .get_dashcam_only());
}

#[test]
fn exact_ford_angle_overrides_preserve_secoc_rejection_and_no_nnff_effects() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    for candidate in ["FORD_ESCAPE_MK4_5", "FORD_EXPEDITION_MK4"] {
        let mut logs = Vec::new();
        let result = ford::parameters_logged(
            ford::ParamsInput {
                candidate,
                fingerprints: &[(2, vec![(0x3d6, 16)])],
                firmware: &[],
                alpha_long: true,
                settings: &settings,
            },
            |log| {
                logs.push(log.message.clone());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(logs, ["dashcamOnly: SecOC is unsupported"]);
        let cp = result.get_root_as_reader::<car_params::Reader>().unwrap();
        assert!(cp.get_dashcam_only());
        assert_eq!(
            cp.get_steer_control_type().unwrap(),
            car_params::SteerControlType::Angle
        );
        assert!(settings.get("NNFFModelName").unwrap().is_none());
    }
}

#[test]
fn failed_error_sink_does_not_run_later_parameter_writes() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let result = ford::parameters_logged(
        ford::ParamsInput {
            candidate: "FORD_F_150_MK14",
            fingerprints: &[(2, vec![(0x3d6, 16)])],
            firmware: &[],
            alpha_long: false,
            settings: &settings,
        },
        |_| Err(Error::Event("test sink failure")),
    );
    assert!(matches!(result, Err(Error::Event("test sink failure"))));
    assert!(settings.get("NNFFModelName").unwrap().is_none());
}
