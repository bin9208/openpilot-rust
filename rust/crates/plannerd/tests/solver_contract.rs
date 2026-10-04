use openpilot_plannerd::solver::{Field, Kind};

#[test]
fn terminal_state_survives_control_horizon_boundary() {
    let kind = Kind::Lateral;
    let state = Field::State.shape(kind, 32).unwrap();
    assert_eq!(state, [4, 0]);
    assert!(Field::Control.shape(kind, 32).is_err());
}

#[test]
fn terminal_cost_uses_reduced_reference_and_weight_dimensions() {
    for (kind, horizon, costs) in [(Kind::Lateral, 32, 3), (Kind::Longitudinal, 12, 5)] {
        assert_eq!(Field::Reference.shape(kind, horizon).unwrap(), [costs, 0]);
        assert_eq!(Field::Weights.shape(kind, horizon).unwrap(), [costs, costs]);
        assert!(Field::Reference.shape(kind, horizon + 1).is_err());
    }
}

#[test]
fn slack_cannot_be_written_to_lateral_or_terminal_cost() {
    assert!(Field::LowerSlack.shape(Kind::Lateral, 0).is_err());
    assert!(Field::LowerSlack.shape(Kind::Longitudinal, 12).is_err());
    assert_eq!(
        Field::LowerSlack.shape(Kind::Longitudinal, 11).unwrap(),
        [4, 0]
    );
}

#[test]
fn bounds_are_only_available_for_the_initial_state() {
    assert_eq!(
        Field::LowerBound.shape(Kind::Longitudinal, 0).unwrap(),
        [3, 0]
    );
    assert!(Field::LowerBound.shape(Kind::Longitudinal, 1).is_err());
    assert!(Field::UpperBound.shape(Kind::Lateral, usize::MAX).is_err());
}
