use super::{config::Mode, BASE_TOPICS};
use openpilot_messaging::state::Options;
pub(super) fn specification<'a>(
    cameras: &'a [String],
    gps: &'a str,
    mode: &Mode,
) -> (Vec<&'a str>, Options) {
    let mut names = BASE_TOPICS.to_vec();
    names.extend(cameras.iter().map(String::as_str));
    names.extend(["accelerometer", "gyroscope", gps]);
    let mut ignore = vec![
        "accelerometer".into(),
        "gyroscope".into(),
        gps.to_owned(),
        "alertDebug".into(),
    ];
    if mode.simulation {
        ignore.extend(["driverCameraState".into(), "managerState".into()]);
    }
    ignore.push("driverMonitoringState".into());
    if mode.replay {
        ignore.push("roadCameraState".into());
        if cameras.iter().any(|name| name == "wideRoadCameraState") {
            ignore.push("wideRoadCameraState".into());
        }
    }
    (
        names,
        Options {
            frequency: Some(100.0),
            ignore_alive: ignore.clone(),
            ignore_frequency: ignore.clone(),
            ignore_valid: ignore,
            ..Options::default()
        },
    )
}
