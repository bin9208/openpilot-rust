use super::{exit_code, Generation};
use openpilot_process_supervision::{pty::Pair, CapturedCommand};
use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
use std::{
    io::Read,
    path::PathBuf,
    time::{Duration, Instant},
};

fn launcher() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("openpilot-process-child")
}

fn process_state(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .map(|text| {
            text.rsplit_once(')')
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap()
                .into()
        })
}

#[test]
fn exited_leader_remains_reserved_until_owned_descendant_cleanup() {
    // Given an owned PTY group with a leader and a HUP-resistant descendant.
    rustix::process::set_child_subreaper(Some(Pid::INIT)).unwrap();
    let pair = Pair::open(30, 100).unwrap();
    let child = CapturedCommand {
        launcher: launcher(), cwd: std::env::current_dir().unwrap(),
        argv: ["/bin/sh", "-c", "trap '' HUP; /bin/sh -c 'trap \"\" HUP; exec sleep 600' & printf 'OWNED_CHILD=%s\\n' \"$!\"; exit 23"].map(Into::into).into(),
    }.spawn_session_pty_with_env(pair.slave, &[]).unwrap();
    let leader = child.process.id();
    let pid = Pid::from_raw(i32::try_from(leader).unwrap()).unwrap();
    let mut generation = Generation {
        master: pair.master,
        child,
        pid,
    };
    let observed = waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOWAIT,
    )
    .unwrap()
    .unwrap();
    assert_eq!(observed.exit_status(), Some(23));
    let mut bytes = [0; 128];
    let count = generation.master.read(&mut bytes).unwrap();
    let output = String::from_utf8_lossy(&bytes[..count]);
    let descendant: u32 = output
        .split("OWNED_CHILD=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let status = generation.observe().unwrap().unwrap();
    assert_eq!(exit_code(status), 23);
    let before = process_state(leader);
    assert_eq!(before.as_deref(), Some("Z"));
    let mut unrelated = std::process::Command::new("/bin/sleep")
        .arg("600")
        .spawn()
        .unwrap();
    // When the generation closes its owned group before reaping the leader.
    drop(generation);
    let deadline = Instant::now() + Duration::from_secs(2);
    let descendant_after = loop {
        let state = process_state(descendant);
        if state.as_deref().is_none_or(|state| state == "Z") {
            break state;
        }
        assert!(
            Instant::now() < deadline,
            "owned descendant {descendant} still live: {state:?}"
        );
        std::thread::yield_now();
    };
    // Then the leader is reaped and the descendant cannot remain alive.
    assert_eq!(process_state(leader), None);
    assert_eq!(descendant_after, None);
    assert!(unrelated.try_wait().unwrap().is_none());
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
    println!("{{\"scenario\":\"leader-reservation-before-group-cleanup\",\"leader\":{leader},\"descendant\":{descendant},\"unreaped_leader_state\":\"Z\",\"reserved_exit_status\":23,\"leader_reaped\":true,\"descendant_after\":{}}}", descendant_after.as_deref().map_or_else(|| "null".into(), |state| format!("\"{state}\"")));
}
