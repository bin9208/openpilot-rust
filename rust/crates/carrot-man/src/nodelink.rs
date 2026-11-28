//! Lane counts from the Korean standard node-link (ITS 표준노드링크), in the
//! NLK1 file `tools/navigation/nodelink_nlk.py` builds. carrot's ATC knows a
//! turn's side and distance but not how many lanes it must cross; the
//! node-link knows each directed link's lane count.
use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Seek, SeekFrom};

pub const PATH: &str = "/data/navigation/nodelink/nodelink_kr.nlk";
/// The file is too large for the repository: carrot-man downloads this pinned
/// release asset when PATH is missing or differs (tools/navigation/nodelink_nlk.py
/// builds it; publish a new asset and change these two to update it).
pub const URL: &str =
    "https://github.com/bin9208/openpilot-rust/releases/download/nodelink-20261128/nodelink_kr.nlk";
pub const SHA256: &str = "d4700dc164948e745aee40b2d13c86c053e80be683fd424d527531c25096fef0";
const MAX_BYTES: u64 = 64 << 20;
const TILE: f64 = 0.01;
const UNIT: f64 = 0.25;
const METERS: f64 = 111_320.;
const CACHE: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    /// Lanes in the link's direction (0 = unknown).
    pub lanes: u8,
    /// Road rank x10: 10 expressway, 9 urban expressway, ... 0 unknown.
    pub rank: u8,
    /// Legal maximum, km/h (0 = unknown).
    pub speed: u8,
}

struct Chunk {
    link: Link,
    /// (north, east) metres from the cell's south-west corner.
    points: Vec<[f64; 2]>,
}

struct Tile {
    scale: f64,
    chunks: Vec<Chunk>,
}

pub struct NodeLinks<R> {
    source: R,
    directory: HashMap<(i32, i32), (u32, u32)>,
    cache: HashMap<(i32, i32), Tile>,
    order: VecDeque<(i32, i32)>,
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

impl NodeLinks<std::fs::File> {
    pub fn open(path: &std::path::Path) -> io::Result<Self> {
        Self::new(std::fs::File::open(path)?)
    }
}

impl<R: Read + Seek> NodeLinks<R> {
    pub fn new(mut source: R) -> io::Result<Self> {
        let mut header = [0u8; 12];
        source.read_exact(&mut header)?;
        if &header[..4] != b"NLK1" {
            return Err(invalid("not an NLK1 file"));
        }
        let count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        if count > 4_000_000 {
            return Err(invalid("tile count"));
        }
        let mut raw = vec![0u8; count * 16];
        source.read_exact(&mut raw)?;
        let word = |b: &[u8]| [b[0], b[1], b[2], b[3]];
        let directory = raw
            .chunks_exact(16)
            .map(|e| {
                (
                    (
                        i32::from_le_bytes(word(&e[0..])),
                        i32::from_le_bytes(word(&e[4..])),
                    ),
                    (
                        u32::from_le_bytes(word(&e[8..])),
                        u32::from_le_bytes(word(&e[12..])),
                    ),
                )
            })
            .collect();
        Ok(Self {
            source,
            directory,
            cache: HashMap::new(),
            order: VecDeque::new(),
        })
    }

    fn load(&mut self, key: (i32, i32)) -> bool {
        if self.cache.contains_key(&key) {
            return true;
        }
        let Some(&(offset, length)) = self.directory.get(&key) else {
            return false;
        };
        let mut blob = vec![0u8; length as usize];
        if self
            .source
            .seek(SeekFrom::Start(u64::from(offset)))
            .is_err()
            || self.source.read_exact(&mut blob).is_err()
        {
            return false;
        }
        let Some(chunks) = decode(&blob) else {
            return false;
        };
        let latitude = f64::from(key.0) * TILE + TILE / 2.;
        if self.order.len() >= CACHE {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
        self.order.push_back(key);
        self.cache.insert(
            key,
            Tile {
                scale: latitude.to_radians().cos(),
                chunks,
            },
        );
        true
    }

    /// The nearest link within `max_distance` m whose direction is within
    /// `max_angle` degrees of `bearing` (north 0, east 90).
    pub fn matched(
        &mut self,
        latitude: f64,
        longitude: f64,
        bearing: f64,
        max_distance: f64,
        max_angle: f64,
    ) -> Option<Link> {
        if ![latitude, longitude, bearing].iter().all(|v| v.is_finite()) {
            return None;
        }
        let base = (
            (latitude / TILE).floor() as i32,
            (longitude / TILE).floor() as i32,
        );
        let mut best: Option<(f64, Link)> = None;
        for dlat in -1..=1 {
            for dlon in -1..=1 {
                let key = (base.0 + dlat, base.1 + dlon);
                if !self.load(key) {
                    continue;
                }
                let tile = &self.cache[&key];
                let p = [
                    (latitude - f64::from(key.0) * TILE) * METERS,
                    (longitude - f64::from(key.1) * TILE) * METERS * tile.scale,
                ];
                for chunk in &tile.chunks {
                    for segment in chunk.points.windows(2) {
                        let (a, b) = (segment[0], segment[1]);
                        let ab = [b[0] - a[0], b[1] - a[1]];
                        let length = ab[0] * ab[0] + ab[1] * ab[1];
                        if length < 1e-6 {
                            continue;
                        }
                        let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / length)
                            .clamp(0., 1.);
                        let distance = (a[0] + t * ab[0] - p[0]).hypot(a[1] + t * ab[1] - p[1]);
                        let heading = ab[1].atan2(ab[0]).to_degrees();
                        let angle = ((heading - bearing + 540.).rem_euclid(360.) - 180.).abs();
                        if distance <= max_distance
                            && angle <= max_angle
                            && best.is_none_or(|(d, _)| distance < d)
                        {
                            best = Some((distance, chunk.link));
                        }
                    }
                }
            }
        }
        best.map(|(_, link)| link)
    }
}

fn sha256(path: &std::path::Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Make `path` the pinned file: keep it when its hash matches, otherwise
/// download URL beside it with curl and move it in once the hash matches.
pub fn ensure(path: &std::path::Path) -> io::Result<()> {
    if sha256(path).is_ok_and(|h| h == SHA256) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = path.with_extension("nlk.part");
    let status = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--retry",
            "3",
            "--max-time",
            "900",
        ])
        .arg("--max-filesize")
        .arg(MAX_BYTES.to_string())
        .arg("-o")
        .arg(&part)
        .arg(URL)
        .stdin(std::process::Stdio::null())
        .status()?;
    let valid = status.success() && sha256(&part).is_ok_and(|h| h == SHA256);
    if !valid {
        let _ = std::fs::remove_file(&part);
        return Err(invalid("node-link download failed or hash mismatch"));
    }
    std::fs::rename(&part, path)
}

fn decode(blob: &[u8]) -> Option<Vec<Chunk>> {
    let count = u32::from_le_bytes(blob.get(..4)?.try_into().ok()?) as usize;
    let mut at = 4;
    let mut chunks = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let head = blob.get(at..at + 4)?;
        let link = Link {
            lanes: head[0],
            rank: head[1],
            speed: head[2],
        };
        let n = usize::from(head[3]);
        at += 4;
        let raw = blob.get(at..at + 4 * n)?;
        at += 4 * n;
        let points = raw
            .chunks_exact(4)
            .map(|q| {
                [
                    f64::from(i16::from_le_bytes([q[0], q[1]])) * UNIT,
                    f64::from(i16::from_le_bytes([q[2], q[3]])) * UNIT,
                ]
            })
            .collect();
        chunks.push(Chunk { link, points });
    }
    Some(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// One cell with a two-lane northbound and a four-lane eastbound link.
    fn file() -> Vec<u8> {
        let mut blob = 2u32.to_le_bytes().to_vec();
        let mut chunk = |lanes: u8, points: &[[f64; 2]]| {
            blob.extend([lanes, 7, 60, points.len() as u8]);
            for p in points {
                blob.extend(((p[0] / UNIT) as i16).to_le_bytes());
                blob.extend(((p[1] / UNIT) as i16).to_le_bytes());
            }
        };
        chunk(2, &[[100., 300.], [900., 300.]]);
        chunk(4, &[[500., 100.], [500., 800.]]);
        let mut out = b"NLK1".to_vec();
        out.extend(1u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(3750i32.to_le_bytes());
        out.extend(12700i32.to_le_bytes());
        out.extend(28u32.to_le_bytes());
        out.extend((blob.len() as u32).to_le_bytes());
        out.extend(blob);
        out
    }

    fn at(north: f64, east: f64) -> (f64, f64) {
        let scale = (37.505f64).to_radians().cos();
        (37.5 + north / METERS, 127.0 + east / (METERS * scale))
    }

    #[test]
    fn links_match_by_distance_and_direction() {
        let mut links = NodeLinks::new(Cursor::new(file())).unwrap();
        let (lat, lon) = at(300., 303.);
        assert_eq!(links.matched(lat, lon, 0., 20., 45.).unwrap().lanes, 2);
        assert_eq!(
            links.matched(lat, lon, 180., 20., 45.),
            None,
            "opposite direction"
        );
        let (lat, lon) = at(505., 400.);
        assert_eq!(links.matched(lat, lon, 90., 20., 45.).unwrap().lanes, 4);
        let (lat, lon) = at(300., 360.);
        assert_eq!(links.matched(lat, lon, 0., 20., 45.), None, "too far");
    }

    #[test]
    fn file_hashes_are_sha256_hex() {
        let path = std::env::temp_dir().join(format!("nlk-hash-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_missing_cell_or_bad_file_matches_nothing() {
        let mut links = NodeLinks::new(Cursor::new(file())).unwrap();
        assert_eq!(links.matched(36.0, 127.0, 0., 20., 45.), None);
        assert!(NodeLinks::new(Cursor::new(b"NOPE00000000".to_vec())).is_err());
    }
}
