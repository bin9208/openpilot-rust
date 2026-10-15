use super::*;
use serde_json::json;
#[test]
fn source_slot_zero_press_history_and_fallback() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/startup-recovery-touch-final.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        let mut tracker = Tracker::default();
        let mut open = row["opened"].as_bool().unwrap();
        let mut closed = 0;
        for step in row["steps"].as_array().unwrap() {
            let pressed = if !open {
                step["mouse"].as_bool().unwrap()
            } else {
                match step["kind"].as_str().unwrap() {
                    "again" => false,
                    "error" => {
                        open = false;
                        closed += 1;
                        false
                    }
                    "bytes" => {
                        let bytes: Vec<_> = step["data"]
                            .as_str()
                            .unwrap()
                            .as_bytes()
                            .chunks_exact(2)
                            .map(|b| {
                                u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap()
                            })
                            .collect();
                        tracker.read(&bytes)
                    }
                    _ => unreachable!(),
                }
            };
            assert_eq!(pressed, step["pressed"].as_bool().unwrap());
            assert_eq!(
                json!([
                    open,
                    tracker.slot,
                    tracker.down,
                    tracker.previous,
                    tracker.saw_mt
                ]),
                step["state"]
            );
            assert_eq!(json!(closed), step["closed"]);
        }
    }
}
#[test]
fn actual_nonblocking_pipe_and_read_failure_fallback() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("touch");
    let mut missing = Touch::open(&path);
    assert!(!missing.raw());
    assert!(missing.pressed(true));
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let mut touch = Touch::open(&path);
    assert!(touch.raw());
    let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
    assert!(!touch.pressed(true));
    assert!(touch.raw());
    let mut bytes = Vec::new();
    for (kind, code, value) in [(1_u16, 0x14a_u16, 1_i32), (0, 0, 0)] {
        bytes.extend([0; 16]);
        bytes.extend(kind.to_ne_bytes());
        bytes.extend(code.to_ne_bytes());
        bytes.extend(value.to_ne_bytes());
    }
    writer.write_all(&bytes).unwrap();
    assert!(touch.pressed(false));
    assert!(!touch.pressed(true));
    let mut directory = Touch::open(root.path());
    assert!(directory.raw());
    assert!(!directory.pressed(true));
    assert!(!directory.raw());
    assert!(directory.pressed(true));
}
