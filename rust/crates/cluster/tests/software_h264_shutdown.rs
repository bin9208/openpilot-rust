use openpilot_cluster::{
    h264::{Config, Control, Output, Software},
    usb::H264Chunk,
};
use std::{
    io,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct Recipient(Arc<AtomicBool>);
impl Output for Recipient {
    fn start(&mut self, _: i64) -> io::Result<usize> {
        Ok(64)
    }
    fn send(&mut self, _: H264Chunk<'_>) -> io::Result<()> {
        Ok(())
    }
    fn control(&mut self, _: Control) -> io::Result<()> {
        Ok(())
    }
    fn stop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn inherited_provider_pipes_cannot_hold_stream_cleanup_indefinitely() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("encoder");
    let pid_file = directory.path().join("descendant.pid");
    std::fs::write(&executable, format!(
        "#!/bin/sh\nif [ \"$2\" = '-muxers' ]; then printf ' E h264\\n'; exit; fi\n/bin/sleep 8 &\nprintf '%s' \"$!\" > '{}'\nexit 0\n", pid_file.display()
    )).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let worker_stopped = Arc::clone(&stopped);
    let mut pipeline = Software::start(
        Config {
            width: 32,
            height: 16,
            fps: 10,
            bitrate: "500k".into(),
            gop: 3,
            ffmpeg: executable,
            encoder: "libx264".into(),
            requested_chunk_size: 64,
            wait_for_ack: false,
            soft_ack: false,
            dump: None,
        },
        move || Ok(Recipient(worker_stopped)),
    )
    .unwrap();
    let started = Instant::now();
    pipeline.close().unwrap();
    let elapsed = started.elapsed();
    let pid = std::fs::read_to_string(pid_file)
        .unwrap()
        .parse::<i32>()
        .unwrap();
    if let Some(pid) = rustix::process::Pid::from_raw(pid) {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    }
    assert!(stopped.load(Ordering::SeqCst));
    assert!(
        elapsed < Duration::from_secs(6),
        "stream cleanup took {elapsed:?}"
    );
}
