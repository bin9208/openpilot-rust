use openpilot_cereal::log_capnp::manager_state::process_state;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessState {
    pub name: String,
    pub pid: i32,
    pub running: bool,
    pub should_be_running: bool,
    pub exit_code: i32,
}

impl ProcessState {
    pub fn write(&self, mut message: process_state::Builder<'_>) {
        message.set_name(self.name.as_str());
        message.set_pid(self.pid);
        message.set_running(self.running);
        message.set_should_be_running(self.should_be_running);
        message.set_exit_code(self.exit_code);
    }
}
