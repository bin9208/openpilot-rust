use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::media::MediaInput;
use serde::Deserialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Step {
    packet: PathBuf,
    #[serde(default)]
    reset: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, path] = args.as_slice() else {
        return Err("cluster_media_decode_fixture CASE".into());
    };
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(path)?)?;
    let capacity = openpilot_messaging::services::lookup("carrotNaviMedia")
        .unwrap()
        .queue_size;
    let mut publisher = openpilot_msgq::Publisher::with_capacity("carrotNaviMedia", capacity)?;
    let mut input = MediaInput::isolated()?;
    let mut frames = Vec::new();
    let mut session = 0_u64;
    for (index, step) in steps.into_iter().enumerate() {
        let sequence = u64::try_from(index)? + 1;
        if step.reset {
            session += 1;
        }
        let bytes = std::fs::read(step.packet)?;
        let mut message = Builder::new_default();
        {
            let event = message.init_root::<event::Builder<'_>>();
            let mut media = event.init_carrot_navi_media();
            media.set_schema_version(1);
            media.set_session_id(format!("session-{session}"));
            media.set_kind("render");
            media.set_name("map_main");
            media.set_present(true);
            media.set_message_type(3);
            media.set_sequence(sequence);
            media.set_flags(u16::from(step.reset));
            media.set_width(96);
            media.set_height(64);
            media.set_payload(&bytes);
        }
        publisher.send(&serialize::write_message_to_words(&message))?;
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            let dashboard = input.update_at(None, 10.0 + f64::from(u32::try_from(index)?) * 0.1)?;
            let frame = dashboard
                .media
                .into_iter()
                .find(|frame| frame.key == "render:map_main" && frame.sequence == sequence);
            if frame.is_some() || Instant::now() >= deadline {
                frames.push(frame);
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let dropped = input.dropped_decode_requests();
    assert!(input.close());
    eprintln!(
        "owned media decoder processed {} IPC requests; dropped={dropped}; worker closed",
        frames.len()
    );
    println!("{}", serde_json::to_string(&frames)?);
    Ok(())
}
