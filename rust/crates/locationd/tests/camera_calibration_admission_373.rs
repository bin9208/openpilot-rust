//! #373 follow-up: original camera/calibration policy precedes shape admission.
//! Original 34-control capture SHA256:
//! 1fb7962961354bf1e47c0d5d849ed97ccf7c06c4ecde1759345c43e7ae192e22.
//! All original controls made zero numerical observations; numerical parity is separate.
//! The sensor-only package and real IPC gate passed before this follow-up.
#![cfg(feature = "solver")]

use openpilot_cereal::log_capnp::event;
use openpilot_locationd::{estimator::Estimator, wire};
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn check(
    name: &str,
    packet: &[u8],
    expected: Option<i32>,
    expected_logs: &[(String, String)],
) -> TestResult {
    let mut estimator = Estimator::new(true)?;
    estimator.kf.reset_default(Some(100.))?;
    let before = estimator.kf.snapshot()?;
    let actual = wire::decode(packet)
        .and_then(|event| estimator.handle(event.log_time_ns as f64 * 1e-9, event.input));
    match expected {
        Some(value) => assert_eq!(actual? as i32, value, "{name}"),
        None => assert!(
            actual.is_err(),
            "{name}: malformed admitted shape must remain an error"
        ),
    }
    assert_eq!(
        estimator.logs, expected_logs,
        "{name}: diagnostic order/content"
    );
    let after = estimator.kf.snapshot()?;
    assert_eq!(before.time, after.time, "{name}: no observation expected");
    assert_eq!(before.x, after.x, "{name}: no observation expected");
    assert_eq!(
        before.covariance, after.covariance,
        "{name}: no observation expected"
    );
    Ok(())
}

fn camera(
    time: u64,
    rotation: &[f32],
    translation: &[f32],
    rotation_std: &[f32],
    translation_std: &[f32],
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(true);
    root.set_log_mono_time(time);
    let mut camera = root.init_camera_odometry();
    camera.set_timestamp_eof(time);
    fn fill(mut output: capnp::primitive_list::Builder<'_, f32>, values: &[f32]) {
        for (index, value) in values.iter().enumerate() {
            output.set(index as u32, *value);
        }
    }
    fill(camera.reborrow().init_rot(rotation.len() as u32), rotation);
    fill(
        camera.reborrow().init_trans(translation.len() as u32),
        translation,
    );
    fill(
        camera.reborrow().init_rot_std(rotation_std.len() as u32),
        rotation_std,
    );
    fill(
        camera
            .reborrow()
            .init_trans_std(translation_std.len() as u32),
        translation_std,
    );
    capnp::serialize::write_message_to_words(&message)
}

fn calibration(values: &[f32]) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(true);
    root.set_log_mono_time(100_000_000_000);
    let mut output = root
        .init_live_calibration()
        .init_rpy_calib(values.len() as u32);
    for (index, value) in values.iter().enumerate() {
        output.set(index as u32, *value);
    }
    capnp::serialize::write_message_to_words(&message)
}

#[test]
fn immutable_original_packets_preserve_all_34_admission_outcomes() -> TestResult {
    let original: Value = serde_json::from_slice(include_bytes!(
        "data/admission-373/camera-calibration-original.json"
    ))?;
    assert_eq!(
        original["source_sha256"],
        "aa29e3a4da21007bd93a99713f87c07b79407a52e067a1fdf82019c967103f5e"
    );
    let rows = original["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 34);
    for row in rows {
        assert_eq!(row["observation_calls"], serde_json::json!([]));
        let packet: Vec<u8> = serde_json::from_value(row["packet"].clone())?;
        let logs: Vec<(String, String)> = serde_json::from_value(row["logs"].clone())?;
        let expected = if row["outcome"]["returned"].as_bool().unwrap() {
            Some(row["outcome"]["result"].as_i64().unwrap() as i32)
        } else {
            assert_eq!(row["outcome"]["exception"], "ValueError");
            None
        };
        check(row["name"].as_str().unwrap(), &packet, expected, &logs)?;
    }
    Ok(())
}

#[test]
fn four_previously_captured_camera_and_calibration_rejections_preserve_exact_outcomes() -> TestResult
{
    let original: Value =
        serde_json::from_slice(include_bytes!("data/admission-373/original.json"))?;
    assert_eq!(
        original["source_sha256"],
        "aa29e3a4da21007bd93a99713f87c07b79407a52e067a1fdf82019c967103f5e"
    );
    let rows = original["other_early_rejections"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    for row in rows {
        assert_eq!(row["observation_calls"], serde_json::json!([]));
        let packet: Vec<u8> = serde_json::from_value(row["packet"].clone())?;
        let logs: Vec<(String, String)> = serde_json::from_value(row["logs"].clone())?;
        check(
            row["name"].as_str().unwrap(),
            &packet,
            Some(row["outcome"]["result"].as_i64().unwrap() as i32),
            &logs,
        )?;
    }
    Ok(())
}

#[test]
fn original_camera_timestamp_and_norm_rejections_precede_later_vector_access() -> TestResult {
    let mut checked = 0;
    for size in 0..=4 {
        let values = vec![0.; size];
        let std = vec![0.1; size];
        check(
            &format!("camera-stale-shape-{size}"),
            &camera(98_000_000_000, &values, &values, &std, &std),
            Some(1),
            &[(
                "warning".into(),
                "Observation timestamp is older than the max rewind threshold of the filter".into(),
            )],
        )?;
        check(
            &format!("camera-rotation-rejected-before-std-{size}"),
            &camera(100_000_000_000, &[20., 0., 0.], &[0.; 3], &std, &std),
            Some(2),
            &[],
        )?;
        checked += 2;
    }
    for size in [0, 1, 2, 4] {
        let values = vec![0.; size];
        let std = vec![0.1; size];
        check(
            &format!("camera-admitted-malformed-shape-{size}"),
            &camera(100_000_000_000, &values, &values, &std, &std),
            None,
            &[],
        )?;
        checked += 1;
    }
    for (name, rotation, translation, expected) in [
        ("rotation-at-limit", [10., 0., 0.], [0.; 3], None),
        ("rotation-above-limit", [10.0001, 0., 0.], [0.; 3], Some(2)),
        ("translation-at-limit", [0.; 3], [200., 0., 0.], None),
        (
            "translation-above-limit",
            [0.; 3],
            [200.0001, 0., 0.],
            Some(2),
        ),
    ] {
        check(
            name,
            &camera(100_000_000_000, &rotation, &translation, &[], &[]),
            expected,
            &[],
        )?;
        checked += 1;
    }
    assert_eq!(checked, 18);
    Ok(())
}

#[test]
fn original_calibration_bounds_cover_every_component_before_euler_shape_validation() -> TestResult {
    let mut checked = 0;
    for size in [1, 2, 4, 5] {
        for (position, replacement, expected) in [
            (None, 0., None),
            (Some(0), -0.6, Some(2)),
            (Some(size - 1), 0.6, Some(2)),
        ] {
            let mut values = vec![0.; size];
            if let Some(position) = position {
                values[position] = replacement;
            }
            check(
                &format!("calibration-{size}-{position:?}"),
                &calibration(&values),
                expected,
                &[],
            )?;
            checked += 1;
        }
    }
    for (name, values, expected) in [
        ("empty", &[][..], 0),
        ("three-zero", &[0., 0., 0.][..], 0),
        ("three-at-limits", &[-0.5, 0.5, 0.][..], 0),
        ("three-above-limit", &[0., 0., 0.5001][..], 2),
    ] {
        check(name, &calibration(values), Some(expected), &[])?;
        checked += 1;
    }
    assert_eq!(checked, 16);
    Ok(())
}

#[test]
fn original_nan_bounds_dual_shapes_and_variable_std_admission_are_preserved() -> TestResult {
    let original: Value = serde_json::from_slice(include_bytes!(
        "data/admission-373/camera-calibration-edges-original.json"
    ))?;
    assert_eq!(
        original["source_sha256"],
        "aa29e3a4da21007bd93a99713f87c07b79407a52e067a1fdf82019c967103f5e"
    );
    let rows = original["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 29);
    for row in rows {
        assert_eq!(row["observation_calls"], serde_json::json!([]));
        let packet: Vec<u8> = serde_json::from_value(row["packet"].clone())?;
        let logs: Vec<(String, String)> = serde_json::from_value(row["logs"].clone())?;
        let expected = if row["outcome"]["returned"].as_bool().unwrap() {
            Some(row["outcome"]["result"].as_i64().unwrap() as i32)
        } else {
            assert_eq!(row["outcome"]["exception"], "ValueError");
            None
        };
        check(row["name"].as_str().unwrap(), &packet, expected, &logs)?;
    }
    Ok(())
}
