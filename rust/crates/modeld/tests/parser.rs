use openpilot_modeld::parse::{sigmoid, softmax, RawOutputs};
use std::collections::BTreeMap;

#[test]
fn rejects_bad_slice_before_parsing() {
    let layout = BTreeMap::from([("pose".to_owned(), [0, 13])]);
    let values = [0.0; 12];
    assert!(RawOutputs::new(&values, &layout).is_err());
}

#[test]
fn keeps_numpy_clip_and_nan_semantics() {
    assert!((sigmoid(-100.0) - 1.0 / (1.0 + 11_f32.exp())).abs() < 1e-10);
    assert!(sigmoid(f32::NAN).is_nan());
    let mut logits = [1.0, f32::NAN];
    softmax(&mut logits);
    assert!(logits.iter().all(|value| value.is_nan()));
}

#[test]
fn lead_tie_selects_last_hypothesis_like_reversed_numpy_argsort() {
    let layout = BTreeMap::from([("lead".to_owned(), [0, 102])]);
    let mut values = [0.0; 102];
    values[0] = 10.0;
    values[51] = 20.0;
    let raw = RawOutputs::new(&values, &layout).unwrap();
    let leads = raw.leads().unwrap();
    assert_eq!(leads.mean[0], 20.0);
    assert_eq!(leads.mean[24], 20.0);
    assert_eq!(leads.mean[48], 20.0);
}
