//! Preserve original case counts, ordering, stop barriers and recording policy.
use super::{
    fixtures::{self, Fixtures},
    peer::{Peer, Settings},
    require, Result,
};
use serde_json::Value;
pub(super) const ENCODERS: [&str; 4] = [
    "roadEncodeData",
    "wideRoadEncodeData",
    "driverEncodeData",
    "qRoadEncodeData",
];
const OFFSETS: [i32; 4] = [70, 10, 0, 29];
const ORDINARY: [&str; 8] = [
    "can",
    "carState",
    "deviceState",
    "controlsState",
    "modelV2",
    "livePose",
    "errorLogMessage",
    "logMessage",
];
pub(super) fn settings(name: &str) -> Settings {
    let services = ORDINARY
        .into_iter()
        .chain(ENCODERS)
        .chain(["userBookmark", "audioFeedback", "rawAudioData"])
        .collect::<Vec<_>>();
    let mut settings = Settings::new(&services);
    settings.audio = matches!(name, "audio" | "audio-queue");
    settings
}
pub(super) fn drive(peer: &mut Peer, name: &str, fixtures: &Fixtures) -> Result<()> {
    match name {
        "ordinary" => {
            for sequence in 0..101 {
                for service in ORDINARY {
                    peer.send(service, &fixtures::event(service, sequence)?)?;
                }
            }
        }
        "burst" => {
            peer.stopped()?;
            for sequence in 0..1000 {
                peer.publish("can", &fixtures::event("can", sequence)?, true)?;
            }
            peer.signal(rustix::process::Signal::CONT)?;
            peer.wait_readers("can")?;
            peer.barrier()?;
        }
        "fairness" => {
            peer.stopped()?;
            for (service, count) in [("can", 500), ("carState", 3), ("deviceState", 1)] {
                for sequence in 0..count {
                    peer.publish(service, &fixtures::event(service, sequence)?, true)?;
                }
            }
            peer.signal(rustix::process::Signal::CONT)?;
            for service in ["can", "carState", "deviceState"] {
                peer.wait_readers(service)?;
            }
            peer.barrier()?;
        }
        "video" => {
            for (service, offset) in ENCODERS.into_iter().zip(OFFSETS) {
                peer.send(
                    service,
                    &fixtures::encoded(
                        fixtures.packet(service, 0)?,
                        service,
                        offset,
                        999,
                        Some(false),
                    )?,
                )?;
            }
            for part in 0..3 {
                for frame in 0..30 {
                    for (service, offset) in ENCODERS.into_iter().zip(OFFSETS) {
                        peer.send(
                            service,
                            &fixtures::encoded(
                                fixtures.packet(service, frame)?,
                                service,
                                offset + part,
                                part as u32 * 1000 + frame as u32,
                                None,
                            )?,
                        )?;
                    }
                    peer.send(
                        "carState",
                        &fixtures::event("carState", part as u32 * 1000 + frame as u32)?,
                    )?;
                }
                for service in ["userBookmark", "audioFeedback"] {
                    peer.send(service, &fixtures::event(service, part as u32)?)?;
                }
                peer.barrier()?;
                let mut value = [0_u8; 16];
                let size = rustix::fs::getxattr(
                    peer.segment(part as usize),
                    "user.preserve",
                    value.as_mut_slice(),
                )?;
                require(
                    &value[..size] == b"1",
                    "logger bookmark/audio-feedback preservation xattr differs",
                )?;
            }
            require(
                std::fs::read_to_string(peer.params.join("AthenadRecentlyViewedRoutes"))?
                    == format!("old-route{}", format!(",{}", peer.route).repeat(3)),
                "logger route-viewed Params update differs",
            )?;
        }
        "audio" => {
            peer.send(
                "qRoadEncodeData",
                &fixtures::encoded(
                    fixtures.packet("qRoadEncodeData", 0)?,
                    "qRoadEncodeData",
                    29,
                    0,
                    None,
                )?,
            )?;
            for sequence in 0..225 {
                peer.send("rawAudioData", &fixtures::event("rawAudioData", sequence)?)?;
            }
            for frame in 1..30 {
                peer.send(
                    "qRoadEncodeData",
                    &fixtures::encoded(
                        fixtures.packet("qRoadEncodeData", frame)?,
                        "qRoadEncodeData",
                        29,
                        frame as u32,
                        None,
                    )?,
                )?;
                peer.send(
                    "rawAudioData",
                    &fixtures::event("rawAudioData", 225 + frame as u32)?,
                )?;
            }
        }
        "audio-queue" => {
            for frame in 0..205 {
                peer.send(
                    "qRoadEncodeData",
                    &fixtures::encoded(
                        fixtures.packet("qRoadEncodeData", 0)?,
                        "qRoadEncodeData",
                        0,
                        frame,
                        Some(true),
                    )?,
                )?;
            }
            peer.send("rawAudioData", &fixtures::event("rawAudioData", 0)?)?;
            peer.send(
                "qRoadEncodeData",
                &fixtures::encoded(
                    fixtures.packet("qRoadEncodeData", 0)?,
                    "qRoadEncodeData",
                    0,
                    206,
                    Some(true),
                )?,
            )?;
        }
        "queue-restart" => {
            for (service, offset) in ENCODERS.into_iter().zip(OFFSETS) {
                peer.send(
                    service,
                    &fixtures::encoded(fixtures.packet(service, 0)?, service, offset, 0, None)?,
                )?;
            }
            for frame in 0..205 {
                peer.send(
                    "roadEncodeData",
                    &fixtures::encoded(
                        fixtures.packet("roadEncodeData", frame % 160)?,
                        "roadEncodeData",
                        71,
                        frame as u32 + 1000,
                        None,
                    )?,
                )?;
            }
            for (service, offset) in ENCODERS.into_iter().zip(OFFSETS).skip(1) {
                peer.send(
                    service,
                    &fixtures::encoded(
                        fixtures.packet(service, 0)?,
                        service,
                        offset + 1,
                        1000,
                        None,
                    )?,
                )?;
            }
            for (fixture, part, frame) in [(0, 71, 2000), (0, 0, 2001), (1, 0, 2002)] {
                peer.send(
                    "roadEncodeData",
                    &fixtures::encoded(
                        fixtures.packet("roadEncodeData", fixture)?,
                        "roadEncodeData",
                        part,
                        frame,
                        None,
                    )?,
                )?;
            }
            peer.barrier()?;
            require(
                peer.segment(1).is_dir(),
                "logger restarted source queue did not rotate",
            )?;
        }
        _ => return Err("unknown logger input scenario".into()),
    }
    Ok(())
}
pub(super) fn assert_logs(name: &str, logs: &[Value]) -> Result<()> {
    fn ids(logs: &[Value], part: usize, service: &str) -> Result<Vec<u64>> {
        logs.get(part)
            .and_then(|v| v["rlog"].as_array())
            .ok_or("logger scenario segment missing")?
            .iter()
            .filter_map(|v| v.get(service))
            .map(|v| {
                v["frameId"]
                    .as_u64()
                    .ok_or_else(|| "logger index frame missing".into())
            })
            .collect()
    }
    match name {
        "audio-queue" => {
            let mut expected = (0..201).collect::<Vec<_>>();
            expected.push(206);
            require(
                ids(logs, 0, "qRoadEncodeIdx")? == expected,
                "logger audio queue boundary indices differ",
            )?;
        }
        "queue-restart" => {
            let mut expected = (1000..1201).collect::<Vec<_>>();
            expected.extend([2000, 2002]);
            require(
                ids(logs, 1, "roadEncodeIdx")? == expected,
                "logger queue restart indices differ",
            )?;
        }
        "fairness" => {
            let actual = logs
                .first()
                .and_then(|v| v["rlog"].as_array())
                .ok_or("fairness rlog missing")?
                .iter()
                .filter_map(|row| {
                    ["can", "carState", "deviceState"]
                        .into_iter()
                        .find(|service| row.get(*service).is_some())
                })
                .collect::<Vec<_>>();
            let expected = std::iter::repeat_n("can", 200)
                .chain(std::iter::repeat_n("carState", 3))
                .chain(["deviceState"])
                .chain(std::iter::repeat_n("can", 300))
                .collect::<Vec<_>>();
            require(
                actual == expected,
                "logger per-service fairness order differs",
            )?;
        }
        _ => {}
    }
    Ok(())
}
