//! A blocked diagnostic must not hold the real PortAudio callback's playback lock.
#![cfg(target_os = "linux")]
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn library(root: &Path) -> PathBuf {
    let path = root.join("libowned-portaudio.so");
    let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args([
            "-std=c11", "-shared", "-fPIC", "-pthread", "-Wall", "-Wextra", "-Werror",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fixtures/soundd/portaudio.c"))
        .arg("-o")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
}
fn assets(root: &Path) {
    // Every supported filename is present, so asset loading emits no fallback
    // warnings before the particular language-change diagnostic under test.
    let names = [
        "engage",
        "disengage",
        "refuse",
        "prompt",
        "prompt_distracted",
        "warning_soft",
        "warning_immediate",
        "tici_engaged",
        "tici_disengaged",
        "traffic_sign_green",
        "traffic_sign_changed",
        "audio_traffic_error",
        "audio_car_watchout",
        "audio_lane_change",
        "audio_stopstop",
        "audio_stopping",
        "audio_auto_hold",
        "audio_engage",
        "audio_disengage",
        "audio_speed_down",
        "audio_turn",
        "reverse_gear",
        "audio_1",
        "audio_2",
        "audio_3",
        "audio_4",
        "audio_5",
        "audio_6",
        "audio_7",
        "audio_8",
        "audio_9",
        "audio_10",
        "engage_tizi",
        "disengage_tizi",
    ];
    let pcm: Vec<u8> = (0..4800)
        .flat_map(|index| {
            let sample: i16 = if index % 2 == 0 { 1000 } else { -1000 };
            sample.to_le_bytes()
        })
        .collect();
    let size = u32::try_from(pcm.len()).unwrap();
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + size).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16_u32.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(48000_u32.to_le_bytes());
    wav.extend(96000_u32.to_le_bytes());
    wav.extend(2_u16.to_le_bytes());
    wav.extend(16_u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(size.to_le_bytes());
    wav.extend(pcm);
    for language in ["sounds_eng", "sounds"] {
        let directory = root.join(language);
        fs::create_dir_all(&directory).unwrap();
        for name in names {
            fs::write(directory.join(format!("{name}.wav")), &wav).unwrap();
        }
    }
}
fn event_count(path: &Path, name: &str) -> usize {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == name)
        .count()
}
fn drain(reader: &mut File, captured: &mut Vec<u8>) {
    let mut bytes = [0; 8192];
    loop {
        match reader.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => captured.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("stderr read failed: {error}"),
        }
    }
}
#[test]
fn language_diagnostic_backpressure_does_not_stop_audio_callbacks() {
    let root = tempfile::tempdir().unwrap();
    let shm = tempfile::Builder::new()
        .prefix("msgq_sound-lock-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = shm
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let params = root.path().join("params").join(prefix);
    fs::create_dir_all(&params).unwrap();
    for (key, value) in [
        ("SoundVolumeAdjustEngage", "37"),
        ("SoundVolumeAdjust", "73"),
        ("SoundLanguageSetting", "en"),
    ] {
        fs::write(params.join(key), value).unwrap();
    }
    let asset_root = root.path().join("assets");
    assets(&asset_root);
    let events = root.path().join("events.jsonl");
    let (reader, writer) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC).unwrap();
    rustix::fs::fcntl_setfl(&reader, rustix::fs::OFlags::NONBLOCK).unwrap();
    let mut reader = File::from(reader);
    let mut writer = File::from(writer);
    let child = Owned(
        Command::new(env!("CARGO_BIN_EXE_openpilot-soundd"))
            .arg("--assets")
            .arg(asset_root)
            .arg("--portaudio-library")
            .arg(library(root.path()))
            .env_remove("MANAGER_DAEMON")
            .env("OPENPILOT_PREFIX", prefix)
            .env("PARAMS_ROOT", root.path().join("params"))
            .env("SOUND_FIXTURE_EVENTS", &events)
            .env("SOUND_FIXTURE_SAMPLES", root.path().join("samples.f32"))
            .env("LOGPRINT", "info")
            .stdout(Stdio::null())
            .stderr(writer.try_clone().unwrap())
            .spawn()
            .unwrap(),
    );
    let mut captured = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while event_count(&events, "poll") < 4 {
        drain(&mut reader, &mut captured);
        assert!(
            Instant::now() < deadline,
            "sound daemon did not start: {}",
            String::from_utf8_lossy(&captured)
        );
        thread::sleep(Duration::from_millis(5));
    }
    drain(&mut reader, &mut captured);
    rustix::fs::fcntl_setfl(&writer, rustix::fs::OFlags::NONBLOCK).unwrap();
    loop {
        match writer.write(&[b'x'; 4096]) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("stderr fill failed: {error}"),
        }
    }
    rustix::fs::fcntl_setfl(&writer, rustix::fs::OFlags::empty()).unwrap();
    fs::write(params.join("SoundLanguageSetting"), "ko").unwrap();
    let wait_channel = PathBuf::from(format!("/proc/{}/wchan", child.0.id()));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = fs::read_to_string(&wait_channel).unwrap();
        if matches!(state.trim(), "pipe_write" | "anon_pipe_write") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "diagnostic did not block on the full pipe: {state}"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let before = event_count(&events, "callback");
    let deadline = Instant::now() + Duration::from_secs(3);
    while event_count(&events, "callback") < before + 3 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    let after = event_count(&events, "callback");
    // Release the diagnostic and verify that it was the intended language event.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !String::from_utf8_lossy(&captured).contains("soundd language changed: ko")
        && Instant::now() < deadline
    {
        drain(&mut reader, &mut captured);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        String::from_utf8_lossy(&captured).contains("soundd language changed: ko"),
        "wrong diagnostic blocked"
    );
    assert!(
        after >= before + 3,
        "blocked language diagnostic stalled callbacks: {before} -> {after}"
    );
}
