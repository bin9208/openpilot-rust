//! Native replacement for the original marker-before-GPS-sleep regression.
use openpilot_dev_tools::timed_shutdown_phase::wait_original_gps_sleep;
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::{CommandExt, ExitStatusExt},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn source_signal_waits_for_sleep_after_an_earlier_ready_marker() {
    let temporary = tempfile::tempdir().unwrap();
    let marker = temporary.path().join("sleep-readiness.json");
    // Immutable executables: parent never opens an executable for writing.
    // Blocking read precedes exec(sleep), so marker readiness cannot stand in
    // for the real hrtimer_nanosleep phase, matching the original regression.
    let mut child = Owned(
        Command::new("/bin/sh")
            .args([
                "-c",
                "printf 'command-recorded\\n'; read -r release; exec /bin/sleep 10",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    let mut input = child.0.stdin.take().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), "command-recorded");
    let released = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&released);
    let observed = marker.clone();
    let release = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !observed.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(observed.exists());
        flag.store(true, Ordering::SeqCst);
        input.write_all(b"g\n").unwrap();
        input.flush().unwrap();
    });
    let phase = wait_original_gps_sleep(&mut child.0, temporary.path()).unwrap();
    release.join().unwrap();
    assert!(released.load(Ordering::SeqCst));
    assert_eq!(phase["wchan"], "hrtimer_nanosleep");
    let observations: Value = serde_json::from_slice(&fs::read(marker).unwrap()).unwrap();
    assert_ne!(observations[0]["state"]["wchan"], "hrtimer_nanosleep");
    assert_eq!(
        observations.as_array().unwrap().last().unwrap()["state"]["wchan"],
        "hrtimer_nanosleep"
    );
    let started = Instant::now();
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.0.id()).unwrap()).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::INT).unwrap();
    assert_eq!(child.0.wait().unwrap().signal(), Some(2));
    assert!(started.elapsed() < Duration::from_secs(2));
}
