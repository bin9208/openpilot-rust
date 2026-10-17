use openpilot_cereal::log_capnp::manager_state;
use openpilot_ui_application::services::updater::{bind, Binding, Strategy, Unavailable};
use std::{
    fs,
    process::{Child, Command},
};

struct Live(Child);
impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn configured_executable_name_and_alias_are_accepted_but_wrong_process_is_not() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("renamed-updater");
    fs::copy("/bin/sleep", &executable).unwrap();
    let live = Live(Command::new(&executable).arg("30").spawn().unwrap());
    let alias = root.path().join("updater-alias");
    std::os::unix::fs::symlink(&executable, &alias).unwrap();
    let mut message = capnp::message::Builder::new_default();
    let mut manager = message.init_root::<manager_state::Builder<'_>>();
    {
        let mut rows = manager.reborrow().init_processes(1);
        let mut row = rows.reborrow().get(0);
        row.set_name("updated");
        row.set_running(true);
        row.set_pid(live.0.id().try_into().unwrap());
    }
    for expected in [&executable, &alias] {
        assert!(matches!(
            bind(
                manager.reborrow().into_reader(),
                expected,
                Strategy::RecheckedPid
            )
            .unwrap(),
            Binding::Ready(_)
        ));
    }
    assert!(matches!(
        bind(
            manager.into_reader(),
            std::path::Path::new("/bin/sleep"),
            Strategy::RecheckedPid
        )
        .unwrap(),
        Binding::Unavailable(Unavailable::IdentityChanged)
    ));
}
