use openpilot_camerad::requests::Diagnostic;
use openpilot_logging::{native::Logger, record::Level, site::Site};
use std::sync::OnceLock;

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn initialize(device: &str) -> Result<(), openpilot_logging::Error> {
    let header = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../openpilot/common/version.h"
    ));
    let version = header
        .split('"')
        .nth(1)
        .ok_or(openpilot_logging::Error::Contract("missing COMMA_VERSION"))?;
    let logger = Logger::for_runtime(version, device)?;
    let _ = LOGGER.set(logger);
    openpilot_camera_kernel::set_diagnostic_handler(kernel);
    Ok(())
}

fn kernel(value: openpilot_camera_kernel::KernelDiagnostic) {
    use openpilot_camera_kernel::KernelDiagnostic;
    match value {
        KernelDiagnostic::Message { debug, text } => emit(
            openpilot_logging::log_site!(),
            if debug { Level::Debug } else { Level::Info },
            text,
        ),
        KernelDiagnostic::CameraControl { opcode, errno } => camera_log!(
            Error,
            "VIDIOC_CAM_CONTROL error: op_code {} - errno {errno}",
            opcode as i32
        ),
        KernelDiagnostic::SyncControl {
            id,
            errno,
            transport,
            kernel,
        } => camera_log!(
            Error,
            "CAM_SYNC error: id {id} - errno {errno} - ret {transport} - ioctl_result {kernel}"
        ),
    }
}

pub fn emit(site: Site, level: Level, message: String) {
    if let Some(logger) = LOGGER.get() {
        let _ = logger.emit(site, level, message);
    }
}

pub fn close() {
    if let Some(logger) = LOGGER.get() {
        let _ = logger.close();
    }
}

pub fn request(value: Diagnostic<'_>) {
    match value {
        Diagnostic::Stale { camera, event } => camera_log!(Debug,
            "skipping frame: ts before requeue / cam {camera} ts {} req id {} frame id {}",
            event.timestamp, event.request_id, event.frame_id),
        Diagnostic::Timing { camera, event, received, sample, previous_frame, previous_request, last_requeue } => camera_log!(Warning,
            "camera SOF timing: camera {camera} raw_id {} request {} sof_boot_ns {} received_ns {received} sof_delta_ms {:.3} event_age_ms {:.3} sof_status {} last_valid_raw {previous_frame} last_valid_request {previous_request} last_requeue_ns {last_requeue} suppressed {}",
            event.frame_id, event.request_id, event.timestamp, sample.sof_delta_ns as f64 * 1e-6, sample.event_age_ns as f64 * 1e-6, event.sof_status, sample.suppressed),
        Diagnostic::InvalidReset { camera } => camera_log!(Error, "camera {camera} reset after half second of invalid requests"),
        Diagnostic::Gap { camera, frame: true, previous, current } => camera_log!(Error, "camera {camera} frame ID skipped, {previous} -> {current}"),
        Diagnostic::Gap { camera, frame: false, previous, current } => camera_log!(Error, "camera {camera} requests skipped {} -> {}", previous as i64, current as i64),
        Diagnostic::Requeue { camera, from } => camera_log!(Warning, "clearing and requeuing camera {camera} from {from}"),
        Diagnostic::SyncFailure { camera, event } => camera_log!(Error, "camera {camera} sync failure {} {} ", event.request_id as i64, event.frame_id as i64),
        Diagnostic::WaitFailure { camera, point, elapsed_ms } => camera_log!(Error, "camera {camera} {} failed after {elapsed_ms:.2}ms", point.name()),
        Diagnostic::Synchronization { camera, event, sync } => {
            let transition = sync.transition();
            if transition.synchronized {
                for (id, data) in sync.cameras() {
                    camera_log!(Warning, "camera {id} synced on frame_id_offset {} timestamp {}", data.frame_id_offset as i64, data.timestamp);
                }
            }
            if transition.timed_out {
                camera_log!(Error, "camera first frame sync timed out: camera {camera} request {} raw_id {} timestamp {} cams {}/{}", event.request_id, event.frame_id, event.timestamp, sync.cameras().len(), sync.enabled_camera_count());
                for (id, data) in sync.cameras() {
                    camera_log!(Error, "camera {id} first frame sync data: frame_id_offset {} timestamp {} staggered {}", data.frame_id_offset, data.timestamp, u8::from(data.staggered));
                }
            }
        }
    }
}
