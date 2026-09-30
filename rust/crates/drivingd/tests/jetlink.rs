use openpilot_cereal::log_capnp::{event, jetlink_frame_status};
use openpilot_driving_modeld::{jetlink, publication::Messages};
use openpilot_jetlink::{
    runtime::Status,
    transition::{Decision, Phase, Source},
};
use std::io::Cursor;
fn status() -> Status {
    Status {
        decision: Decision {
            source: Source::Native,
            phase: Phase::Lost,
            loss_latched: true,
            reset_required: false,
        },
        generation: "ab".repeat(16),
        frame: 42,
        execution_ms: 51.0,
        validated: true,
    }
}
fn message(kind: usize) -> Vec<u8> {
    let mut msg = capnp::message::Builder::new_default();
    let mut root = msg.init_root::<event::Builder>();
    root.set_valid(true);
    match kind {
        0 => {
            root.init_model_v2();
        }
        1 => {
            root.init_driving_model_data();
        }
        _ => {
            root.init_camera_odometry();
        }
    }
    capnp::serialize::write_message_to_words(&msg)
}
#[test]
fn stale_fallback_invalidates_all_three_real_wire_messages() {
    // Given current model/driving/odometry packets and a latched external loss.
    let mut messages = Messages {
        model: message(0),
        driving: message(1),
        pose: message(2),
    };
    // When publication is past the complete external budget.
    jetlink::publication(&mut messages, &status(), false).unwrap();
    // Then every packet is invalid, with identical external status in both driving packets.
    for bytes in [&messages.model, &messages.driving, &messages.pose] {
        let message = capnp::serialize::read_message(
            Cursor::new(bytes),
            capnp::message::ReaderOptions::new(),
        )
        .unwrap();
        let root = message.get_root::<event::Reader>().unwrap();
        assert!(!root.get_valid());
        let status = match root.which().unwrap() {
            event::ModelV2(value) => Some(value.unwrap().get_jetlink().unwrap()),
            event::DrivingModelData(value) => Some(value.unwrap().get_jetlink().unwrap()),
            event::CameraOdometry(_) => None,
            _ => panic!("unexpected topic"),
        };
        if let Some(status) = status {
            assert_eq!(
                status.get_source().unwrap(),
                jetlink_frame_status::Source::Native
            );
            assert_eq!(status.get_phase().unwrap().to_str().unwrap(), "LOST");
            assert!(status.get_loss_latched());
            assert_eq!(
                status.get_generation().unwrap().to_str().unwrap(),
                "ab".repeat(16)
            );
            assert_eq!(status.get_frame_id(), 42);
        }
    }
}
#[test]
fn source_and_loss_persist_together_for_restart_safety() {
    // Given real isolated Params storage and no previous persisted runtime state.
    let directory = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(directory.path(), "fixture").unwrap();
    let mut stored = None;
    // When an external loss falls back to native.
    jetlink::persist(&params, &mut stored, &status()).unwrap();
    // Then a restarted runtime can recover the loss even though native is now selected.
    assert!(!params.get_bool("JetlinkActive").unwrap());
    assert!(params.get_bool("JetlinkLossLatched").unwrap());
    assert_eq!(stored, Some((false, true)));
}

#[test]
fn requested_external_raw_predictions_preserve_original_failure_boundary() {
    // Given an active external source and a native buffer that must not be relabeled.
    let native = [13, 42];
    // When raw output is requested, then the original missing raw_pred is a dedicated typed error.
    assert!(matches!(
        jetlink::raw_predictions(true, Source::Jetlink, &native),
        Err(openpilot_driving_modeld::Error::JetlinkRawPredictionsUnavailable)
    ));
}
#[test]
fn ordinary_external_output_does_not_require_raw_predictions() {
    // Given active external output with the original default raw setting disabled.
    let native = [13, 42];
    // When preparing publication, then the normal path succeeds without any native raw bytes.
    assert_eq!(
        jetlink::raw_predictions(false, Source::Jetlink, &native).unwrap(),
        None
    );
}
