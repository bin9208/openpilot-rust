//! Native input generation for the unchanged paramsd Python fixture family.
//! The original controller remains the independent oracle for wire bytes.
use capnp::serialize::write_message_to_words;
use openpilot_cereal::{
    car_capnp::car_params,
    log_capnp::{event, live_calibration_data, live_pose},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const FIXTURE_SHA: &str = "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521";
const DAEMON_SHA: &str = "04a5433f356d594d3db8ce42394274e0bb1c856d20e975c5657285ce8f998b87";

fn pin(root: &Path, name: &str, hash: &str) -> Result<()> {
    let bytes = fs::read(root.join(name))?;
    if format!("{:x}", Sha256::digest(bytes)) != hash {
        return Err(format!("paramsd original source changed: {name}").into());
    }
    Ok(())
}

pub fn car_params(index: usize, fingerprint: Option<&str>) -> Vec<u8> {
    let (mass, inertia, wheelbase, front, tire_front, tire_rear, ratio) = [
        (1600., 2700., 2.7, 1.1, 80000., 90000., 15.),
        (2450., 4800., 3.05, 1.35, 125000., 140000., 18.7),
        (3100., 6200., 3.2, 1.4, 165000., 155000., 12.5),
    ][index];
    let mut message = capnp::message::Builder::new_default();
    let mut car = message.init_root::<car_params::Builder<'_>>();
    let synthetic = format!("SYNTHETIC_{index}");
    car.set_car_fingerprint(
        fingerprint
            .filter(|value| !value.is_empty())
            .unwrap_or(&synthetic),
    );
    car.set_mass(mass as f32);
    car.set_rotational_inertia(inertia as f32);
    car.set_wheelbase(wheelbase as f32);
    car.set_center_to_front(front as f32);
    car.set_tire_stiffness_front(tire_front as f32);
    car.set_tire_stiffness_rear(tire_rear as f32);
    car.set_steer_ratio(ratio as f32);
    write_message_to_words(&message)
}

fn timestamp(time: f64) -> u64 {
    (time * 1e9) as u64
}

fn measurement(
    mut out: live_pose::x_y_z_measurement::Builder<'_>,
    values: [f64; 3],
    stds: [f64; 3],
    valid: bool,
) {
    out.set_x(values[0] as f32);
    out.set_y(values[1] as f32);
    out.set_z(values[2] as f32);
    out.set_x_std(stds[0] as f32);
    out.set_y_std(stds[1] as f32);
    out.set_z_std(stds[2] as f32);
    out.set_valid(valid);
}

fn packet(time: f64, valid: bool, fill: impl FnOnce(event::Builder<'_>)) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(timestamp(time));
    event.set_valid(valid);
    fill(event);
    write_message_to_words(&message)
}

pub fn car_state(time: f64, speed: f64, steering: f64) -> Vec<u8> {
    packet(time, true, |mut event| {
        let mut state = event.reborrow().init_car_state();
        state.set_v_ego(speed as f32);
        state.set_steering_angle_deg(steering as f32);
    })
}

pub fn calibration(time: f64, rpy: [f64; 3]) -> Vec<u8> {
    packet(time, true, |mut event| {
        let mut calibration = event.reborrow().init_live_calibration();
        let mut values = calibration.reborrow().init_rpy_calib(3);
        for (index, value) in rpy.into_iter().enumerate() {
            values.set(index as u32, value as f32);
        }
        calibration.set_cal_status(live_calibration_data::Status::Calibrated);
    })
}

#[derive(Clone, Copy)]
pub struct Pose {
    pub yaw: f64,
    pub yaw_std: f64,
    pub yaw_valid: bool,
    pub roll: f64,
    pub roll_std: f64,
    pub sensors: bool,
    pub posenet: bool,
    pub valid: bool,
}
impl Default for Pose {
    fn default() -> Self {
        Self {
            yaw: 0.03,
            yaw_std: 0.03,
            yaw_valid: true,
            roll: 0.02,
            roll_std: 0.01,
            sensors: true,
            posenet: true,
            valid: true,
        }
    }
}

pub fn pose(time: f64, options: Pose) -> Vec<u8> {
    packet(time, options.valid, |mut event| {
        let mut pose = event.reborrow().init_live_pose();
        pose.set_timestamp(timestamp(time));
        pose.set_sensors_o_k(options.sensors);
        pose.set_posenet_o_k(options.posenet);
        measurement(
            pose.reborrow().init_orientation_n_e_d(),
            [options.roll, 0.03, -0.04],
            [options.roll_std, 0.01, 0.02],
            true,
        );
        measurement(
            pose.reborrow().init_velocity_device(),
            [12., 0.1, -0.2],
            [0.1, 0.2, 0.3],
            true,
        );
        measurement(
            pose.reborrow().init_acceleration_device(),
            [0.3, -0.2, 0.1],
            [0.2, 0.3, 0.4],
            true,
        );
        measurement(
            pose.init_angular_velocity_device(),
            [-0.01, 0.02, options.yaw],
            [0.01, 0.02, options.yaw_std],
            options.yaw_valid,
        );
    })
}

pub fn gps(service: &str, time: f64) -> Result<Vec<u8>> {
    if !matches!(service, "gpsLocation" | "gpsLocationExternal") {
        return Err("unexpected paramsd GPS service".into());
    }
    Ok(packet(time, false, |mut event| {
        let mut location = if service == "gpsLocation" {
            event.reborrow().init_gps_location()
        } else {
            event.reborrow().init_gps_location_external()
        };
        location.set_has_fix(true);
        location.set_latitude(37.5);
        location.set_longitude(127.);
        location.set_bearing_deg(22.5);
    }))
}

pub fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|value| value.to_bits()).collect()
}

pub fn state(values: Option<&[f64]>, covariance: Option<&[f64]>) -> Value {
    let default_values = [1., 15., 0., 0., 10., 0., 0., 0., 0.];
    let default_covariance = [0.001; 9];
    json!({
        "state": bits(values.unwrap_or(&default_values)),
        "covariance": bits(covariance.unwrap_or(&default_covariance)),
        "time": 500.0,
    })
}

fn operation(packet: Vec<u8>) -> Value {
    json!({"packet": packet})
}

pub fn scenarios() -> Value {
    let mut cases = Vec::new();
    for profile in 0..3 {
        let mut operations = vec![operation(calibration(99., [0.02, -0.03, 0.04]))];
        for i in 0..160 {
            let i = f64::from(i);
            let time = 100. + i * 0.05;
            operations.push(operation(car_state(
                time,
                18. + (i * 0.1).sin(),
                4. * (i * 0.07).sin(),
            )));
            operations.push(operation(pose(
                time + 0.001,
                Pose {
                    yaw: 0.03 * (i * 0.07).sin(),
                    roll: 0.015 * (i * 0.03).sin(),
                    ..Pose::default()
                },
            )));
        }
        cases.push(json!({
            "name": format!("physical-model-{profile}"),
            "car": car_params(profile, None),
            "operations": operations,
        }));
    }
    let mut boundaries = Vec::new();
    for (i, (speed, steering)) in [
        (0., 0.),
        (1., 0.),
        (1.01, 0.),
        (20., 44.99),
        (20., 45.),
        (20., -45.),
        (-2., 0.),
        (10., 0.),
        (10.01, 0.),
    ]
    .into_iter()
    .enumerate()
    {
        let time = 200. + i as f64 * 0.1;
        boundaries.push(operation(car_state(time, speed, steering)));
        boundaries.push(operation(pose(time + 0.01, Pose::default())));
    }
    let options = [
        Pose {
            yaw_valid: false,
            ..Pose::default()
        },
        Pose {
            yaw_std: 0.,
            ..Pose::default()
        },
        Pose {
            yaw_std: 10.,
            ..Pose::default()
        },
        Pose {
            yaw: 1.,
            ..Pose::default()
        },
        Pose {
            yaw: f64::NAN,
            ..Pose::default()
        },
        Pose {
            roll_std: f64::NAN,
            ..Pose::default()
        },
        Pose {
            roll_std: 1.5_f64.to_radians(),
            ..Pose::default()
        },
        Pose {
            roll: 10_f64.to_radians(),
            ..Pose::default()
        },
        Pose {
            sensors: false,
            ..Pose::default()
        },
        Pose {
            posenet: false,
            ..Pose::default()
        },
        Pose {
            yaw_std: -0.04,
            ..Pose::default()
        },
    ];
    for (i, options) in options.into_iter().enumerate() {
        boundaries.push(operation(pose(202. + i as f64 * 0.05, options)));
    }
    cases.push(json!({"name":"active-fallback-and-thresholds", "car":car_params(0, None), "operations":boundaries}));
    let mut operations = Vec::new();
    for (angle, count) in [
        (11.0_f64, 14),
        (9., 4),
        (7.5, 4),
        (-11., 24),
        (-9., 4),
        (-7.5, 4),
    ] {
        let radians = angle.to_radians();
        operations.push(state(
            Some(&[1., 15., radians, 0., 10., 0., 0., 0., radians]),
            None,
        ));
        operations.extend((0..count).map(|_| json!({})));
    }
    for (stiffness, ratio) in [
        (0.2 - 1e-10, 30. + 1e-8),
        (0.199, 30.01),
        (5. + 1e-8, 7.5 - 1e-8),
        (5.01, 7.49),
    ] {
        operations.push(state(
            Some(&[stiffness, ratio, 0., 0., 10., 0., 0., 0., 0.]),
            None,
        ));
    }
    cases.push(json!({"name":"clipping-hysteresis-and-float32-validity", "car":car_params(0, None), "operations":operations}));
    cases.push(json!({"name":"nonfinite-state-keeps-prereset-std", "car":car_params(0, None), "operations":[
        state(Some(&[1.,15.,f64::NAN,0.,10.,0.,0.,0.,0.]), Some(&[2.,3.,4.,5.,6.,7.,8.,9.,10.])),
        json!({}), state(None, Some(&[0.001,0.001,0.001,0.001,0.001,0.001,0.001,0.001,-1.])), json!({}),
    ]}));
    let mut rewind = vec![operation(car_state(600., 20., 2.))];
    for i in 0..130 {
        rewind.push(operation(pose(600.05 + i as f64 * 0.01, Pose::default())));
    }
    rewind.push(operation(pose(600.5, Pose::default())));
    rewind.push(operation(pose(598., Pose::default())));
    rewind.push(operation(car_state(603., 0., 0.)));
    rewind.push(operation(pose(700., Pose::default())));
    rewind.push(operation(car_state(700.1, 20., 1.)));
    cases.push(json!({"name":"rewind-eviction-and-inactive-clock-reset", "car":car_params(0, None), "operations":rewind}));
    json!({"cases": cases})
}

pub fn daemon_case(restart: bool, saved_packet: Option<&[u8]>) -> Result<Value> {
    let gps_service = if restart {
        "gpsLocation"
    } else {
        "gpsLocationExternal"
    };
    let count = if restart { 8 } else { 1202 };
    let mut frames = vec![json!({"time":99.95, "messages":[]})];
    for i in 0..count {
        let time = 100. + i as f64 * 0.05;
        frames.push(json!({"time":time,"messages":[
            car_state(time, 18., 2.),
            calibration(time, [0.01, 0.02, -0.01]),
            gps(gps_service, time)?,
            pose(time, Pose::default()),
        ]}));
    }
    let seed = if restart {
        let packet = saved_packet.ok_or("restart requires saved original packet")?;
        json!({"LiveParametersV2":packet, "CarParamsPrevRoute":car_params(0, None)})
    } else {
        json!({})
    };
    Ok(json!({
        "name":if restart {"restart"} else {"continuous"},
        "car":car_params(0, None), "seed":seed, "debug":true, "replay":true,
        "simulation":true, "gps":gps_service, "frames":frames,
    }))
}

pub fn generate(root: &Path, output: &Path, saved_packet: Option<&Path>) -> Result<()> {
    pin(root, "rust/tools/paramsd_fixture.py", FIXTURE_SHA)?;
    pin(root, "rust/tools/check_paramsd_daemon.py", DAEMON_SHA)?;
    let saved = saved_packet.map(fs::read).transpose()?;
    let scenarios = serde_json::to_vec(&scenarios())?;
    let continuous = serde_json::to_vec(&daemon_case(false, None)?)?;
    let restart = saved
        .as_deref()
        .map(|bytes| daemon_case(true, Some(bytes)))
        .transpose()?;
    fs::create_dir(output)?;
    fs::write(output.join("scenarios.json"), scenarios)?;
    fs::write(output.join("continuous-input.json"), continuous)?;
    if let Some(restart) = restart {
        fs::write(
            output.join("restart-input.json"),
            serde_json::to_vec(&restart)?,
        )?;
    }
    pin(root, "rust/tools/paramsd_fixture.py", FIXTURE_SHA)?;
    pin(root, "rust/tools/check_paramsd_daemon.py", DAEMON_SHA)?;
    Ok(())
}
