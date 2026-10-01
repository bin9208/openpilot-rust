#![cfg(feature = "solver")]
use openpilot_locationd::{bridge::ffi, kalman::PoseKalman, model, types::diagonal};

#[test]
fn snapshots_outlive_filter_and_reset_inputs() -> Result<(), Box<dyn std::error::Error>> {
    for iteration in 0..64 {
        let mut filter = PoseKalman::new()?;
        let mut x = model::INITIAL_X;
        x[9] = f64::from(iteration) * 0.001;
        let mut covariance = diagonal::<18, 324>([0.2; 18]);
        filter.reset(None, &x, &covariance)?;
        x.fill(99.);
        covariance.fill(99.);
        let snapshot = filter.snapshot()?;
        drop(filter);
        assert_eq!(snapshot.x[9], f64::from(iteration) * 0.001);
        assert_eq!(snapshot.covariance[0], 0.2);
        assert!(snapshot.time.is_nan());
    }
    Ok(())
}

#[test]
fn malformed_boundary_dimensions_and_kind_leave_state_intact(
) -> Result<(), Box<dyn std::error::Error>> {
    let p = diagonal::<18, 324>(model::INITIAL_P);
    let q = diagonal::<18, 324>(model::PROCESS_NOISE);
    assert!(ffi::new_filter(&[], &p, &q).is_err());
    let mut filter = ffi::new_filter(&model::INITIAL_X, &p, &q)?;
    assert!(filter.pin_mut().reset(&[0.; 17], &p, 1.).is_err());
    assert!(filter.pin_mut().observe(1., 4, &[0.; 2], &[0.; 9]).is_err());
    assert!(filter
        .pin_mut()
        .observe(1., 999, &[0.; 3], &[0.; 9])
        .is_err());
    assert!(filter
        .pin_mut()
        .observe(f64::NAN, 4, &[0.; 3], &[0.; 9])
        .is_err());
    let snapshot = filter.snapshot();
    assert_eq!(snapshot.x, model::INITIAL_X);
    assert_eq!(snapshot.covariance, p);
    assert!(snapshot.time.is_nan());
    Ok(())
}

#[test]
fn rewind_replays_owned_measurements_and_reset_clears_history(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut filter = PoseKalman::new()?;
    for i in 0..600 {
        let result = filter.observe(100. + f64::from(i) * 0.001, 4, [0.01, -0.02, 0.03], None)?;
        assert!(result.observed);
    }
    let before = filter.snapshot()?;
    assert!(!filter.observe(100.001, 4, [0.; 3], None)?.observed);
    assert_eq!(filter.snapshot()?.x, before.x);
    assert!(
        filter
            .observe(100.4, 4, [0.02, -0.04, 0.06], None)?
            .observed
    );
    let after = filter.snapshot()?;
    assert_eq!(after.time, before.time);
    assert_ne!(after.x, before.x);
    filter.reset_default(Some(200.))?;
    assert!(!filter.observe(199.5, 4, [0.; 3], None)?.observed);
    assert!(filter.observe(200., 4, [0.; 3], None)?.observed);
    Ok(())
}

#[test]
fn separate_filters_keep_independent_states() -> Result<(), Box<dyn std::error::Error>> {
    let mut first = PoseKalman::new()?;
    let second = PoseKalman::new()?;
    first.observe(20., 4, [0.1, 0.2, 0.3], None)?;
    assert_ne!(first.snapshot()?.x, second.snapshot()?.x);
    assert!(second.snapshot()?.time.is_nan());
    Ok(())
}
