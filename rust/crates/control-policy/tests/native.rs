#![cfg(feature = "numerics")]
use openpilot_control_policy::{flux::Flux, numerics::Numerics};
use std::path::PathBuf;
#[test]
fn native_owned_buffers_dimensions_and_library_lifetime() {
    let path = PathBuf::from(
        std::env::var_os("CONTROLS_NUMERICS").expect("pinned numerical artifact required"),
    );
    for _ in 0..128 {
        let kernel = Numerics::load(&path).unwrap();
        assert_eq!(
            kernel
                .float_matrix(&[2., 3.], 3, &[1., 4., 2., 5., 3., 6.])
                .unwrap(),
            [14., 19., 24.]
        );
        assert_eq!(
            kernel
                .double_matrix(&[2., 3.], 3, &[1., 2., 3., 4., 5., 6.])
                .unwrap(),
            [14., 19., 24.]
        );
        assert_eq!(kernel.float_matrix(&[2., 3.], 1, &[1., 4.]).unwrap(), [14.]);
        assert_eq!(
            kernel.double_matrix(&[2., 3.], 1, &[1., 4.]).unwrap(),
            [14.]
        );
        for columns in [0, 1, 3, usize::MAX] {
            assert!(kernel.float_matrix(&[], columns, &[]).is_err());
            assert!(kernel.double_matrix(&[1., 2.], columns, &[0.]).is_err());
        }
        assert!(Flux::decode(
            br#"{"input_size":2,"input_mean":[[0],[0]],"input_std":[[1],[1]],"layers":[]}"#,
            &kernel
        )
        .is_err());
        assert!(Flux::decode(br#"{"input_size":2,"input_mean":[[0],[0]],"input_std":[[1],[1]],"layers":[{"activation":"identity","bad_W":[[1]],"bad_b":[[0]]}]}"#, &kernel).is_err());
    }
}

#[test]
fn nano_loads_validated_layers_once_and_reuses_them() {
    use openpilot_control_policy::nano::Nano;
    let weights = serde_json::json!({
        "w_1": [[1.,2.],[3.,4.]], "b_1": [0.,0.],
        "w_2": [[1.,0.],[0.,1.]], "b_2": [0.,0.],
        "w_3": [[1.,0.],[0.,1.]], "b_3": [0.,0.],
        "w_4": [[2.,0.],[3.,1.]], "b_4": [0.,0.],
        "input_norm_mat": [[0.,1.],[0.,1.]], "output_norm_mat": [0.,1.]
    });
    let model: Nano = serde_json::from_value(weights.clone()).unwrap();
    let kernel = Numerics::load(&PathBuf::from(
        std::env::var_os("CONTROLS_NUMERICS").unwrap(),
    ))
    .unwrap();
    for _ in 0..128 {
        assert_eq!(model.predict(&[2., 3.], &kernel).unwrap(), 70.);
    }
    for key in ["w_1", "w_2", "w_3", "w_4", "b_1", "b_2", "b_3", "b_4"] {
        let mut malformed = weights.clone();
        malformed[key] = serde_json::json!([]);
        assert!(serde_json::from_value::<Nano>(malformed).is_err(), "{key}");
    }
}
