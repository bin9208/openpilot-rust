use capnp::{message::Builder, serialize};

fn isolated(name: &str) -> bool {
    if std::env::var_os("CLUSTER_LIVE_TEST_CHILD").is_some() {
        return true;
    }
    let directory = tempfile::Builder::new()
        .prefix("msgq_rust-probe-cluster-live-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = directory
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("CLUSTER_LIVE_TEST_CHILD", "1")
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .status()
        .unwrap();
    assert!(status.success());
    false
}
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::live::Input;

#[test]
fn external_route_projects_current_carrot_position_only_in_debug_mode() {
    if !isolated("external_route_projects_current_carrot_position_only_in_debug_mode") {
        return;
    }
    let mut input = Input::isolated(false, 0, 10.0).unwrap();
    let mut route = Builder::new_default();
    {
        let event = route.init_root::<event::Builder<'_>>();
        let mut coordinates = event.init_nav_route().init_coordinates(4);
        for (index, latitude) in [37.0, 37.0001, 37.0002, 37.0003].into_iter().enumerate() {
            let mut point = coordinates.reborrow().get(u32::try_from(index).unwrap());
            point.set_latitude(latitude);
            point.set_longitude(127.0);
        }
    }
    let mut carrot = Builder::new_default();
    {
        let event = carrot.init_root::<event::Builder<'_>>();
        let mut data = event.init_carrot_man();
        data.set_x_pos_lat(37.0);
        data.set_x_pos_lon(127.0);
        data.set_x_pos_angle(0.0);
    }
    input
        .subscriber
        .state
        .update(
            10.0,
            &[
                serialize::write_message_to_words(&route),
                serialize::write_message_to_words(&carrot),
            ],
        )
        .unwrap();
    let original = input.project_at(10.0).unwrap();
    input.set_debug_panels_enabled(false, false, true);
    let state = input.project_at(10.1).unwrap();
    assert_eq!(state.model_path.len(), 4);
    assert!((state.model_path[1].forward_m - 11.022_440_592_447_916).abs() < 1e-8);
    assert_eq!(state.model_path[1].lateral_m.to_bits(), 0.0_f64.to_bits());
    input.set_debug_panels_enabled(false, false, false);
    assert_eq!(
        input.project_at(10.2).unwrap().model_path,
        original.model_path
    );
    println!(
        "external route native state: {}",
        serde_json::to_string(&state).unwrap()
    );
}

#[test]
fn params_navigation_image_and_held_light_keep_source_expiry() {
    if !isolated("params_navigation_image_and_held_light_keep_source_expiry") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(directory.path(), "cluster").unwrap();
    params.put("CarrotNaviDebug", r#"{"title":"Signal","lines":["turn right"],"speedLimitKph":"５０.9","receivedMono":10.0,"trafficLight":{"redS":9,"redOn":true}}"#.as_bytes()).unwrap();
    params
        .put(
            "CarrotNaviImage",
            br#"{"imageBase64":"YWJj","imageHash":"frame","imageWidth":24,"imageHeight":16}"#,
        )
        .unwrap();
    let mut input = Input::isolated(false, 0, 10.0).unwrap();
    input.attach_params(params.clone(), None);
    input.set_debug_panels_enabled(false, false, true);
    let state = input.project_at(10.0).unwrap();
    let info = state.navi_debug.unwrap();
    assert_eq!(info.title, "Signal");
    assert_eq!(info.speed_limit_kph, Some(50));
    assert_eq!(info.traffic_light.unwrap().red_s, Some(9));
    assert_eq!(info.guidance_image.unwrap().image_hash, "frame");
    assert_eq!(state.speed_limit_source.as_deref(), Some("n"));
    params.remove("CarrotNaviDebug").unwrap();
    let state = input.project_at(12.0).unwrap();
    assert_eq!(
        state.navi_debug.unwrap().traffic_light.unwrap().red_s,
        Some(7)
    );
    let state = input.project_at(20.001).unwrap();
    assert!(state.navi_debug.unwrap().traffic_light.is_none());
    println!(
        "Params navigation source expiry: {}",
        serde_json::to_string(&input.last_state).unwrap()
    );
}

fn device(started: bool, brightness: i8) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(true);
    let mut data = event.init_device_state();
    data.set_started(started);
    data.set_screen_brightness_percent(brightness);
    serialize::write_message_to_words(&message)
}
fn wide(exposure: f32, valid: bool) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(valid);
    event
        .init_wide_road_camera_state()
        .set_exposure_val_percent(exposure);
    serialize::write_message_to_words(&message)
}
fn selfdrive() -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(true);
    event.init_selfdrive_state().set_enabled(true);
    serialize::write_message_to_words(&message)
}

#[test]
fn owned_cereal_messages_produce_the_shared_cluster_state() {
    if !isolated("owned_cereal_messages_produce_the_shared_cluster_state") {
        return;
    }
    let mut publisher = openpilot_messaging::runtime::PubMaster::isolated(&[
        "carState",
        "deviceState",
        "selfdriveState",
    ])
    .unwrap();
    let mut input = Input::isolated(false, 50, 10.0).unwrap();
    let mut message = Builder::new_default();
    {
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_valid(true);
        let mut car = event.init_car_state();
        car.set_v_ego(20.0);
        car.set_v_ego_cluster(21.0);
        car.set_a_ego(1.5);
        car.set_v_cruise_cluster(100.5);
        car.reborrow().init_cruise_state().set_available(true);
    }
    publisher
        .send("carState", &serialize::write_message_to_words(&message))
        .unwrap();
    publisher.send("deviceState", &device(true, 72)).unwrap();
    publisher.send("selfdriveState", &selfdrive()).unwrap();
    input.poll().unwrap();
    let state = input.project_at(10.0).unwrap();
    assert_eq!(state.speed_kph.to_bits(), 72.0_f64.to_bits());
    assert_eq!(state.display_speed_kph, Some(75.600_000_000_000_01));
    assert_eq!(state.cruise_kph, Some(100));
    assert_eq!(
        state.cruise_display_state,
        openpilot_cluster::models::CruiseDisplayState::Engaged
    );
    assert!(state.onroad);
    assert_eq!(input.frames, 1);
    assert!(state.alert.is_none());
    println!(
        "owned IPC projected state: {}",
        serde_json::to_string(&state).unwrap()
    );
}

#[test]
fn camera_brightness_precedes_offroad_device_brightness_and_keeps_validity() {
    if !isolated("camera_brightness_precedes_offroad_device_brightness_and_keeps_validity") {
        return;
    }
    let mut input = Input::isolated(false, 0, 10.0).unwrap();
    assert_eq!(input.onroad_state().unwrap(), None);
    assert_eq!(input.brightness().unwrap(), None);
    input
        .subscriber
        .state
        .update(10.0, &[device(false, 72), wide(92.0, true)])
        .unwrap();
    assert_eq!(input.onroad_state().unwrap(), Some(false));
    assert_eq!(input.brightness().unwrap(), Some(31));
    input
        .subscriber
        .state
        .update(10.1, &[wide(25.0, false)])
        .unwrap();
    assert_eq!(input.brightness().unwrap(), Some(0));
    input
        .subscriber
        .state
        .update(10.2, &[device(true, 72), wide(f32::NAN, true)])
        .unwrap();
    assert_eq!(input.brightness().unwrap(), Some(72));
}

#[test]
fn original_startup_timeout_reboot_boundaries_and_onroad_reset() {
    if !isolated("original_startup_timeout_reboot_boundaries_and_onroad_reset") {
        return;
    }
    let mut input = Input::isolated(false, 0, 10.0).unwrap();
    input
        .subscriber
        .state
        .update(10.0, &[device(true, 40)])
        .unwrap();
    assert_eq!(input.alert(None, true, 10.0).unwrap(), None);
    assert_eq!(input.alert(None, true, 15.0).unwrap(), None);
    assert_eq!(
        input.alert(None, true, 15.001).unwrap().unwrap().alert_type,
        "clusterSelfdriveStartup"
    );
    input.subscriber.state.update(16.0, &[selfdrive()]).unwrap();
    assert_eq!(input.alert(None, true, 16.0).unwrap(), None);
    input.subscriber.state.update(16.1, &[]).unwrap();
    assert_eq!(input.alert(None, true, 21.0).unwrap(), None);
    assert_eq!(
        input.alert(None, true, 21.001).unwrap().unwrap().alert_type,
        "clusterSelfdriveTimeout"
    );
    assert_eq!(
        input.alert(None, true, 31.0).unwrap().unwrap().alert_type,
        "clusterSelfdriveReboot"
    );
    assert_eq!(input.alert(None, false, 32.0).unwrap(), None);
    assert_eq!(input.alert(None, true, 40.0).unwrap(), None);
    assert_eq!(
        input.alert(None, true, 45.001).unwrap().unwrap().alert_type,
        "clusterSelfdriveStartup"
    );
}

#[test]
fn owned_cereal_ipc_updates_original_validity_and_runtime_status() {
    if !isolated("owned_cereal_ipc_updates_original_validity_and_runtime_status") {
        return;
    }
    let mut publisher =
        openpilot_messaging::runtime::PubMaster::isolated(&["deviceState", "wideRoadCameraState"])
            .unwrap();
    let mut input =
        Input::isolated(true, 50, openpilot_cluster::live::monotonic().unwrap()).unwrap();
    publisher.send("deviceState", &device(true, 72)).unwrap();
    publisher
        .send("wideRoadCameraState", &wide(25.0, false))
        .unwrap();
    input.poll().unwrap();
    assert_eq!(input.onroad_state().unwrap(), Some(true));
    assert_eq!(input.brightness().unwrap(), Some(72));
    let wide = input.subscriber.state.topic("wideRoadCameraState").unwrap();
    assert!(wide.updated && wide.alive && !wide.valid);
    let status = input
        .status_text(openpilot_cluster::live::monotonic().unwrap(), 0, 0)
        .unwrap();
    assert!(status.starts_with("live can/sendcan alive="), "{status}");
    assert!(
        status.ends_with("/28 upd=2 state=0.0Hz radar=0 detected=0"),
        "{status}"
    );
    println!("owned Cereal IPC observable: {status}; invalid wide retained, brightness=72");
}
