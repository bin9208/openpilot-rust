//! Original Bluetooth gesture input corpus, generated without invoking Python.
use crate::seal::Result;
use openpilot_runtime_core::python_float::write_float;
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::journald_check::Random;

const HELPER: &str = "rust/tools/bluetooth_gesture_cases.py";
const RECORDING: &str = "openpilot/selfdrive/carrot/bluetooth/tests/yiser_j6.json";
const HELPER_SHA: &str = "66810b7ba9f149785d872ea4f4b5ab8ccdce19d26ba30e065f7fc54f1d388253";
const RECORDING_SHA: &str = "e68fb82c2b4927f02c6fc54363ce92fc443893c86e9ae25a006eb8b8dfc0440f";

fn identity(metadata: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}
struct Pin {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    hash: String,
    bytes: Vec<u8>,
}
impl Pin {
    fn open(path: PathBuf, expected: &str) -> Result<Self> {
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(format!("gesture source is not a regular file: {}", path.display()).into());
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        if identity(&metadata) != identity(&file.metadata()?) {
            return Err("gesture source changed while hashing".into());
        }
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if hash != expected {
            return Err(format!("gesture source hash changed: {}", path.display()).into());
        }
        Ok(Self {
            path,
            file,
            metadata,
            hash,
            bytes,
        })
    }
    fn verify(&self) -> Result<()> {
        let current = Self::open(self.path.clone(), &self.hash)?;
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&current.metadata)
        {
            return Err(format!("gesture source identity changed: {}", self.path.display()).into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Stamp {
    Float(f64),
    Integer(i64),
}
impl Stamp {
    fn value(self) -> f64 {
        match self {
            Self::Float(value) => value,
            Self::Integer(value) => value as f64,
        }
    }
}
#[derive(Clone)]
enum Operation {
    Feed(i64, i64, i64, Stamp),
    Flush(Stamp),
    Cancel,
}
fn feed(ops: &mut Vec<Operation>, kind: i64, code: i64, value: i64, at: f64) {
    ops.push(Operation::Feed(kind, code, value, Stamp::Float(at)));
}
fn flush(ops: &mut Vec<Operation>, at: f64) {
    ops.push(Operation::Flush(Stamp::Float(at)));
}
fn nextafter(value: f64, positive: bool) -> f64 {
    if positive {
        value.next_up()
    } else {
        value.next_down()
    }
}
fn operations() -> Vec<Operation> {
    let mut ops = Vec::new();
    let mut at = 20.0_f64;
    for boundary in [0.0_f64, 0.35, 0.4, 0.7, 1.5, 10.0] {
        for duration in [
            nextafter(boundary, false),
            boundary,
            nextafter(boundary, true),
        ] {
            feed(&mut ops, 1, 30, 1, at);
            feed(&mut ops, 0, 0, 0, at);
            flush(&mut ops, at + duration);
            feed(&mut ops, 1, 30, 0, at + duration);
            feed(&mut ops, 0, 0, 0, at + duration);
            flush(&mut ops, at + duration + 0.35);
            flush(&mut ops, at + duration + 0.4);
            at += 30.0;
        }
    }
    for gap in [
        0.1,
        0.35_f64.next_down(),
        0.35,
        0.35_f64.next_up(),
        0.4,
        0.5,
    ] {
        for stamp in [at, at + 0.1 + gap] {
            feed(&mut ops, 1, 30, 1, stamp);
            feed(&mut ops, 0, 0, 0, stamp);
            feed(&mut ops, 1, 30, 0, stamp + 0.1);
            feed(&mut ops, 0, 0, 0, stamp + 0.1);
        }
        flush(&mut ops, at + 1.0);
        at += 5.0;
    }
    for key in [30, 115, 255, 256, 329, 331, 351, 352, 767, 768, 65535] {
        feed(&mut ops, 1, key, 1, at);
        feed(&mut ops, 0, 0, 0, at);
        for offset in [0.69, 0.7, 1.19, 1.2, 2.2, 9.99, 10.0, 10.001] {
            flush(&mut ops, at + offset);
        }
        ops.push(Operation::Cancel);
        feed(&mut ops, 1, key, 0, at + 10.1);
        feed(&mut ops, 0, 0, 0, at + 10.1);
        at += 15.0;
    }
    for (x, y) in [
        (300, 500),
        (235, 435),
        (365, 565),
        (366, 565),
        (420, 850),
        (355, 785),
        (485, 915),
        (i32::MIN, i32::MAX),
        (-13, 12),
        (-12, 13),
        (12, -13),
        (13, -12),
    ] {
        feed(&mut ops, 3, 0, i64::from(x), at);
        feed(&mut ops, 3, 1, i64::from(y), at);
        feed(&mut ops, 1, 330, 1, at);
        feed(&mut ops, 0, 0, 0, at);
        for (dx, dy) in [(119, 0), (120, 0), (-120, 120), (0, -120), (0, 0)] {
            at += 0.2;
            feed(&mut ops, 3, 0, i64::from(x.saturating_add(dx)), at);
            feed(&mut ops, 3, 1, i64::from(y.saturating_add(dy)), at);
            feed(&mut ops, 0, 0, 0, at);
            flush(&mut ops, at + 0.8);
        }
        feed(&mut ops, 1, 330, 0, at + 1.0);
        feed(&mut ops, 0, 0, 0, at + 1.0);
        flush(&mut ops, at + 2.0);
        at += 5.0;
    }
    let mut rng = Random::new(155);
    let events = [
        (0, 0, 0),
        (0, 3, 0),
        (1, 30, 1),
        (1, 30, 0),
        (1, 30, 2),
        (1, 115, 1),
        (1, 115, 0),
        (1, 330, 1),
        (1, 330, 0),
        (3, 0, 300),
        (3, 1, 500),
    ];
    let advances = [-0.01, 0.0, 0.001, 0.1, 0.35, 0.4, 0.5, 0.7, 1.0, 10.0];
    let flushes = [0.0, 0.35, 0.5, 0.7, 11.0];
    for _ in 0..10_000 {
        at += advances[rng.below(advances.len() as u64) as usize];
        let (kind, code, value) = events[rng.below(events.len() as u64) as usize];
        feed(&mut ops, kind, code, value, at);
        flush(
            &mut ops,
            at + flushes[rng.below(flushes.len() as u64) as usize],
        );
        if rng.below(10) == 0 {
            ops.push(Operation::Cancel);
        }
    }
    ops
}
fn stamp(value: Stamp, output: &mut String) -> Result<()> {
    match value {
        Stamp::Float(value) => write_float(value, output)?,
        Stamp::Integer(value) => output.push_str(&value.to_string()),
    }
    Ok(())
}
fn op_json(op: &Operation, output: &mut String) -> Result<()> {
    match op {
        Operation::Feed(kind, code, value, at) => {
            output.push_str(&format!("{{\"op\": \"feed\", \"event\": {{\"kind\": {kind}, \"code\": {code}, \"value\": {value}, \"at\": "));
            stamp(*at, output)?;
            output.push_str("}}");
        }
        Operation::Flush(at) => {
            output.push_str("{\"op\": \"flush\", \"at\": ");
            stamp(*at, output)?;
            output.push('}');
        }
        Operation::Cancel => output.push_str("{\"op\": \"cancel\"}"),
    }
    Ok(())
}
fn mapping(output: &mut String, mode: usize) -> Result<()> {
    output.push('{');
    if mode > 0 {
        let mut first = true;
        for token in [
            "key:30", "key:115", "1", "2", "up", "down", "left", "right", "center",
        ] {
            for (suffix, action) in [
                ("", "gapAdjustCruise"),
                ("@long", "accelCruiseLong"),
                ("@double", "cancel"),
            ] {
                if !first {
                    output.push_str(", ");
                }
                first = false;
                output.push_str(&serde_json::to_string(&format!("{token}{suffix}"))?);
                output.push_str(": ");
                output.push_str(&serde_json::to_string(if mode == 1 {
                    action
                } else {
                    "none"
                })?);
            }
        }
    }
    output.push('}');
    Ok(())
}
fn case(
    output: &mut String,
    profile: &str,
    mode: usize,
    learning: bool,
    ops: &[Operation],
) -> Result<()> {
    output.push_str("{\"profile\": ");
    output.push_str(&serde_json::to_string(profile)?);
    output.push_str(", \"mapping\": ");
    mapping(output, mode)?;
    output.push_str(if learning {
        ", \"learning\": true, \"operations\": ["
    } else {
        ", \"learning\": false, \"operations\": ["
    });
    for (index, op) in ops.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        op_json(op, output)?;
    }
    output.push_str("]}\n");
    Ok(())
}
fn recording(bytes: &[u8]) -> Result<Vec<Operation>> {
    let rows: Vec<Vec<Box<RawValue>>> = serde_json::from_slice(bytes)?;
    let mut ops = Vec::new();
    for row in rows {
        if row.len() != 4 {
            return Err("gesture recording row width changed".into());
        }
        let text = row[0].get();
        let at = if text.contains('.') || text.contains('e') || text.contains('E') {
            Stamp::Float(text.parse()?)
        } else {
            Stamp::Integer(text.parse()?)
        };
        let kind = row[1].get().parse()?;
        let code = row[2].get().parse()?;
        let value = row[3].get().parse()?;
        ops.push(Operation::Feed(kind, code, value, at));
    }
    Ok(ops)
}
fn render(recorded: &[Operation]) -> Result<String> {
    let mut output = String::new();
    let shared = operations();
    for profile in ["generic", "yiser-j6"] {
        for mode in 0..3 {
            for learning in [false, true] {
                case(&mut output, profile, mode, learning, &shared)?;
            }
        }
    }
    for (boundary, middle) in [
        (0.0_f64, Some(0)),
        (0.35_f64, None),
        (0.4_f64, None),
        (0.7_f64, None),
        (1.5_f64, None),
        (10.0_f64, Some(10)),
    ] {
        for at in [
            Stamp::Float(boundary.next_down()),
            middle.map_or(Stamp::Float(boundary), Stamp::Integer),
            Stamp::Float(boundary.next_up()),
        ] {
            let mut ops = Vec::new();
            feed(&mut ops, 1, 30, 1, 0.0);
            feed(&mut ops, 0, 0, 0, 0.0);
            ops.push(Operation::Flush(at));
            ops.push(Operation::Feed(1, 30, 0, at));
            ops.push(Operation::Feed(0, 0, 0, at));
            flush(&mut ops, at.value() + 0.4);
            case(&mut output, "generic", 1, false, &ops)?;
        }
    }
    for boundary in [0.35_f64, 0.4_f64] {
        for stamp in [boundary.next_down(), boundary, boundary.next_up()] {
            let mut ops = Vec::new();
            feed(&mut ops, 1, 30, 1, -0.1);
            feed(&mut ops, 0, 0, 0, -0.1);
            feed(&mut ops, 1, 30, 0, 0.0);
            feed(&mut ops, 0, 0, 0, 0.0);
            flush(&mut ops, stamp);
            feed(&mut ops, 1, 30, 1, stamp);
            feed(&mut ops, 1, 30, 0, stamp);
            feed(&mut ops, 0, 0, 0, stamp);
            case(&mut output, "generic", 1, false, &ops)?;
        }
    }
    case(&mut output, "yiser-j6", 0, false, recorded)?;
    Ok(output)
}
pub fn generate(root: &Path) -> Result<String> {
    let helper = Pin::open(root.join(HELPER), HELPER_SHA)?;
    let recording = Pin::open(root.join(RECORDING), RECORDING_SHA)?;
    let output = render(&self::recording(&recording.bytes)?)?;
    helper.verify()?;
    recording.verify()?;
    Ok(output)
}
pub fn write(root: &Path, output: &Path) -> Result<()> {
    let helper = Pin::open(root.join(HELPER), HELPER_SHA)?;
    let recording = Pin::open(root.join(RECORDING), RECORDING_SHA)?;
    let text = render(&self::recording(&recording.bytes)?)?;
    helper.verify()?;
    recording.verify()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let owned = file.metadata()?;
    let result = (|| -> Result<()> {
        file.write_all(text.as_bytes())?;
        file.flush()?;
        helper.verify()?;
        recording.verify()?;
        Ok(())
    })();
    if result.is_err()
        && fs::symlink_metadata(output)
            .is_ok_and(|m| (m.dev(), m.ino()) == (owned.dev(), owned.ino()))
    {
        let _ = fs::remove_file(output);
    }
    result
}
