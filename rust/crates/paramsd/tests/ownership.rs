#![cfg(feature = "solver")]
use openpilot_paramsd::{bridge::ffi, kalman::CarKalman, model, types::diagonal};
const GLOBALS: [f64; 6] = [1600., 2700., 1.1, 1.6, 80000., 90000.];

#[test]
fn snapshots_outlive_filter_and_copied_inputs() -> Result<(), Box<dyn std::error::Error>> {
    for iteration in 0..64 {
        let mut globals = GLOBALS;
        let mut filter = CarKalman::new(&globals)?;
        globals.fill(99.);
        let mut x = model::INITIAL_X;
        x[2] = f64::from(iteration) * 0.001;
        let mut p = diagonal(&[0.2; 9]);
        filter.reset(None, &x, &p)?;
        x.fill(99.);
        p.fill(99.);
        let snapshot = filter.snapshot()?;
        drop(filter);
        assert_eq!(snapshot.x[2], f64::from(iteration) * 0.001);
        assert_eq!(snapshot.covariance[0], 0.2);
        assert!(snapshot.time.is_nan());
    }
    Ok(())
}

#[test]
fn malformed_dimensions_and_kind_leave_state_intact() -> Result<(), Box<dyn std::error::Error>> {
    let p = diagonal(&model::INITIAL_P);
    let q = diagonal(&model::PROCESS_NOISE);
    assert!(ffi::new_filter(&[], &p, &q, &GLOBALS).is_err());
    assert!(ffi::new_filter(&model::INITIAL_X, &p, &q, &[]).is_err());
    let mut filter = ffi::new_filter(&model::INITIAL_X, &p, &q, &GLOBALS)?;
    assert!(filter.pin_mut().reset(&[0.; 8], &p, 1.).is_err());
    assert!(filter.pin_mut().observe(1., 25, &[0.; 2], &[1.]).is_err());
    assert!(filter.pin_mut().observe(1., 999, &[0.], &[1.]).is_err());
    assert!(filter
        .pin_mut()
        .observe(f64::NAN, 25, &[0.], &[1.])
        .is_err());
    let snapshot = filter.snapshot();
    assert_eq!(snapshot.x, model::INITIAL_X);
    assert_eq!(snapshot.covariance, p);
    assert!(snapshot.time.is_nan());
    Ok(())
}

#[test]
fn rewind_owns_measurements_and_pause_clears_history() -> Result<(), Box<dyn std::error::Error>> {
    let mut filter = CarKalman::new(&GLOBALS)?;
    for i in 0..600 {
        assert!(
            filter
                .observe(100. + f64::from(i) * 0.001, 25, 0.01, Some(0.01))?
                .observed
        );
    }
    let before = filter.snapshot()?;
    assert!(!filter.observe(100.001, 25, 0., Some(0.01))?.observed);
    assert_eq!(filter.snapshot()?.x, before.x);
    assert!(filter.observe(100.4, 25, 0.02, Some(0.01))?.observed);
    assert_eq!(filter.snapshot()?.time, before.time);
    filter.pause(200.);
    assert!(!filter.observe(199.5, 25, 0., Some(0.01))?.observed);
    assert!(filter.observe(200., 25, 0., Some(0.01))?.observed);
    Ok(())
}

fn trajectory(globals: &[f64; 6]) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    let mut filter = CarKalman::new(globals)?;
    for i in 0..50 {
        filter.observe(10. + f64::from(i) * 0.05, 26, 0.1, None)?;
        filter.observe(10. + f64::from(i) * 0.05, 25, 0.01, Some(0.01))?;
    }
    Ok(filter.snapshot()?.x)
}

#[test]
fn interleaved_and_concurrent_models_keep_owned_globals() -> Result<(), Box<dyn std::error::Error>>
{
    let mut other = GLOBALS;
    other[0] *= 2.;
    other[4] *= 0.6;
    let expected = [trajectory(&GLOBALS)?, trajectory(&other)?];
    assert_ne!(expected[0], expected[1]);
    let mut filters = [CarKalman::new(&GLOBALS)?, CarKalman::new(&other)?];
    for i in 0..50 {
        for filter in &mut filters {
            filter.observe(10. + f64::from(i) * 0.05, 26, 0.1, None)?;
            filter.observe(10. + f64::from(i) * 0.05, 25, 0.01, Some(0.01))?;
        }
    }
    for (filter, expected) in filters.into_iter().zip(&expected) {
        assert_eq!(&filter.snapshot()?.x, expected);
    }
    let workers: Vec<_> = [GLOBALS, other]
        .into_iter()
        .map(|globals| std::thread::spawn(move || trajectory(&globals).unwrap()))
        .collect();
    for (worker, expected) in workers.into_iter().zip(expected) {
        assert_eq!(worker.join().unwrap(), expected);
    }
    Ok(())
}
