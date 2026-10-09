use super::{wait, OwnedChild};
use crate::model_delivery::Error;
use rustix::process::{getpid, set_child_subreaper, waitpid, Pid, WaitOptions};
use std::{
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[test]
fn timeout_reaps_runner_and_terminates_noncooperative_worker() -> std::io::Result<()> {
    // Given an owned runner and worker that both ignore TERM.
    set_child_subreaper(Some(getpid()))?;
    let mut command = Command::new("/bin/sh");
    command
        .process_group(0)
        .arg("-c")
        .arg("trap '' TERM; /bin/sleep 60 & echo $!; wait")
        .stdout(Stdio::piped());
    let mut child = OwnedChild {
        child: command.spawn()?,
        stopped: false,
    };
    let runner = child.child.id();
    let mut line = String::new();
    BufReader::new(
        child
            .child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("owned runner output unavailable"))?,
    )
    .read_line(&mut line)?;
    let raw = line.trim().parse::<i32>().map_err(std::io::Error::other)?;
    let worker =
        Pid::from_raw(raw).ok_or_else(|| std::io::Error::other("owned worker PID unavailable"))?;
    let before = std::fs::read_to_string(format!("/proc/{raw}/stat"))?;
    let start_time = before
        .rsplit_once(") ")
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .ok_or_else(|| std::io::Error::other("owned worker identity unavailable"))?;

    // When the production wait helper expires with an accelerated test-only bound.
    let started = Instant::now();
    let result = wait(
        &mut child,
        &AtomicBool::new(false),
        Duration::from_millis(50),
    );

    // Then its group is killed and both exact children can be reaped.
    assert!(
        matches!(&result, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    assert!(child.child.try_wait()?.is_some());
    let reaped = loop {
        if let Some((pid, status)) = waitpid(Some(worker), WaitOptions::NOHANG)? {
            assert_eq!(pid, worker);
            break status.terminating_signal().is_some();
        }
        assert!(started.elapsed() < Duration::from_secs(4));
        std::thread::sleep(Duration::from_millis(5));
    };
    set_child_subreaper(None)?;
    assert!(reaped);
    assert!(!std::path::Path::new(&format!("/proc/{raw}")).exists());
    println!(
        "{}",
        serde_json::json!({"runner":runner,"worker":raw,"worker_start_time":start_time,
            "runner_reaped":true,"worker_reaped":reaped,"seconds":started.elapsed().as_secs_f64(),
            "helper_limit_ms":50,"production_limit_seconds":300})
    );
    Ok(())
}

#[test]
fn normal_exit_keeps_group_leader_unreaped_until_final_signal() -> std::io::Result<()> {
    // Given a normal owned runner whose numeric PID also names its group.
    let mut child = OwnedChild {
        child: Command::new("/bin/sh")
            .process_group(0)
            .args(["-c", "exit 0"])
            .spawn()?,
        stopped: false,
    };
    let pid = child.child.id();
    let path = format!("/proc/{pid}/stat");
    let started = Instant::now();

    // When the production observer detects completion without reaping it.
    let status = loop {
        if let Some(status) = child.observe()? {
            break status;
        }
        assert!(started.elapsed() < Duration::from_secs(2));
        std::thread::yield_now();
    };
    let fields = std::fs::read_to_string(&path)?;
    let state = fields
        .rsplit_once(") ")
        .and_then(|(_, fields)| fields.split_whitespace().next());

    // Then the leader remains a zombie until stop sends its final group signal/reaps.
    assert_eq!(status.exit_status(), Some(0));
    assert_eq!(state, Some("Z"));
    child.stop()?;
    assert!(!std::path::Path::new(&path).exists());
    println!(
        "{}",
        serde_json::json!({"runner":pid,"unreaped_state":state,"exit":0,
            "runner_reaped_after_stop":true,"observer":"waitid WNOWAIT"})
    );
    Ok(())
}
