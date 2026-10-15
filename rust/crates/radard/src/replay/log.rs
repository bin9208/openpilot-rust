//! Full Cereal schema streaming; recorded leads are retained alongside recalculation.
use super::{Inversion, Replay};
use crate::{controller::Options, daemon};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::state::State;
use serde_json::json;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn run(
    path: &Path,
    options: Options,
    choice: Inversion,
    output: &mut impl Write,
    mut raw: Option<&mut super::raw::Raw>,
) -> Result<()> {
    let file = File::open(path)?;
    let mut reader: Box<dyn Read> = match path.extension().and_then(|v| v.to_str()) {
        Some("zst") => Box::new(zstd::stream::read::Decoder::new(file)?),
        Some("bz2") => Box::new(bzip2::read::BzDecoder::new(file)),
        _ => Box::new(io::BufReader::new(file)),
    };
    let mut state = State::new(&daemon::SERVICES, daemon::options())?;
    let mut replay = Replay::new(options, choice);
    let mut recorded = None;
    let mut flipped = false;
    let mut frames = 0;
    while let Some(message) = serialize::try_read_message(&mut reader, ReaderOptions::new())? {
        let value = message.get_root::<event::Reader<'_>>()?;
        let now = value.get_log_mono_time();
        let is_model = matches!(value.which()?, event::ModelV2(_));
        let mut builder = capnp::message::Builder::new_default();
        builder.set_root(value)?;
        let bytes = serialize::write_message_to_words(&builder);
        if let Some(raw) = raw.as_deref_mut() {
            match value.which()? {
                event::ModelV2(_) => raw.scene(&bytes, false)?,
                event::LivePose(_) => raw.scene(&bytes, true)?,
                _ => (),
            }
            if matches!(value.which()?, event::Can(_) | event::CarState(_)) {
                let (publications, errors) =
                    raw.feed(&bytes, now, matches!(value.which()?, event::Can(_)))?;
                for error in errors {
                    writeln!(
                        output,
                        "{}",
                        json!({"mono_time": now, "raw_input_error": error})
                    )?;
                }
                for publication in publications {
                    let message =
                        serialize::read_message(publication.as_slice(), ReaderOptions::new())?;
                    if let event::LiveTracks(value) =
                        message.get_root::<event::Reader<'_>>()?.which()?
                    {
                        flipped = value?.get_radar_track_flipped();
                    }
                    #[allow(clippy::cast_precision_loss)]
                    state.update(now as f64 * 1e-9, &[publication])?;
                }
            }
        }
        match value.which()? {
            event::RadarState(value) => {
                recorded = Some(
                    json!({"mono_time": now, "leads": openpilot_plannerd::radar_decode::radar(value?)?}),
                );
                continue;
            }
            event::LiveTracks(value) => {
                if raw.is_some() {
                    continue;
                }
                flipped = value?.get_radar_track_flipped();
            }
            event::ModelV2(_) | event::CarState(_) | event::LivePose(_) => (),
            _ => continue,
        }
        #[allow(clippy::cast_precision_loss)]
        state.update(now as f64 * 1e-9, &[bytes])?;
        if !is_model {
            continue;
        }
        // A segment may begin before the first car/track/pose message. Report it
        // explicitly rather than inventing an ego sample or erasing invalidity.
        let missing: Vec<_> = ["modelV2", "carState", "liveTracks"]
            .iter()
            .filter(|name| state.topic(name).is_ok_and(|topic| !topic.seen))
            .copied()
            .collect();
        if !missing.is_empty() {
            writeln!(
                output,
                "{}",
                json!({"mono_time": now, "skipped_missing": missing})
            )?;
            continue;
        }
        // Preserve display-only ARS objects independently of controller input.
        // Recorded flip has already been applied by radarcan; analysis applies
        // only the difference to the explicitly requested orientation.
        let display = match state.topic("liveTracks")?.event()?.which()? {
            event::LiveTracks(tracks) => {
                let tracks = tracks?;
                let mut points = crate::wire::display_points(tracks)?;
                super::apply_inversion(&mut points, flipped, choice);
                json!({"has_display_data":tracks.get_has_display_data(),"points":points})
            }
            _ => return Err("liveTracks event mismatch".into()),
        };
        let input = daemon::input(&state)?;
        let model_time_s = input.time_s;
        let radar_to_model_time_s = input.radar_to_model_time_s;
        let result = replay.update(input, flipped)?;
        writeln!(
            output,
            "{}",
            json!({"frame": frames, "mono_time": now, "input_valid": state.all_checks(&daemon::SERVICES)?, "model_time_s": model_time_s, "radar_to_model_time_s": radar_to_model_time_s, "recorded_flipped": flipped, "display": display, "recorded_leads": recorded, "output": result})
        )?;
        frames += 1;
    }
    // Empty or CAN-only logs cannot accidentally masquerade as a successful replay.
    if frames == 0 {
        return Err("no complete model/car/liveTracks frames in full-schema log".into());
    }
    Ok(())
}
