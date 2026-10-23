use openpilot_card::brands::ford::{self, ParamsInput};
use openpilot_cereal::car_capnp::{car_params, car_params::SteerControlType};
use openpilot_params::Params;

#[test]
fn exact_ford_placeholders_admit_only_finite_angle_parameters_even_with_nnff_requested() {
    for candidate in ["FORD_ESCAPE_MK4_5", "FORD_EXPEDITION_MK4"] {
        for nnff in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let settings = Params::open(root.path(), "d").unwrap();
            settings.put_bool("NNFF", nnff).unwrap();
            let cp = ford::parameters(ParamsInput {
                candidate,
                fingerprints: &[],
                firmware: &[],
                alpha_long: false,
                settings: &settings,
            })
            .unwrap();
            let cp = cp.get_root_as_reader::<car_params::Reader>().unwrap();
            assert_eq!(
                cp.get_steer_control_type().unwrap(),
                SteerControlType::Angle
            );
            assert_eq!(cp.get_max_lateral_accel(), 1.5);
            assert!(matches!(
                cp.get_lateral_tuning().which().unwrap(),
                car_params::lateral_tuning::Which::Pid(_)
            ));
            for value in [
                cp.get_mass(),
                cp.get_wheelbase(),
                cp.get_center_to_front(),
                cp.get_steer_ratio(),
                cp.get_rotational_inertia(),
                cp.get_tire_stiffness_front(),
                cp.get_tire_stiffness_rear(),
                cp.get_steer_actuator_delay(),
                cp.get_max_lateral_accel(),
            ] {
                assert!(value.is_finite());
            }
            assert!(settings.get("NNFFModelName").unwrap().is_none());
        }
    }
}
