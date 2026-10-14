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

fn assert_scalar_snapshot_reads(filter: &PoseKalman) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = filter.snapshot()?;
    assert_eq!(filter.filter_time()?.to_bits(), snapshot.time.to_bits());
    assert_eq!(filter.gyro_bias_z()?.to_bits(), snapshot.x[11].to_bits());
    Ok(())
}

#[test]
fn scalar_reads_match_owned_snapshots_through_reset_and_nonfinite_seeds(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut filter = PoseKalman::new()?;
    let independent = PoseKalman::new()?;
    assert_scalar_snapshot_reads(&filter)?;
    for (time, bias) in [
        (None, -0.0),
        (Some(0.0), 0.25),
        (Some(-0.0), -0.5),
        (Some(17.25), f64::INFINITY),
        (Some(f64::NEG_INFINITY), f64::NEG_INFINITY),
        (
            Some(f64::from_bits(0x7ff8_0000_0000_1234)),
            f64::from_bits(0x7ff8_0000_0000_5678),
        ),
    ] {
        let mut x = model::INITIAL_X;
        x[11] = bias;
        filter.reset(time, &x, &diagonal::<18, 324>(model::INITIAL_P))?;
        assert_scalar_snapshot_reads(&filter)?;
        assert_eq!(filter.gyro_bias_z()?.to_bits(), bias.to_bits());
        assert_scalar_snapshot_reads(&independent)?;
        assert!(independent.filter_time()?.is_nan());
        assert_eq!(
            independent.gyro_bias_z()?.to_bits(),
            model::INITIAL_X[11].to_bits()
        );
    }
    filter.reset_default(Some(200.))?;
    assert_scalar_snapshot_reads(&filter)?;
    Ok(())
}

#[test]
fn scalar_reads_follow_current_state_after_delayed_replay_not_returned_estimate(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut filter = PoseKalman::new()?;
    for i in 0..40 {
        let time = 100. + f64::from(i) * 0.01;
        assert!(filter.observe(time, 4, [0.2, -0.1, 0.3], None)?.observed);
        assert!(
            filter
                .observe(time, 14, [0.05, -0.04, 0.08], Some([0.0001; 3]))?
                .observed
        );
        assert_scalar_snapshot_reads(&filter)?;
    }
    let latest = filter.filter_time()?;
    let delayed = filter.observe(100.15, 4, [0.5, 0.4, -0.2], None)?;
    assert!(delayed.observed);
    assert_eq!(filter.filter_time()?.to_bits(), latest.to_bits());
    assert_ne!(
        filter.gyro_bias_z()?.to_bits(),
        delayed.x[11].to_bits(),
        "fixture must distinguish the returned delayed Estimate from replayed current state"
    );
    assert_scalar_snapshot_reads(&filter)?;
    let before = filter.snapshot()?;
    assert!(!filter.observe(98., 4, [0.; 3], None)?.observed);
    assert_eq!(filter.snapshot()?.x, before.x);
    assert_scalar_snapshot_reads(&filter)?;
    filter.reset_default(Some(200.))?;
    assert!(!filter.observe(199.5, 4, [0.; 3], None)?.observed);
    assert_scalar_snapshot_reads(&filter)?;
    assert!(filter.observe(200., 4, [0.; 3], None)?.observed);
    assert_scalar_snapshot_reads(&filter)?;
    Ok(())
}
