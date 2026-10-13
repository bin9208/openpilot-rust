use super::Input;
use crate::{runtime::HelperConfig, runtime_args::RuntimeArgs};
use std::path::Path;

#[test]
fn random_runtime_advances_native_simulation_and_formats_source_status() {
    let helpers = HelperConfig {
        program: "unused".into(),
        launcher: "unused".into(),
        cwd: ".".into(),
    };
    let mut input = Input::new(Path::new("."), &RuntimeArgs::default(), 100.0, &helpers).unwrap();
    let first = input.sample(100.1, 0.1, 0).unwrap();
    assert!(first.state.speed_kph > 0.0);
    assert!(first
        .status
        .unwrap()
        .starts_with("random R2=0.29 L2=0.00 LSX=+0.02"));
    let next = input.sample(100.2, 0.1, 0).unwrap();
    assert!(next.state.speed_kph > first.state.speed_kph);
    assert_eq!(input.onroad().unwrap(), None);
    assert_eq!(input.brightness().unwrap(), None);
    input.close().unwrap();
}
