use openpilot_control_policy::{
    drive,
    math::{clip, divide, interp},
    pid::{Gains, Multiplicative, Pid, Step},
    similarity,
};
#[test]
fn interpolation_and_clip_keep_numpy_boundaries() {
    assert!(interp(0., &[], &[]).is_err());
    assert!(divide(1., 0.).is_err());
    assert_eq!(interp(0., &[0., 0., 1.], &[1., 2., 3.]).unwrap(), 2.);
    assert_eq!(interp(f64::NAN, &[0.], &[3.]).unwrap(), 3.);
    assert!(interp(f64::NAN, &[0., 1.], &[3., 4.]).unwrap().is_nan());
    assert_eq!(clip(2., 5., 1.), 1.);
    assert!(clip(f64::NAN, 0., 1.).is_nan());
}
#[test]
fn pid_freeze_and_override_have_distinct_integral_paths() {
    let mut pid = Pid::new(Gains::constants(0.2, 0.3, 1.), [-1., 1.]);
    assert_eq!(pid.update(Step::new(2., 0., 0.)).unwrap(), 0.406);
    let saved = pid.i;
    let mut step = Step::new(-1., 0., 0.);
    step.freeze = true;
    pid.update(step).unwrap();
    assert_eq!(pid.i, saved);
    step.driver_override = true;
    pid.update(step).unwrap();
    assert_eq!(pid.i, saved - 0.003);
    pid.multiplicative = Some(Multiplicative {
        previous_override: false,
        factor: 1.,
        min_command: 1e-10,
        reduction_time: 1.,
    });
    pid.update(step).unwrap();
    let factor = pid.multiplicative.as_ref().unwrap().factor;
    assert!(factor > 0. && factor < 1.);
    pid.reset();
    assert_eq!(pid.i, 0.);
    assert_eq!(pid.multiplicative.as_ref().unwrap().factor, factor);
}
#[test]
fn saturation_blocks_integral_growth_and_recovers() {
    let mut pid = Pid::new(Gains::constants(1., 0.5, 1.), [-1., 1.]);
    for _ in 0..1000 {
        assert_eq!(pid.update(Step::new(10., 0., 0.)).unwrap(), 1.);
    }
    assert_eq!(pid.i, 0.);
    assert_eq!(pid.update(Step::new(-0.1, 0., 0.)).unwrap(), -0.1005);
}
#[test]
fn steer_ratio_and_curvature_do_not_weaken_limits() {
    assert_eq!(drive::steer_ratio(15., 29., 0., false), 15.);
    assert_eq!(drive::steer_ratio(15., 30., 0., false), 4.5);
    assert_eq!(drive::steer_ratio(15., 100., 200., true), 15.);
    let (limited, flag) = drive::clip_curvature(1., 1., 2., 0.).unwrap();
    assert_eq!(limited, 0.2);
    assert!(flag);
}
#[test]
fn model_selection_preserves_first_tie_and_identity_guard() {
    let files = vec!["ABC1.json".into(), "ABC2.json".into(), "OTHER.json".into()];
    assert_eq!(similarity::select(&files, "OTHER", ""), Some("OTHER.json"));
    assert_eq!(similarity::select(&files, "MISSING", ""), None);
    assert_eq!(similarity::ratio("", ""), 1.);
    assert_eq!(similarity::ratio("abcd", "bc"), 2. / 3.);
}
