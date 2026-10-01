use openpilot_paramsd::{
    model,
    types::{clip, diagonal, hysteresis},
    wire,
};

#[test]
fn generated_transition_uses_physical_globals() {
    let x = [1., 15., 0., 0., 20., 0.2, 0.01, 0.1, 0.02];
    let a = model::transition(&x, &[1600., 2700., 1.1, 1.6, 80000., 90000.], 0.05);
    let b = model::transition(&x, &[2400., 4800., 1.4, 1.8, 140000., 150000.], 0.05);
    assert_ne!(a, b);
    assert!(a.iter().all(|v| v.is_finite()));
}
#[test]
fn hysteresis_clipping_and_diagonal_preserve_boundaries() {
    assert!(!hysteresis(true, 10., 10., 8.));
    assert!(!hysteresis(false, 8., 10., 8.));
    assert!(hysteresis(false, 7.99, 10., 8.));
    assert!(clip(f64::NAN, -1., 1.).is_nan());
    assert_eq!(clip(2., -1., 1.), 1.);
    assert_eq!(diagonal(&[0.2, 0.3]), [0.2, 0., 0., 0.3]);
}
#[test]
fn malformed_wire_inputs_return_errors() {
    for size in 0..64 {
        assert!(wire::car(&vec![255; size]).is_err());
        assert!(wire::decode(&vec![255; size]).is_err());
    }
}
