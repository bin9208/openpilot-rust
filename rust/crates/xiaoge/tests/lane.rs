use openpilot_xiaoge::lane::{select, Candidate, Head};

#[test]
fn selection_uses_bottom_center_distance_before_confidence() {
    // Given a confident distant line and a less confident nearby line on each side.
    let candidates = [
        Candidate {
            class_id: 0,
            score: 0.95,
            bottom: 80,
            center: 10.0,
        },
        Candidate {
            class_id: 1,
            score: 0.4,
            bottom: 103,
            center: 50.0,
        },
        Candidate {
            class_id: 2,
            score: 0.5,
            bottom: 103,
            center: 52.0,
        },
        Candidate {
            class_id: 5,
            score: 0.99,
            bottom: 60,
            center: 70.0,
        },
    ];
    // When selecting the current boundaries.
    let result = select(&candidates).unwrap();
    // Then the nearest dashed left and solid right win, including center=52 on the right.
    assert_eq!((result.left_line, result.right_line), (0, 1));
    assert_eq!((result.left_conf, result.right_conf), (0.4, 0.5));
}

#[test]
fn overlapping_classes_are_retained_but_same_class_is_suppressed() {
    // Given three overlapping detections, two sharing a class, and positive prototypes.
    let mut predictions = vec![0.0; 42 * 3];
    for anchor in 0..3 {
        for (row, value) in [(0, 120.0), (1, 300.0), (2, 80.0), (3, 160.0), (10, 1.0)] {
            predictions[row * 3 + anchor] = value;
        }
    }
    predictions[4 * 3] = 0.9;
    predictions[4 * 3 + 1] = 0.8;
    predictions[5 * 3 + 2] = 0.7;
    let mut prototypes = vec![0.0; 32 * 104 * 104];
    prototypes[..104 * 104].fill(1.0);
    // When decoding the ONNX head with the original thresholds.
    let candidates = Head::new(&predictions, &prototypes)
        .unwrap()
        .candidates(0.25, 0.5)
        .unwrap();
    // Then NMS retains both classes in score order and removes the duplicate solid box.
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.class_id)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(candidates[0].bottom, 95);
    assert_eq!(candidates[0].center, 30.0);
}

#[test]
fn empty_masks_and_ignored_classes_never_become_lane_boundaries() {
    // Given a confident ignored class and a solid line with an all-negative mask.
    let mut predictions = vec![0.0; 42 * 2];
    for anchor in 0..2 {
        for (row, value) in [(0, 120.0), (1, 300.0), (2, 80.0), (3, 160.0), (10, -1.0)] {
            predictions[row * 2 + anchor] = value;
        }
    }
    predictions[7 * 2] = 0.9;
    predictions[4 * 2 + 1] = 0.8;
    let mut prototypes = vec![0.0; 32 * 104 * 104];
    prototypes[..104 * 104].fill(1.0);
    // When producing the lane result.
    let result = Head::new(&predictions, &prototypes)
        .unwrap()
        .result(0.25, 0.5)
        .unwrap();
    // Then inference is valid but neither side invents a line.
    assert!(result.valid);
    assert_eq!(
        (result.left_line, result.right_line, result.candidates_count),
        (-1, -1, 0)
    );
}

#[test]
fn malformed_tensors_are_rejected_before_indexing() {
    // Given prediction channels or prototype storage with the wrong dimensions.
    let prototypes = vec![0.0; 32 * 104 * 104];
    // When parsing the output boundary, then short tensors return a typed error.
    assert!(Head::new(&[0.0; 41], &prototypes).is_err());
    assert!(Head::new(&[0.0; 42], &prototypes[..prototypes.len() - 1]).is_err());
}
