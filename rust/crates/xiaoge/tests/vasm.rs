use openpilot_xiaoge::vasm::confidence;

#[test]
fn occupied_class_is_second_value_after_squeezing() {
    for shape in [vec![2], vec![1, 2], vec![1, 2, 1, 1], vec![1, 1, 3]] {
        let values = if shape.contains(&3) {
            vec![0.99, 0.8, 0.97]
        } else {
            vec![0.99, 0.8]
        };
        assert_eq!(confidence(&shape, &values).unwrap(), f64::from(0.8f32));
    }
}

#[test]
fn detector_tensors_and_invalid_classifier_shapes_are_rejected() {
    for (shape, length) in [
        (vec![1, 8, 6], 48),
        (vec![1, 1], 1),
        (vec![4], 4),
        (vec![0], 0),
        (vec![-2], 2),
        (vec![3], 2),
    ] {
        assert!(confidence(&shape, &vec![0.0; length]).is_err());
    }
}

#[test]
fn selected_nonfinite_value_is_preserved_like_source() {
    assert!(confidence(&[3], &[0.8, f32::NAN, 0.99]).unwrap().is_nan());
    assert_eq!(confidence(&[2], &[f32::NAN, 0.75]).unwrap(), 0.75);
}
