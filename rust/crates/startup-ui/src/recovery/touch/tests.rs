use super::*;
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
