//! Missing/disabled recordings, excluded services, shutdown and malformed input.
use super::{
    fixtures::{self, Fixtures},
    peer::{Peer, Settings},
    require, save,
    scenarios::ENCODERS,
    Result,
};
use serde_json::{json, Value};
use std::{fs, time::Duration};
pub(super) fn settings(name: &str) -> Settings {
    match name {
        "disabled" => {
            let mut settings = Settings::new(&ENCODERS);
            settings
                .parameters
                .insert("RecordRoadCam".into(), "0".into());
            settings.parameters.insert("RecordFront".into(), "0".into());
            settings
        }
        "selection" => Settings::new(&[
            "livestreamRoadEncodeData",
            "livestreamWideRoadEncodeData",
            "livestreamDriverEncodeData",
            "youtubeRoadEncodeData",
            "rawAudioData",
        ]),
        "missing-audio" => {
            let mut settings = Settings::new(&["qRoadEncodeData"]);
            settings.audio = true;
            settings
        }
        "malformed" => Settings::new(&["roadEncodeData"]),
        _ => Settings::new(&["carState"]),
    }
}
pub(super) fn run(peer: &mut Peer, name: &str, fixtures: &Fixtures) -> Result<Value> {
    match name {
        "disabled" => {
            for part in 0..2 {
                for frame in 0..3 {
                    for (service, offset) in ENCODERS.into_iter().zip([70, 10, 0, 29]) {
                        let keyframe = if service == "qRoadEncodeData" {
                            None
                        } else {
                            Some(!(part == 1 && frame == 0))
                        };
                        peer.send(
                            service,
                            &fixtures::encoded(
                                fixtures.packet(service, frame)?,
                                service,
                                offset + part,
                                part as u32 * 10 + frame as u32,
                                keyframe,
                            )?,
                        )?;
                    }
                }
            }
            let logs = peer.stop(rustix::process::Signal::TERM)?;
            require(logs.len() == 2, "disabled recording segment count differs")?;
            for (part, row) in logs.iter().enumerate() {
                require(
                    !fs::read_dir(peer.segment(part))?.any(|e| {
                        e.is_ok_and(|e| e.path().extension().is_some_and(|s| s == "hevc"))
                    }),
                    "disabled recording emitted HEVC",
                )?;
                require(
                    row["rlog"]
                        .as_array()
                        .ok_or("disabled rlog missing")?
                        .iter()
                        .filter(|v| v.get("roadEncodeIdx").is_some())
                        .count()
                        == 3,
                    "disabled recording lost index packets",
                )?;
            }
            Ok(json!({"logs":logs}))
        }
        "selection" => {
            for service in [
                "livestreamRoadEncodeData",
                "livestreamWideRoadEncodeData",
                "livestreamDriverEncodeData",
                "youtubeRoadEncodeData",
                "rawAudioData",
            ] {
                peer.publish(service, b"malformed-unsubscribed-data", false)?;
            }
            peer.barrier()?;
            let logs = peer.stop(rustix::process::Signal::TERM)?;
            require(
                super::validation::normalize(&logs[0]["rlog"])?
                    .as_array()
                    .is_some_and(|v| v.len() == 3),
                "logger selected an excluded service",
            )?;
            Ok(json!({"logs":logs}))
        }
        "missing-audio" => {
            for sequence in 0..205 {
                peer.send(
                    "qRoadEncodeData",
                    &fixtures::encoded(
                        fixtures.packet("qRoadEncodeData", 0)?,
                        "qRoadEncodeData",
                        0,
                        sequence,
                        Some(true),
                    )?,
                )?;
            }
            peer.barrier()?;
            peer.signal(rustix::process::Signal::TERM)?;
            let exit = peer.wait(Duration::from_secs(20))?;
            require(exit != 0, "missing audio finalized successfully")?;
            require(
                peer.segment(0).join("qcamera.ts.lock").is_file()
                    && !peer.segment(0).join("rlog.lock").exists(),
                "missing-audio incomplete media/finalized rlog locks differ",
            )?;
            let logs = peer.logs()?;
            require(
                !logs[0]["rlog"]
                    .as_array()
                    .ok_or("missing-audio rlog absent")?
                    .iter()
                    .any(|v| v.get("qRoadEncodeIdx").is_some()),
                "missing audio published queued video indices",
            )?;
            require(
                fs::metadata(peer.segment(0).join("qcamera.ts"))?.len() == 0,
                "missing audio wrote video bytes",
            )?;
            let report = json!({"exit_status":exit,"video_bytes":0,"retained_lock":"qcamera.ts.lock","rlog_finalized":true,"qlog_finalized":true});
            save(&peer.root.join("shutdown.json"), &report)?;
            peer.cleanup()?;
            Ok(json!({"logs":logs,"report":report}))
        }
        "malformed" => {
            peer.publish("roadEncodeData", &[0xff; 64], false)?;
            let exit = peer.wait(Duration::from_secs(20))?;
            require(
                exit != 0 && peer.segment(0).join("rlog.lock").is_file(),
                "malformed logger payload lost failure/retained-lock contract",
            )?;
            let report = json!({"exit_status":exit,"incomplete_lock_retained":true});
            save(&peer.root.join("result.json"), &report)?;
            peer.cleanup()?;
            Ok(json!({"report":report}))
        }
        signal => {
            let number = signal
                .strip_prefix("signal-")
                .ok_or("unknown logger edge case")?
                .parse::<i32>()?;
            require(
                [2, 15, 30].contains(&number),
                "unsupported logger shutdown signal",
            )?;
            peer.send("carState", &fixtures::event("carState", 1)?)?;
            let received =
                rustix::process::Signal::from_raw(number).ok_or("invalid platform signal")?;
            let logs = peer.stop(received)?;
            let sentinel = &logs[0]["rlog"]
                .as_array()
                .and_then(|v| v.last())
                .ok_or("signal sentinel absent")?["sentinel"];
            require(
                *sentinel == json!({"type":"endOfRoute","signal":number}),
                "logger shutdown sentinel signal differs",
            )?;
            Ok(json!({"logs":logs,"report":{"exit_status":0,"sentinel_signal":number,"locks":0}}))
        }
    }
}
pub(super) fn fallback_settings() -> Settings {
    let mut settings = Settings::new(&["deviceState", "roadEncodeData"]);
    settings.test = false;
    settings
        .parameters
        .insert("RecordRoadCam".into(), "0".into());
    settings
}
pub(super) fn fallback_tick(
    peer: &mut Peer,
    stuck: bool,
    sequence: u32,
    road: &[u8],
) -> Result<()> {
    if stuck {
        peer.send(
            "roadEncodeData",
            &fixtures::encoded(road, "roadEncodeData", 0, sequence, None)?,
        )?;
    } else {
        peer.send("deviceState", &fixtures::event("deviceState", sequence)?)?;
    }
    peer.barrier()
}
