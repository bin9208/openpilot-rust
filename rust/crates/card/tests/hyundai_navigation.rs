use num_traits::ToPrimitive;
use openpilot_card::brands::hyundai::{
    navigation::{Input, Navigation, Output},
    wire::Values,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Case {
    wrapped: bool,
    mode: i32,
    school: bool,
    ticks: Vec<Tick>,
}
#[derive(Deserialize)]
struct Tick {
    input: OwnedInput,
    speed: f64,
    initial_limit: f64,
    warning: bool,
    expected: Output,
    total_distance: f64,
    camera_target: Option<f64>,
    status_target: Option<f64>,
}
#[derive(Deserialize)]
struct OwnedInput {
    hda: Option<Values>,
    position: Option<Values>,
    segment: Option<Values>,
    profile: Option<Values>,
    status: Option<Values>,
    profile_timestamp: u64,
    position_timestamp: u64,
    segment_timestamp: u64,
    hda_timestamp: u64,
    status_timestamp: u64,
    last_update: u64,
    pt_timeout: bool,
    alt_timeout: bool,
    hda_size: usize,
    status_size: usize,
    metric: bool,
}
impl OwnedInput {
    fn reader(&self) -> Input<'_> {
        Input {
            hda: self.hda.as_ref(),
            position: self.position.as_ref(),
            segment: self.segment.as_ref(),
            profile: self.profile.as_ref(),
            status: self.status.as_ref(),
            profile_timestamp: self.profile_timestamp,
            position_timestamp: self.position_timestamp,
            segment_timestamp: self.segment_timestamp,
            hda_timestamp: self.hda_timestamp,
            status_timestamp: self.status_timestamp,
            last_update: self.last_update,
            pt_timeout: self.pt_timeout,
            alt_timeout: self.alt_timeout,
            hda_size: self.hda_size,
            status_size: self.status_size,
            metric: self.metric,
        }
    }
}

#[test]
fn navigation_state_when_route_changes_camera_passage_section_and_stale_messages() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(root.join("navigation.json")).unwrap()).unwrap();
    let settings = openpilot_params::Params::open(
        &root.join(format!("navigation-settings-{}", std::process::id())),
        "p",
    )
    .unwrap();
    let mut outputs = Vec::new();
    for (case_index, case) in cases.iter().enumerate() {
        settings
            .put("VehicleNaviCanControl", case.mode.to_string().as_bytes())
            .unwrap();
        settings
            .put("VehicleSpeedCameraDistanceTime", b"40")
            .unwrap();
        settings
            .put_bool("VehicleNaviSchoolZoneControl", case.school)
            .unwrap();
        let mut nav = Navigation::new(case.wrapped, &settings).unwrap();
        let mut trace = Vec::new();
        for (tick_index, tick) in case.ticks.iter().enumerate() {
            let input = tick.input.reader();
            let changed = nav.refresh(&settings).unwrap();
            let mut output = Output {
                speed_limit: tick.initial_limit,
                ..Output::default()
            };
            let camera = if case.wrapped {
                nav.pv5_camera_warning(&input).unwrap()
            } else {
                tick.warning
            };
            let camera = nav.update_events(&input, &mut output, camera).unwrap() || camera;
            nav.speed_limit(&mut output, &input, camera, tick.speed, changed);
            for value in [
                &mut output.speed_limit,
                &mut output.speed_limit_distance,
                &mut output.bump_distance,
                &mut output.speed,
            ] {
                *value = f64::from(value.to_f32().unwrap());
            }
            assert_eq!(
                serde_json::to_value(&output).unwrap(),
                serde_json::to_value(&tick.expected).unwrap(),
                "case={case_index} tick={tick_index}"
            );
            assert_eq!(
                (
                    nav.total_distance,
                    nav.camera_target,
                    nav.camera_status_target
                ),
                (tick.total_distance, tick.camera_target, tick.status_target),
                "state case={case_index} tick={tick_index}"
            );
            trace.push(output);
        }
        outputs.push(trace);
    }
    std::fs::write(
        root.join("native-navigation.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}
