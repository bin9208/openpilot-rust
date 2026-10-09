use super::exit_status;
use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
use std::process::Command;

#[test]
fn non_reaping_normal_and_signal_status_mapping() {
    for (script, code) in [("exit 7", 7), ("kill -TERM $$", -15)] {
        let mut child = Command::new("/bin/sh")
            .args(["-c", script])
            .spawn()
            .unwrap();
        let pid = Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
        let observed = waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .unwrap();
        assert_eq!(exit_status(observed), code);
        let retained = waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .unwrap();
        assert_eq!(exit_status(retained), code);
        use std::os::unix::process::ExitStatusExt;
        let reaped = child.wait().unwrap();
        assert_eq!(
            reaped.code().unwrap_or_else(|| -reaped.signal().unwrap()),
            code
        );
        println!(
            "{}",
            serde_json::json!({"pid":child.id(),"mapped_status":code,"leader_retained_before_final_wait":true})
        );
    }
}
