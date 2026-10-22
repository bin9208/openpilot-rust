//! Actual source FfmpegEncoder capture and exact source-authored event inputs.
use super::process::Process;
use super::{require, save, Result};
use openpilot_cereal::log_capnp::{encode_data, event};
use openpilot_msgq::Subscriber;
use serde_json::json;
use std::{fs, io::Cursor, path::Path, time::Duration};
pub(super) struct Fixtures {
    pub road: Vec<Vec<u8>>,
    pub qroad: Vec<Vec<u8>>,
}
impl Fixtures {
    pub fn load(path: &Path) -> Result<Self> {
        fn files(path: &Path) -> Result<Vec<Vec<u8>>> {
            let mut paths = fs::read_dir(path)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.retain(|p| p.extension().is_some_and(|v| v == "capnp"));
            paths.sort();
            let rows = paths
                .into_iter()
                .map(fs::read)
                .collect::<std::io::Result<Vec<_>>>()?;
            require(!rows.is_empty(), "source encoder fixture list is empty")?;
            Ok(rows)
        }
        Ok(Self {
            road: files(&path.join("road"))?,
            qroad: files(&path.join("qroad"))?,
        })
    }
    pub fn packet(&self, service: &str, frame: usize) -> Result<&[u8]> {
        let rows = if service == "qRoadEncodeData" {
            &self.qroad
        } else {
            &self.road
        };
        Ok(rows
            .get(frame)
            .ok_or("source encoder fixture index absent")?)
    }
}
pub(super) fn capture(binary: &Path, output: &Path, kind: &str) -> Result<Vec<Vec<u8>>> {
    fs::create_dir_all(output)?;
    let service = if kind == "road" {
        "roadEncodeData"
    } else {
        "qRoadEncodeData"
    };
    let capacity = super::peer::capacity(service)?;
    let mut subscriber = Subscriber::for_runtime(service, false, capacity)?;
    let mut command = std::process::Command::new(binary);
    command.arg(kind).env("PARAMS_ROOT", output.join("params"));
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    fs::create_dir_all(output.join("params").join(prefix))?;
    let mut producer = Process::start(&mut command, output, Duration::from_secs(30))?;
    require(
        producer.line()? == b"ready",
        "source encoder did not become ready",
    )?;
    let mut packets = Vec::new();
    fn drain(subscriber: &mut Subscriber, packets: &mut Vec<Vec<u8>>) -> Result<()> {
        while let Some(packet) = subscriber.receive(Duration::ZERO)? {
            packets.push(packet);
        }
        Ok(())
    }
    drain(&mut subscriber, &mut packets)?;
    packets.clear();
    for frame in 1..=160 {
        producer.write(format!("frame {frame}\n").as_bytes())?;
        require(
            producer.line()? == b"ok",
            "source encoder frame acknowledgement differs",
        )?;
        drain(&mut subscriber, &mut packets)?;
    }
    producer.write(b"quit\n")?;
    require(
        producer.wait(Duration::from_secs(20))? == 0,
        "source encoder failed",
    )?;
    require(
        !packets.is_empty(),
        "source encoder produced no actual packets",
    )?;
    let mut index = Vec::new();
    for (number, packet) in packets.iter().enumerate() {
        let name = format!("{number:04}.capnp");
        fs::write(output.join(&name), packet)?;
        index.push(json!({"path":name,"sha256":super::hash(packet)}));
    }
    save(
        &output.join("index.json"),
        &json!({"producer_sha256":crate::seal::digest(binary)?,"input_frames":160,"packets":index}),
    )?;
    Ok(packets)
}
pub(super) fn encoded(
    record: &[u8],
    service: &str,
    segment: i32,
    frame: u32,
    keyframe: Option<bool>,
) -> Result<Vec<u8>> {
    let source = capnp::serialize::read_message(Cursor::new(record), Default::default())?;
    let source = source.get_root::<event::Reader<'_>>()?;
    let data: encode_data::Reader<'_> = match source.which()? {
        event::RoadEncodeData(v) | event::QRoadEncodeData(v) => v?,
        _ => return Err("encoder fixture union differs".into()),
    };
    let mut message = capnp::message::Builder::new_default();
    {
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_log_mono_time(4_000_000_000 + u64::from(frame) * 100_000);
        root.set_valid(frame.is_multiple_of(2));
        match service {
            "roadEncodeData" => root.set_road_encode_data(data)?,
            "wideRoadEncodeData" => root.set_wide_road_encode_data(data)?,
            "driverEncodeData" => root.set_driver_encode_data(data)?,
            "qRoadEncodeData" => root.set_q_road_encode_data(data)?,
            _ => return Err("unsupported source encoder service".into()),
        }
        let data = match service {
            "roadEncodeData" => root.get_road_encode_data()?,
            "wideRoadEncodeData" => root.get_wide_road_encode_data()?,
            "driverEncodeData" => root.get_driver_encode_data()?,
            "qRoadEncodeData" => root.get_q_road_encode_data()?,
            _ => unreachable!(),
        };
        let mut index = data.get_idx()?;
        index.set_segment_num(segment);
        index.set_frame_id(frame);
        index.set_encode_id(frame);
        index.set_segment_id(frame);
        index.set_timestamp_sof(1_000_000_000 + u64::from(frame) * 50_000_000);
        index.set_timestamp_eof(1_001_000_000 + u64::from(frame) * 50_000_000);
        if let Some(keyframe) = keyframe {
            index.set_flags(if keyframe { 8 } else { 0 });
        }
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub(super) fn event(service: &str, sequence: u32) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_log_mono_time(5_000_000_000 + u64::from(sequence) * 50_000_000);
        root.set_valid(sequence.is_multiple_of(2));
        match service {
            "logMessage" => root.set_log_message(format!("fixture-{sequence:08}").as_str()),
            "errorLogMessage" => {
                root.set_error_log_message(format!("fixture-{sequence:08}").as_str())
            }
            "can" | "sendcan" => {
                let list = if service == "can" {
                    root.init_can(1)
                } else {
                    root.init_sendcan(1)
                };
                let mut data = list.get(0);
                data.set_address(sequence);
                data.set_dat(&[(sequence % 256) as u8, 2, 3]);
            }
            "rawAudioData" => {
                let mut data = root.init_raw_audio_data();
                data.set_sample_rate(16000);
                data.set_data(&[0x40, 0x10].repeat(800));
            }
            "carState" => {
                root.init_car_state();
            }
            "deviceState" => {
                root.init_device_state();
            }
            "controlsState" => {
                root.init_controls_state();
            }
            "modelV2" => {
                root.init_model_v2();
            }
            "livePose" => {
                root.init_live_pose();
            }
            "userBookmark" => {
                root.init_user_bookmark();
            }
            "audioFeedback" => {
                root.init_audio_feedback();
            }
            _ => return Err("unsupported source fixture event".into()),
        }
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub(super) fn barrier() -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    {
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_log_mono_time(now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64);
        root.set_valid(false);
        root.set_log_message("logger-qa-barrier");
    }
    capnp::serialize::write_message_to_words(&message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_can_bytes_equal_independent_original_controller_capture() {
        // Synthetic packet, sealed original run39791779545 artifact12432287942:
        // logger-native/burst-original/inputs/0123-can.capnp (sequence122;
        // initial barrier occupies input0000). Whole ZIP SHA3655dfbe...3a516.
        assert_eq!(
            super::super::hash(&event("can", 122).unwrap()),
            "0223690f61a1a871d7bf85e45ec8f44bb2464bfe0201dbd943f04dd62da58d4f"
        );
    }
}

#[cfg(test)]
mod barrier_tests {
    use super::*;
    #[test]
    fn barrier_preserves_new_message_false_validity_and_boottime() {
        let bytes = barrier();
        let message =
            capnp::serialize::read_message(Cursor::new(bytes), Default::default()).unwrap();
        let event = message.get_root::<event::Reader<'_>>().unwrap();
        assert!(!event.get_valid());
        assert!(event.get_log_mono_time() > 0);
        assert_eq!(
            event.get_log_message().unwrap().to_str().unwrap(),
            "logger-qa-barrier"
        );
    }
}
