//! Independent policy translated from the retained feedback checker and source.
//! The candidate daemon is never used to construct expected events or decisions.
use crate::seal::Result;
use openpilot_cereal::log_capnp::event;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io, path::Path};

const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/feedbackd/manifest.json");
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
/// Every retained source remains mandatory; only the retired checker may be absent.
pub fn validate_source(root: &Path) -> Result<Value> {
    require(
        format!("{:x}", Sha256::digest(MANIFEST))
            == "f8c68fc0d8abe7c98ed194b923fb301a2b6639e92e4629d5592c39fbaf974c1a",
        "feedback original manifest changed",
    )?;
    let manifest: Value = serde_json::from_slice(MANIFEST)?;
    let pins = manifest["source_sha256"]
        .as_object()
        .ok_or("feedback source map missing")?;
    require(pins.len() == 36, "feedback source keyset changed")?;
    for (name, expected) in pins {
        match fs::read(root.join(name)) {
            Ok(raw) => require(
                Some(format!("{:x}", Sha256::digest(&raw)).as_str()) == expected.as_str(),
                format!("feedback original source changed: {name}"),
            )?,
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    && name == "rust/tools/check_feedbackd.py"
                    && matches!(fs::symlink_metadata(root.join(name)), Err(ref e) if e.kind() == io::ErrorKind::NotFound) =>
                {}
            Err(e) => return Err(format!("feedback original source absent: {name}: {e}").into()),
        }
    }
    Ok(manifest["source_sha256"].clone())
}
#[derive(Debug)]
pub struct IterationContract {
    pub index: usize,
    pub bookmarks: usize,
    pub car_valid: bool,
    pub bookmark_valid: Option<bool>,
    pub audio_feedback: usize,
    /// Byte count of the original raw audio payload (not a count of i16 samples).
    pub raw_audio_samples: usize,
    pub sample_rate: u32,
}
pub fn iteration_contract(index: usize) -> Result<IterationContract> {
    require(
        index < 8,
        "feedback iteration outside original eight-case loop",
    )?;
    let pressed = index.is_multiple_of(2);
    Ok(IterationContract {
        index,
        bookmarks: usize::from(pressed),
        car_valid: pressed,
        bookmark_valid: pressed.then_some(index.is_multiple_of(4)),
        audio_feedback: 0,
        raw_audio_samples: 1536,
        sample_rate: 16000,
    })
}
pub fn input_packet(topic: &str, valid: bool, now: u64) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    let mut e = message.init_root::<event::Builder>();
    e.set_valid(valid);
    e.set_log_mono_time(now);
    match topic {
        "rawAudioData" => {
            let mut audio = e.init_raw_audio_data();
            let bytes: Vec<u8> = (0..6).flat_map(|_| 0..=255u8).collect();
            audio.set_data(&bytes);
            audio.set_sample_rate(16000);
        }
        "carState" => {
            let mut car = e.init_car_state();
            car.set_can_valid(true);
            let mut buttons = car.init_button_events(1);
            let mut button = buttons.reborrow().get(0);
            button.set_type(openpilot_cereal::car_capnp::car_state::button_event::Type::Lkas);
            button.set_pressed(valid);
        }
        "bookmarkButton" => {
            e.init_bookmark_button();
        }
        _ => return Err(format!("unsupported feedback input: {topic}").into()),
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub fn validate_bookmark(raw: &[u8], started: u64, received: u64) -> Result<()> {
    let message = capnp::serialize::read_message(
        &mut io::Cursor::new(raw),
        capnp::message::ReaderOptions::new(),
    )?;
    let e = message.get_root::<event::Reader>()?;
    require(
        matches!(e.which()?, event::UserBookmark(_)) && e.get_valid(),
        "feedback bookmark union or validity mismatch",
    )?;
    require(
        started <= e.get_log_mono_time() && e.get_log_mono_time() <= received,
        "feedback bookmark time outside send/receive window",
    )
}
