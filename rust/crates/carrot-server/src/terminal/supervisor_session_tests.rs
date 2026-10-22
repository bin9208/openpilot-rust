use super::*;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn issue270_censused_child_that_detaches_is_not_signalled_or_reaped() {
    let leader = rustix::process::getsid(None).unwrap();
    let mut child = ChildGuard(
        Command::new("/bin/sh")
            .args(["-c", "printf ready; read token; exec setsid /bin/sleep 600"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let pid = Pid::from_raw(i32::try_from(child.0.id()).unwrap()).unwrap();
    let mut ready = [0; 5];
    child
        .0
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    assert_eq!(&ready, b"ready");
    let member = members(leader)
        .unwrap()
        .into_iter()
        .find(|member| member.pid == pid)
        .unwrap();
    child
        .0
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"detach\n")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while read_identity(pid).unwrap().unwrap().session == leader.as_raw_nonzero().get() {
        assert!(
            Instant::now() < deadline,
            "fixture failed to create a detached session"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    cleanup_member(leader, member).unwrap();
    assert!(
        child.0.try_wait().unwrap().is_none(),
        "detached child was killed/reaped"
    );
    // Its original Child handle must remain able to terminate and reap it.
    child.0.kill().unwrap();
    assert!(child.0.wait().unwrap().code().is_none());
}

#[test]
fn issue270_stale_starttime_cannot_select_a_live_child() {
    let leader = rustix::process::getsid(None).unwrap();
    let mut child = ChildGuard(Command::new("/bin/sleep").arg("600").spawn().unwrap());
    let pid = Pid::from_raw(i32::try_from(child.0.id()).unwrap()).unwrap();
    let mut member = members(leader)
        .unwrap()
        .into_iter()
        .find(|member| member.pid == pid)
        .unwrap();
    member.starttime = member.starttime.wrapping_add(1);
    cleanup_member(leader, member).unwrap();
    assert!(child.0.try_wait().unwrap().is_none());
}
