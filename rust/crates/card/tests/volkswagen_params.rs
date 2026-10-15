use openpilot_card::brands::volkswagen::{parameters, ParamsInput};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

#[test]
fn meb_uses_single_zero_integral_gain_and_preserves_timing() {
    for candidate in ["VOLKSWAGEN_ID4_MK1", "VOLKSWAGEN_ID4_MK2"] {
        let root = tempfile::tempdir().unwrap();
        let settings = Params::open(root.path(), "d").unwrap();
        let message = parameters(ParamsInput {
            candidate,
            fingerprints: &[],
            firmware: &[],
            alpha_long: false,
            settings: &settings,
        })
        .unwrap();
        let cp = message.get_root_as_reader::<car_params::Reader>().unwrap();
        let tuning = cp.get_longitudinal_tuning().unwrap();
        let bp = tuning.get_ki_b_p().unwrap();
        let values = tuning.get_ki_v().unwrap();
        assert_eq!(bp.len(), 1, "{candidate}");
        assert_eq!(values.len(), 1, "{candidate}");
        assert_eq!(bp.get(0), 0.);
        assert_eq!(values.get(0), 0.);
        assert_eq!(cp.get_radar_delay(), 0.8);
        assert_eq!(cp.get_longitudinal_actuator_delay(), 0.5);
    }
}
