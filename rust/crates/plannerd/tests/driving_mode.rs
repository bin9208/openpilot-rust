use openpilot_plannerd::{
    driving_mode::{DrivingMode, DrivingModeDetector, TrafficSample},
    lead::Lead,
};

fn stopped() -> TrafficSample {
    TrafficSample {
        valid: true,
        dt: 0.05,
        ego_speed: 5.,
        lead: Lead {
            status: true,
            radar: true,
            radar_track_id: 1,
            d_rel: 12.,
            v_lead: 0.,
            ..Lead::default()
        },
    }
}

#[test]
fn invalid_sample_clears_evidence_without_erasing_congestion() {
    // Given: stopping evidence has selected Safe mode.
    let mut detector = DrivingModeDetector::default();
    for _ in 0..6 {
        detector.update(&stopped());
    }
    let mut invalid = stopped();
    invalid.valid = false;
    // When: an invalid input interrupts the stopped-lead stream.
    detector.update(&invalid);
    // Then: absence of evidence cannot declare open-road recovery.
    assert_eq!(detector.mode(1), DrivingMode::Safe);
}

#[test]
fn missing_lead_at_standstill_does_not_clear_queue_history() {
    // Given: confirmed congestion followed by a disappearing stopped lead.
    let mut detector = DrivingModeDetector::default();
    for _ in 0..6 {
        detector.update(&stopped());
    }
    let mut missing = stopped();
    missing.lead.status = false;
    missing.ego_speed = 0.;
    // When: the disappearance persists while ego remains stationary.
    for _ in 0..100 {
        detector.update(&missing);
    }
    // Then: Safe mode remains selected.
    assert_eq!(detector.mode(2), DrivingMode::Safe);
}
