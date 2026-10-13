use openpilot_cluster::{
    runtime::output::{Geometry, Output},
    runtime_args::{Output as Mode, RuntimeArgs},
    runtime_preferences::Reader,
};
#[test]
fn window_output_owns_the_initial_setup_and_closes_repeatedly() {
    let args = RuntimeArgs {
        output: Mode::Window,
        width: Some(80),
        height: Some(24),
        ..Default::default()
    };
    let preferences = Reader::new(None).snapshot();
    let rate = args.output_rate(false, false).unwrap();
    let (mut output, dimensions) =
        Output::open_with(&args, &preferences, rate, |geometry: Geometry| {
            Ok(geometry.frame)
        })
        .unwrap();
    assert_eq!(dimensions, [80, 24]);
    assert_eq!(output.encoder_pid(), None);
    assert!(!output.native());
    output.check_error().unwrap();
    output.close().unwrap();
    output.close().unwrap();
}
