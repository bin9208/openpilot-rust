use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::os::unix::process::ExitStatusExt;
#[derive(Deserialize)]
struct Audio {
    rate: u32,
    channels: u16,
    sample_count: usize,
    sha256: String,
}
#[derive(Deserialize)]
struct Setting {
    setting: Option<String>,
    volume: Option<f64>,
    error: Option<String>,
    calls: Vec<Audio>,
}
#[derive(Deserialize)]
struct Wave {
    wav_hex: String,
    expected: Audio,
}
#[derive(Deserialize)]
struct Fixture {
    prompt_sha256: String,
    settings: Vec<Setting>,
    waves: Vec<Wave>,
}
fn equal(pcm: sound::Pcm, wanted: &Audio) {
    assert_eq!(pcm.rate, wanted.rate);
    assert_eq!(pcm.channels, wanted.channels);
    assert_eq!(pcm.samples.len(), wanted.sample_count);
    let bytes: Vec<_> = pcm.samples.iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(format!("{:x}", Sha256::digest(bytes)), wanted.sha256);
}
#[test]
fn final_source_prompt_pcm_and_volume_exact() {
    let f: Fixture =
        serde_json::from_str(include_str!("../../../../fixtures/reboot-sound-final.json")).unwrap();
    let asset = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../openpilot/selfdrive/assets/sounds_eng/prompt.wav");
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&asset).unwrap())),
        f.prompt_sha256
    );
    for row in f.settings {
        let v = sound::volume(row.setting.as_deref());
        if row.error.is_some() {
            assert!(v.is_err());
            continue;
        }
        let v = v.unwrap();
        assert_eq!(v, row.volume.unwrap());
        if v == 0.0 {
            assert!(row.calls.is_empty());
        } else {
            equal(sound::load(&asset, v).unwrap(), &row.calls[0]);
        }
    }
    let root = tempfile::tempdir().unwrap();
    for row in f.waves {
        let data: Vec<_> = row
            .wav_hex
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect();
        let path = root.path().join("owned.wav");
        fs::write(&path, data).unwrap();
        equal(sound::load(&path, 0.165).unwrap(), &row.expected);
    }
}
struct Mock {
    device: bool,
    sound_error: u8,
    reboot_error: bool,
    calls: Vec<(&'static str, Duration)>,
    notices: Vec<String>,
}
impl Commands for Mock {
    fn device_only(&mut self) -> io::Result<()> {
        if self.device {
            Ok(())
        } else {
            Err(io::Error::other(
                "Device reboot is disabled on this computer.",
            ))
        }
    }
    fn sound(&mut self, t: Duration) -> Result<ExitStatus, CaptureError> {
        self.calls.push(("sound", t));
        match self.sound_error {
            1 => Err(CaptureError::Io(io::Error::from(io::ErrorKind::NotFound))),
            2 => Err(CaptureError::Timeout {
                timeout: t,
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
            3 => Ok(ExitStatus::from_raw(256)),
            _ => Ok(ExitStatus::from_raw(0)),
        }
    }
    fn reboot(&mut self, t: Duration) -> Result<ExitStatus, CaptureError> {
        self.calls.push(("reboot", t));
        Ok(ExitStatus::from_raw(if self.reboot_error {
            256
        } else {
            0
        }))
    }
    fn notice(&mut self, s: &str) {
        self.notices.push(s.into());
    }
}
#[test]
fn source_bounded_sound_then_reboot_even_after_sound_failure() {
    for device in [false, true] {
        for error in 0..4 {
            for failure in [false, true] {
                let mut m = Mock {
                    device,
                    sound_error: error,
                    reboot_error: failure,
                    calls: Vec::new(),
                    notices: Vec::new(),
                };
                assert_eq!(run(&mut m).is_ok(), device && !failure);
                if device {
                    assert_eq!(
                        m.calls,
                        vec![
                            ("sound", Duration::from_secs(4)),
                            ("reboot", Duration::from_secs(15))
                        ]
                    );
                    assert_eq!(m.notices.len(), usize::from(error != 0));
                } else {
                    assert!(m.calls.is_empty());
                }
            }
        }
    }
}
#[test]
fn source_delay_clamp() {
    for (value, expected) in [
        (f64::NAN, 5.0),
        (f64::INFINITY, 5.0),
        (f64::NEG_INFINITY, 0.0),
        (-1.0, 0.0),
        (0.0, 0.0),
        (0.25, 0.25),
        (4.9, 4.9),
        (5.0, 5.0),
        (9.0, 5.0),
    ] {
        assert_eq!(delay(value).as_secs_f64(), expected);
    }
}

#[test]
fn actual_owned_child_deadlines_guards_and_session() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let trace = root.path().join("calls");
    let helper = root.path().join("chime");
    let sudo = root.path().join("owned-sudo");
    let write = |path: &Path, text: &str| {
        fs::write(path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    };
    let quoted = format!("'{}'", trace.display().to_string().replace('\'', "'\\''"));
    write(
        &sudo,
        &format!("#!/bin/sh\nprintf 'sudo:%s %s\\n' \"$1\" \"$2\" >> {quoted}\n"),
    );
    let launcher = env::var_os("OPENPILOT_REBOOT_TEST_LAUNCHER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("openpilot-process-child")
        });
    let config = Config {
        system: root.path().into(),
        repository: root.path().into(),
        helper: helper.clone(),
        launcher,
        sudo,
        params: root.path().join("params"),
    };
    assert!(reboot(&config).is_err());
    assert!(!trace.exists());
    fs::write(root.path().join("TICI"), "").unwrap();
    // Missing sound helper is still followed by the exact noninteractive reboot argv.
    reboot(&config).unwrap();
    assert_eq!(fs::read_to_string(&trace).unwrap(), "sudo:-n reboot\n");
    fs::remove_file(&trace).unwrap();
    write(
        &helper,
        &format!("#!/bin/sh\nprintf 'sound\\n' >> {quoted}\nexit 7\n"),
    );
    reboot(&config).unwrap();
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        "sound\nsudo:-n reboot\n"
    );
    fs::remove_file(&trace).unwrap();
    write(
        &helper,
        &format!("#!/bin/sh\nprintf 'sound\\n' >> {quoted}\nexec sleep 30\n"),
    );
    let started = std::time::Instant::now();
    reboot(&config).unwrap();
    assert!(started.elapsed() < Duration::from_secs(6));
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        "sound\nsudo:-n reboot\n"
    );
    let mut child = spawn(&config, 1.0).unwrap();
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.process.id()).unwrap()).unwrap();
    assert_eq!(rustix::process::getsid(Some(pid)).unwrap(), pid);
    child.process.kill().unwrap();
    child.process.wait().unwrap();
}
