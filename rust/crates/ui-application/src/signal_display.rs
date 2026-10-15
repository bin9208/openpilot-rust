//! Display-only signal memory from carrot-wip signal_display.py (MIT).
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

pub const DISPLAY_PATH: &str = "/dev/shm/carrot_signal_display.json";
pub const STATUS_PATH: &str = "/dev/shm/carrot_signal_display_status.json";
pub type CameraFrame = (u32, f64, usize, usize, bool);

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: i64,
    pub bounds: [f64; 4],
    pub state: String,
    pub confirmed: bool,
    pub held: bool,
    pub age: f64,
}
#[derive(Clone, PartialEq)]
struct Key(String, f64, f64);
#[derive(Clone)]
struct Saved {
    track: Track,
    observed: f64,
    color_time: f64,
    key: Key,
    order: u64,
}
#[derive(Default)]
pub struct Continuity {
    key: Option<Key>,
    tracks: BTreeMap<i64, Saved>,
    order: u64,
}
fn valid(record: &Value, now: f64, camera: CameraFrame, hold: bool) -> bool {
    let (fid, timestamp, width, height, wide) = camera;
    let limit = if hold { 2. } else { 0.35 };
    let skew = if hold { 2. } else { 0.25 };
    let Some(observed) = record["timestamp"].as_f64() else {
        return false;
    };
    let Some(frame) = record["frame_id"].as_f64() else {
        return false;
    };
    !wide
        && (width, height) == (1344, 760)
        && record["version"] == 1
        && record["stream"] == "road"
        && record["size"] == json!([1344, 760])
        && (0. ..=0.35).contains(&(now - timestamp))
        && (0. ..=limit).contains(&(now - observed))
        && (timestamp - observed).abs() <= skew
        && (f64::from(fid) - frame).abs() <= if hold { 40. } else { 5. }
        && record["session"].as_str().is_some_and(|v| !v.is_empty())
        && record["tracks"].as_array().is_some_and(|v| v.len() <= 20)
}
fn visible(record: &Value, now: f64, camera: CameraFrame, hold: bool) -> Vec<Track> {
    if !valid(record, now, camera, hold) {
        return Vec::new();
    }
    record["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let bounds: [f64; 4] = value["box"]
                .as_array()?
                .iter()
                .map(Value::as_f64)
                .collect::<Option<Vec<_>>>()?
                .try_into()
                .ok()?;
            let [x1, y1, x2, y2] = bounds;
            let id = value["id"].as_i64()?;
            let valid_geometry = bounds.iter().all(|v| v.is_finite())
                && 0. <= x1
                && x1 < x2
                && x2 <= 1344.
                && 0. <= y1
                && y1 < y2
                && y2 <= 760.;
            if !valid_geometry || value["age"].as_f64() != Some(0.) || !(1..1_000_000).contains(&id)
            {
                return None;
            }
            let state = match value["raw"].as_str() {
                Some("red") => "red",
                Some("green") => "green",
                _ => "unknown",
            };
            Some(Track {
                id,
                bounds,
                state: state.into(),
                confirmed: state != "unknown" && value["state"] == state,
                held: false,
                age: 0.,
            })
        })
        .collect()
}
fn same_housing(a: [f64; 4], b: [f64; 4]) -> bool {
    let intersection =
        (a[2].min(b[2]) - a[0].max(b[0])).max(0.) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.);
    let union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - intersection;
    intersection / union.max(1.) >= 0.5
}
impl Continuity {
    pub fn update(&mut self, record: &Value, now: f64, camera: Option<CameraFrame>) -> Vec<Track> {
        let Some(camera) = camera.filter(|camera| valid(record, now, *camera, true)) else {
            *self = Self::default();
            return Vec::new();
        };
        let key = Key(
            record["session"].as_str().unwrap_or_default().into(),
            record["timestamp"].as_f64().unwrap_or_default(),
            record["frame_id"].as_f64().unwrap_or_default(),
        );
        if self
            .key
            .as_ref()
            .is_some_and(|old| key.0 != old.0 || key.1 < old.1 || key.2 < old.2)
        {
            *self = Self::default();
        }
        if self.key.as_ref() != Some(&key) {
            let current = visible(record, now, camera, true);
            let mut inherited = BTreeMap::new();
            for track in &current {
                if self.tracks.contains_key(&track.id) || track.state != "unknown" {
                    continue;
                }
                let matches = self
                    .tracks
                    .iter()
                    .filter(|(id, old)| {
                        !current.iter().any(|t| t.id == **id)
                            && now - old.color_time <= 2.
                            && same_housing(old.track.bounds, track.bounds)
                    })
                    .collect::<Vec<_>>();
                if let [(_, old)] = matches.as_slice() {
                    if current
                        .iter()
                        .filter(|t| same_housing(old.track.bounds, t.bounds))
                        .count()
                        == 1
                    {
                        inherited.insert(track.id, (**old).clone());
                    }
                }
            }
            self.tracks.retain(|id, old| {
                current.iter().any(|t| t.id == *id)
                    || !current.iter().any(|t| {
                        let a = old.track.bounds;
                        let b = t.bounds;
                        a[0].max(b[0]) < a[2].min(b[2]) && a[1].max(b[1]) < a[3].min(b[3])
                    })
            });
            for mut track in current {
                let previous = self
                    .tracks
                    .get(&track.id)
                    .or_else(|| inherited.get(&track.id));
                let mut color_time = key.1;
                if let Some(previous) =
                    previous.filter(|old| track.state == "unknown" && now - old.color_time <= 2.)
                {
                    track.state.clone_from(&previous.track.state);
                    track.confirmed = false;
                    color_time = previous.color_time;
                }
                let order = self
                    .tracks
                    .get(&track.id)
                    .map_or(self.order, |old| old.order);
                self.order = self.order.wrapping_add(1);
                self.tracks.insert(
                    track.id,
                    Saved {
                        track,
                        observed: key.1,
                        color_time,
                        key: key.clone(),
                        order,
                    },
                );
            }
            self.key = Some(key.clone());
        }
        let fresh = visible(record, now, camera, false);
        let mut saved = self
            .tracks
            .values()
            .filter(|v| now - v.observed <= 2.)
            .collect::<Vec<_>>();
        saved.sort_by(|a, b| {
            b.observed
                .total_cmp(&a.observed)
                .then(a.order.cmp(&b.order))
        });
        let result = saved
            .into_iter()
            .take(20)
            .map(|saved| {
                let mut track = saved.track.clone();
                if now - saved.color_time > 2. {
                    track.state = "unknown".into();
                    track.confirmed = false;
                }
                track.held = saved.key != key
                    || !fresh.iter().any(|t| t.id == track.id)
                    || saved.color_time != saved.observed;
                track.age = now - saved.observed;
                if track.held {
                    track.confirmed = false;
                }
                track
            })
            .collect::<Vec<_>>();
        self.tracks
            .retain(|id, _| result.iter().any(|t| t.id == *id));
        // Source rebuilds its ordered dictionary in the displayed order.
        for (index, track) in result.iter().enumerate() {
            if let Some(saved) = self.tracks.get_mut(&track.id) {
                saved.order = u64::try_from(index).unwrap_or_default();
            }
        }
        self.order = u64::try_from(result.len()).unwrap_or_default();
        result
    }
}
#[derive(Default)]
pub struct Reader {
    pub enabled: bool,
    next_flag: f64,
    next_read: f64,
    next_status: f64,
    record: Value,
    bytes: Option<Vec<u8>>,
    continuity: Continuity,
    held: usize,
}
impl Reader {
    pub fn read(
        &mut self,
        now: f64,
        camera: Option<CameraFrame>,
        requested: impl FnOnce() -> bool,
    ) -> Vec<Track> {
        if now >= self.next_flag {
            self.enabled = requested();
            self.next_flag = now + 0.5;
        }
        if !self.enabled {
            self.record = Value::Null;
            self.bytes = None;
            self.continuity = Continuity::default();
            return Vec::new();
        }
        if now >= self.next_read {
            self.next_read = now + 0.05;
            let bytes = fs::File::open(DISPLAY_PATH)
                .and_then(|file| {
                    let mut bytes = Vec::new();
                    file.take(8193).read_to_end(&mut bytes)?;
                    Ok(bytes)
                })
                .ok()
                .filter(|bytes| bytes.len() <= 8192);
            if bytes != self.bytes {
                self.record = bytes
                    .as_ref()
                    .and_then(|bytes| serde_json::from_slice(bytes).ok())
                    .unwrap_or(Value::Null);
                self.bytes = bytes;
            }
        }
        let tracks = self.continuity.update(&self.record, now, camera);
        self.held = tracks.iter().filter(|t| t.held).count();
        tracks
    }
    pub fn record_draw(&mut self, now: f64, camera: Option<CameraFrame>, count: usize) {
        if !self.enabled || now < self.next_status {
            return;
        }
        self.next_status = now + 1.;
        let value = json!({"display_revision":4,"timestamp":now,"boxes_drawn":count,"held_boxes":self.held,
            "camera_frame":camera,"observation_frame":self.record["frame_id"],"engine":self.record["engine"],
            "observation_timestamp":self.record["timestamp"]});
        let path = Path::new(STATUS_PATH);
        let temporary = path.with_extension("tmp");
        if let Ok(bytes) = serde_json::to_vec(&value) {
            if fs::write(&temporary, bytes).is_ok() {
                let _ = fs::rename(temporary, path);
            }
        }
    }
}

pub fn project_box(bounds: [f64; 4], video: [f64; 4]) -> [f64; 4] {
    let [x, y, w, h] = video;
    let [x1, y1, x2, y2] = bounds;
    [
        x + x1 * w / 1344.,
        y + y1 * h / 760.,
        (x2 - x1) * w / 1344.,
        (y2 - y1) * h / 760.,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: i64, raw: &str, timestamp: f64, frame: u32) -> Value {
        json!({"version":1,"stream":"road","size":[1344,760],"session":"a",
            "timestamp":timestamp,"frame_id":frame,"tracks":[
                {"id":id,"box":[100,200,130,250],"raw":raw,"state":raw,"age":0}]})
    }
    #[test]
    fn replacement_housing_holds_color_without_refreshing_its_timestamp() {
        let mut memory = Continuity::default();
        let first = memory.update(
            &record(1, "red", 10., 100),
            10.,
            Some((100, 10., 1344, 760, false)),
        );
        assert!(first[0].confirmed && !first[0].held);
        let next = record(2, "unknown", 10.5, 110);
        let held = memory.update(&next, 10.5, Some((110, 10.5, 1344, 760, false)));
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].state, "red");
        assert!(held[0].held && !held[0].confirmed);
        let expired = memory.update(&next, 12.01, Some((140, 12.01, 1344, 760, false)));
        assert_eq!(expired[0].state, "unknown");
        assert!(memory
            .update(&next, 12.51, Some((140, 12.51, 1344, 760, false)))
            .is_empty());
    }
    #[test]
    fn stale_or_wide_camera_resets_memory_and_boxes_follow_full_image_mapping() {
        let mut memory = Continuity::default();
        let current = record(1, "green", 10., 100);
        assert_eq!(
            memory
                .update(&current, 10., Some((100, 10., 1344, 760, false)))
                .len(),
            1
        );
        assert!(memory
            .update(&current, 10.1, Some((100, 10., 1344, 760, true)))
            .is_empty());
        assert!(memory.tracks.is_empty());
        assert!(memory
            .update(&current, 10.36, Some((100, 10., 1344, 760, false)))
            .is_empty());
        assert_eq!(
            project_box([0., 0., 1344., 760.], [12., 20., 672., 380.]),
            [12., 20., 672., 380.]
        );
    }
    #[test]
    fn equal_timestamp_ties_preserve_previous_display_order() {
        let mut memory = Continuity::default();
        let mut first = record(1, "red", 10., 100);
        let mut second = first["tracks"][0].clone();
        second["id"] = json!(2);
        second["box"] = json!([200, 200, 230, 250]);
        first["tracks"].as_array_mut().unwrap().push(second.clone());
        memory.update(&first, 10., Some((100, 10., 1344, 760, false)));
        let mut update = first.clone();
        update["timestamp"] = json!(10.1);
        update["frame_id"] = json!(101);
        update["tracks"] = json!([second]);
        let order =
            |tracks: Vec<Track>| tracks.into_iter().map(|track| track.id).collect::<Vec<_>>();
        assert_eq!(
            order(memory.update(&update, 10.1, Some((101, 10.1, 1344, 760, false)))),
            [2, 1]
        );
        first["timestamp"] = json!(10.2);
        first["frame_id"] = json!(102);
        assert_eq!(
            order(memory.update(&first, 10.2, Some((102, 10.2, 1344, 760, false)))),
            [2, 1]
        );
    }
}
