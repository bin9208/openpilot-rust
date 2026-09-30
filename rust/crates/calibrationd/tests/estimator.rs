use openpilot_calibrationd::{Calibrator, Limits, Odometry, Seed, Status};

fn sample() -> Odometry {
    Odometry {
        trans: vec![10.0, 0.0, 0.0],
        rot: vec![0.0; 3],
        trans_std: vec![0.0; 3],
        wide: vec![0.01, 0.02, 0.03],
        road: vec![0.0, 0.0, 1.5],
        road_std: vec![0.0; 3],
    }
}

#[test]
fn source_history_excludes_current_block_and_persists_fifth_completed_block() {
    let mut calibrator = Calibrator::new(Limits::standard(), Seed::default()).unwrap();
    calibrator.v_ego = 10.0;
    for index in 1..=500 {
        let update = calibrator.update(&sample()).unwrap();
        assert_eq!(update.persist, index == 500);
        if index == 100 {
            assert_eq!(calibrator.valid_indices(), vec![0]);
        }
    }
    assert_eq!(calibrator.status, Status::Calibrated);
    assert_eq!(calibrator.valid_blocks, 5);
    assert_eq!(calibrator.height, 1.5);
}

#[test]
fn saved_arrays_follow_source_fallback_and_error_boundaries() {
    let seed = Seed {
        rpy: vec![f64::NAN],
        wide: vec![1.0],
        height: vec![],
        ..Seed::default()
    };
    let calibrator = Calibrator::new(Limits::standard(), seed).unwrap();
    assert_eq!(calibrator.rpy, vec![0.0; 3]);
    assert_eq!(calibrator.wide, [0.0; 3]);
    assert_eq!(calibrator.height, 1.22);
    assert!(Calibrator::new(
        Limits::standard(),
        Seed {
            valid_blocks: 51,
            ..Seed::default()
        }
    )
    .is_err());
    let mut malformed = Calibrator::new(
        Limits::standard(),
        Seed {
            rpy: vec![0.0; 2],
            ..Seed::default()
        },
    )
    .unwrap();
    assert_eq!(malformed.rpy.len(), 2);
    malformed.v_ego = 10.0;
    assert!(malformed.update(&sample()).is_err());
}

#[test]
fn calibrated_uncertain_samples_reject_and_yaw_trim_requires_calibrated_status() {
    let mut calibrator = Calibrator::new(
        Limits::standard(),
        Seed {
            valid_blocks: 5,
            ..Seed::default()
        },
    )
    .unwrap();
    calibrator.v_ego = 10.0;
    let mut input = sample();
    input.trans_std[1] = 1.0;
    assert!(calibrator.update(&input).unwrap().rpy.is_none());
    assert!(calibrator.frozen(0.01));
    assert!(!calibrator.frozen(1e-6));
    assert!(!calibrator.frozen(f64::NAN));
}
