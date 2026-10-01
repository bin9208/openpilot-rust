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
