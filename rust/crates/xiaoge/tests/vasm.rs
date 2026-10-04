use openpilot_xiaoge::vasm::confidence;

#[test]
fn class_filter_uses_ties_even_rounding_and_column_four_confidence() {
    // Given six-column detections containing class zero, a rounded tie, and another class.
    let mut data = vec![0.0; 8 * 6];
    for (row, score, class) in [
        (0, 0.3, 0.0),
        (1, 0.8, 0.5),
        (2, 0.99, 0.50001),
        (3, 0.9, -0.6),
    ] {
        data[row * 6 + 4] = score;
        data[row * 6 + 5] = class;
    }
    // When selecting confidence from the original [1, detections, 6] output.
    let result = confidence(&[1, 8, 6], &data).unwrap();
    // Then the half-even class-zero row wins and unrelated classes are excluded.
    assert_eq!(result, f64::from(0.8f32));
}

#[test]
fn transposed_detection_tensor_preserves_the_selected_confidence() {
    // Given a six-by-eight tensor in channel-major order.
    let mut data = vec![0.0; 6 * 8];
    data[4 * 8 + 3] = 0.75;
    data[4 * 8 + 5] = 0.99;
    data[5 * 8 + 5] = 1.0;
    // When the first squeezed dimension is smaller, the source transposes it.
    let result = confidence(&[1, 6, 8], &data).unwrap();
    // Then class-zero confidence is read from the corresponding transposed row.
    assert_eq!(result, 0.75);
}

#[test]
fn empty_vector_returns_zero_and_shape_mismatch_is_an_error() {
    // Given an empty flat output and a truncated two-dimensional output.
    // When interpreting the tensors, then the source empty default and boundary rejection apply.
    assert_eq!(confidence(&[0], &[]).unwrap(), 0.0);
    assert!(confidence(&[8, 6], &[0.0; 47]).is_err());
}

#[test]
fn nan_in_selected_scores_propagates_like_numpy_maximum() {
    // Given a flat output with a non-finite score.
    let values = [0.8, f32::NAN, 0.99];
    // When reducing scores, then NaN is retained rather than hidden by a finite maximum.
    assert!(confidence(&[3], &values).unwrap().is_nan());
}
