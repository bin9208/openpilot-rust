use openpilot_xiaoge::lane::{select, Candidate, Head, ResultData, MASK_CHANNELS, PROTO_SIZE};
use openpilot_xiaoge::Error;

// Frozen deployed implementation, not an oracle derived from the optimized code.
#[path = "support/lane_head_0682e933.rs"]
mod baseline;
#[path = "support/compute_cases.rs"]
mod inputs;

#[test]
fn optimized_masks_preserve_deployed_candidates_including_float_cancellation() {
    for case in inputs::cases() {
        let expected = baseline::Head::new(&case.predictions, &case.prototypes)
            .unwrap()
            .candidates(0.25, 0.5)
            .unwrap();
        let actual = Head::new(&case.predictions, &case.prototypes)
            .unwrap()
            .candidates(0.25, 0.5)
            .unwrap();
        let bits = |items: &[Candidate]| {
            items
                .iter()
                .map(|item| {
                    (
                        item.class_id,
                        item.score.to_bits(),
                        item.bottom,
                        item.center.to_bits(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&actual), bits(&expected), "{}", case.name);
        assert_eq!(
            serde_json::to_value(select(&actual).unwrap()).unwrap(),
            serde_json::to_value(
                baseline::Head::new(&case.predictions, &case.prototypes)
                    .unwrap()
                    .result(0.25, 0.5)
                    .unwrap()
            )
            .unwrap(),
            "{}",
            case.name
        );
    }
}

#[test]
fn cutoff_row_is_inclusive_and_an_upper_positive_mask_cannot_create_a_boundary() {
    let mut predictions = vec![0.0; 42];
    for (index, value) in [
        (0, 208.0),
        (1, 208.0),
        (2, 416.0),
        (3, 416.0),
        (4, 0.9),
        (10, 1.0),
    ] {
        predictions[index] = value;
    }
    let mut prototypes = vec![0.0; 32 * 104 * 104];
    prototypes[52 * 104 + 7] = 1.0;
    let candidates = Head::new(&predictions, &prototypes)
        .unwrap()
        .candidates(0.25, 0.5)
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!((candidates[0].bottom, candidates[0].center), (52, 7.0));
    prototypes[52 * 104 + 7] = 0.0;
    prototypes[51 * 104 + 7] = 1.0;
    assert!(Head::new(&predictions, &prototypes)
        .unwrap()
        .candidates(0.25, 0.5)
        .unwrap()
        .is_empty());
}
