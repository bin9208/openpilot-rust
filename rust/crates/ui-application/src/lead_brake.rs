//! Fresh display-only lead-lamp candidates; no retained color or control input.
use crate::signal_display::CameraFrame;
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
pub const PATH: &str = "/dev/shm/carrot_lead_brake_lights.json";
#[derive(Clone, Debug)]
pub struct Observation {
    pub bounds: Option<[f64; 4]>,
    pub candidate: bool,
}
pub fn display_result(record: &Value, now: f64, camera: CameraFrame) -> Option<Observation> {
    let (frame, stamp, width, height, wide) = camera;
    let observed = record["timestamp"].as_f64()?;
    let fid = record["frame_id"].as_f64()?;
    if wide
        || (width, height) != (1344, 760)
        || record["version"] != 1
        || record["control_permission"] != false
        || !(0. ..=0.35).contains(&(now - stamp))
        || !(0. ..=0.35).contains(&(now - observed))
        || (stamp - observed).abs() > 0.25
        || (f64::from(frame) - fid).abs() > 5.
    {
        return None;
    }
    let candidate = match record["state"].as_str()? {
        "candidate" => true,
        "unknown" => false,
        _ => return None,
    };
    let bounds = if record.get("box").is_none_or(Value::is_null) {
        None
    } else {
        let values = record["box"].as_array()?;
        if values.len() != 4 {
            return None;
        }
        let b = [
            values[0].as_f64()?,
            values[1].as_f64()?,
            values[2].as_f64()?,
            values[3].as_f64()?,
        ];
        if !(b.iter().all(|v| v.is_finite())
            && 0. <= b[0]
            && b[0] < b[2]
            && b[2] <= width as f64
            && 0. <= b[1]
            && b[1] < b[3]
            && b[3] <= height as f64)
        {
            return None;
        }
        Some(b)
    };
    Some(Observation { bounds, candidate })
}
#[derive(Default)]
pub struct Reader {
    record: Value,
    next_read: f64,
    next_parameter: f64,
    enabled: bool,
    path: Option<PathBuf>,
}
impl Reader {
    pub fn read(
        &mut self,
        now: f64,
        camera: Option<CameraFrame>,
        requested: impl FnOnce() -> bool,
    ) -> Option<Observation> {
        if now >= self.next_parameter {
            self.enabled = requested();
            self.next_parameter = now + 0.5;
        }
        if !self.enabled {
            self.record = Value::Null;
            return None;
        }
        let camera = camera?;
        if now >= self.next_read {
            self.next_read = now + 0.05;
            self.record = (|| -> Option<Value> {
                let mut bytes = Vec::with_capacity(4097);
                fs::File::open(self.path.as_deref().unwrap_or(Path::new(PATH)))
                    .ok()?
                    .take(4097)
                    .read_to_end(&mut bytes)
                    .ok()?;
                if bytes.len() > 4096 {
                    return None;
                }
                serde_json::from_slice(&bytes).ok()
            })()
            .unwrap_or(Value::Null);
        }
        display_result(&self.record, now, camera)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn disabled_reader_does_not_open_output_and_enabled_reads_are_bounded_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observation");
        // A directory would fail decoding if opened. OFF never even advances the read timer.
        std::fs::create_dir(&path).unwrap();
        let mut reader = Reader {
            path: Some(path.clone()),
            ..Reader::default()
        };
        let camera = Some((20, 1.05, 1344, 760, false));
        assert!(reader.read(1.1, camera, || false).is_none());
        assert_eq!(reader.next_read, 0.);
        assert!(reader
            .read(1.2, camera, || panic!("parameter cache ignored"))
            .is_none());
        std::fs::remove_dir(&path).unwrap();
        let value = json!({"version":1,"frame_id":20,"timestamp":2.,"control_permission":false,"state":"candidate","box":[600,340,640,350]});
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let camera = Some((20, 2., 1344, 760, false));
        assert!(reader.read(2., camera, || true).unwrap().candidate);
        std::fs::write(&path, vec![b' '; 4097]).unwrap();
        assert!(reader
            .read(2.01, camera, || panic!("parameter cache ignored"))
            .is_some());
        assert!(reader
            .read(2.06, camera, || panic!("parameter cache ignored"))
            .is_none());
        assert!(reader.read(2.6, camera, || false).is_none());
        assert!(reader.record.is_null());
    }
    #[test]
    fn rejects_stale_wrong_camera_and_any_control_permission() {
        let value = json!({"version":1,"frame_id":20,"timestamp":1.,"control_permission":false,"state":"candidate","box":[600,340,640,350]});
        let camera = (20, 1.05, 1344, 760, false);
        assert!(display_result(&value, 1.1, camera).is_some());
        assert!(display_result(&value, 1.4, camera).is_none());
        assert!(display_result(&value, 1.1, (20, 1.05, 1344, 760, true)).is_none());
        assert!(display_result(&value, 1.1, (26, 1.05, 1344, 760, false)).is_none());
        let mut corrupt = value.clone();
        corrupt["control_permission"] = json!(true);
        assert!(display_result(&corrupt, 1.1, camera).is_none());
        corrupt = value.clone();
        corrupt["box"] = json!([-1, 340, 640, 350]);
        assert!(display_result(&corrupt, 1.1, camera).is_none());
    }
}
