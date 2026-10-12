use openpilot_cluster::renderer::hud::format_time;

#[test]
fn replay_clock_keeps_python_ties_even_and_minute_carry() {
    for (input, expected) in [
        (-5.0, "0:00"),
        (0.5, "0:00"),
        (1.5, "0:02"),
        (58.5, "0:58"),
        (59.5, "1:00"),
        (60.5, "1:00"),
        (3599.5, "60:00"),
        (7200.0, "120:00"),
    ] {
        assert_eq!(format_time(input), expected, "{input}");
    }
}
