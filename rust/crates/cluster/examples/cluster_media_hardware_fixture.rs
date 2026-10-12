use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::{media::MediaInput, models::NaviMediaFrame};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn access_unit(bytes: &[u8], sequence: u64) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut media = message
        .init_root::<event::Builder<'_>>()
        .init_carrot_navi_media();
    media.set_schema_version(1);
    media.set_session_id("owned-hardware");
    media.set_kind("render");
    media.set_name("map_main");
    media.set_present(true);
    media.set_message_type(3);
    media.set_sequence(sequence);
    media.set_flags(1);
    media.set_width(96);
    media.set_height(64);
    media.set_payload(bytes);
    serialize::write_message_to_words(&message)
}
fn wait(
    input: &mut MediaInput,
    sequence: u64,
) -> Result<NaviMediaFrame, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = input.update_at(None, 10.0)?;
        if let Some(frame) = state
            .media
            .into_iter()
            .find(|frame| frame.sequence == sequence)
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err("native media worker returned no requested frame".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, packet] = args.as_slice() else {
        return Err("cluster_media_hardware_fixture PACKET".into());
    };
    let bytes = std::fs::read(packet)?;
    let capacity = openpilot_messaging::services::lookup("carrotNaviMedia")
        .ok_or("media service absent")?
        .queue_size;
    let mut publisher = openpilot_msgq::Publisher::with_capacity("carrotNaviMedia", capacity)?;
    let mut input = MediaInput::isolated()?;
    publisher.send(&access_unit(&bytes, 1))?;
    let hardware = wait(&mut input, 1)?;
    let frame = hardware
        .hardware_buffer
        .as_ref()
        .ok_or("hardware-first media owner missing")?;
    assert_eq!(hardware.mime, "video/nv12-dmabuf");
    assert_eq!(
        (
            frame.layout.width,
            frame.layout.height,
            frame.layout.stride,
            frame.layout.uv_offset
        ),
        (96, 64, 128, 8192)
    );
    assert_eq!(frame.sequence(), 1);
    let retained = Arc::clone(frame);
    drop(hardware);
    retained.mark_egl_import_failed("controlled original-contract EGL import failure");
    publisher.send(&access_unit(&bytes, 2))?;
    let software = wait(&mut input, 2)?;
    assert!(software.hardware_buffer.is_none());
    assert_eq!(software.mime, "image/yuv420p");
    assert!(software.plane_data.is_some());
    assert_eq!((software.width, software.height), (96, 64));
    let closed = input.close();
    assert!(closed);
    let lease_after_close = rustix::fs::fstat(retained.as_ref()).is_ok();
    assert!(lease_after_close);
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "hardware_layout": [retained.layout.width, retained.layout.height, retained.layout.stride, retained.layout.uv_offset],
            "hardware_token": retained.token(),
            "hardware_sequence": retained.sequence(),
            "lease_after_close": lease_after_close,
            "worker_closed": closed,
            "software": software,
        }))?
    );
    drop(retained);
    Ok(())
}
