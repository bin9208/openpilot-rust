use openpilot_card::brands::hyundai::stopping::{CanfdStopping, StopInput};
#[test]
fn final_source_first_stop_packet_zeros_both_requests_during_persistent_creep() {
    let mut controller = CanfdStopping::default();
    for _ in 0..1500 {
        let command = controller
            .update(StopInput {
                active: true,
                requested: true,
                speed: 0.3,
                held: false,
                accel: -0.5,
                previous_value: -0.5,
                jerk_u: 2.,
                jerk_l: 1.,
            })
            .unwrap();
        assert_eq!((command.stop_req, command.raw, command.value), (1, 0., 0.));
    }
}
