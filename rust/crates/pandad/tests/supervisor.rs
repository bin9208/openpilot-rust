use openpilot_pandad::health::Health;
use openpilot_pandad::supervisor::{Backend, Fault, Log, Panda, Supervisor};

struct Absent;
impl Panda for Absent {
    fn bootstub(&self) -> bool {
        unreachable!()
    }
    fn is_internal(&mut self) -> Result<bool, Fault> {
        unreachable!()
    }
    fn get_type(&mut self) -> Result<Vec<u8>, Fault> {
        unreachable!()
    }
    fn serial(&mut self) -> Result<String, Fault> {
        unreachable!()
    }
    fn version(&mut self) -> Result<String, Fault> {
        unreachable!()
    }
    fn signature(&mut self) -> Result<Vec<u8>, Fault> {
        unreachable!()
    }
    fn flash(&mut self) -> Result<(), Fault> {
        unreachable!()
    }
    fn recover(&mut self, _: bool) -> Result<(), Fault> {
        unreachable!()
    }
    fn health(&mut self) -> Result<Health, Fault> {
        unreachable!()
    }
    fn reset(&mut self) -> Result<(), Fault> {
        unreachable!()
    }
    fn close(&mut self) -> Result<(), Fault> {
        unreachable!()
    }
}

#[derive(Default)]
struct Environment {
    actions: Vec<String>,
    dfu_failure: bool,
}
impl Backend for Environment {
    type Device = Absent;
    fn log(&mut self, _: Log) -> Result<(), Fault> {
        Ok(())
    }
    fn remove_signatures(&mut self) -> Result<(), Fault> {
        self.actions.push("remove".into());
        Ok(())
    }
    fn reset_internal(&mut self) -> Result<(), Fault> {
        self.actions.push("reset".into());
        Ok(())
    }
    fn recover_internal(&mut self) -> Result<(), Fault> {
        self.actions.push("recover".into());
        Ok(())
    }
    fn sleep(&mut self, seconds: u64) -> Result<(), Fault> {
        self.actions.push(format!("sleep{seconds}"));
        Ok(())
    }
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault> {
        self.actions.push("dfu_list".into());
        if self.dfu_failure {
            Err(Fault::NoDevice("gone".into()))
        } else {
            Ok(Vec::new())
        }
    }
    fn dfu_recover(&mut self, _: Option<&str>) -> Result<(), Fault> {
        unreachable!()
    }
    fn panda_list(&mut self) -> Result<Vec<String>, Fault> {
        self.actions.push("list".into());
        Ok(Vec::new())
    }
    fn connect(&mut self, _: &str) -> Result<Absent, Fault> {
        unreachable!()
    }
    fn expected_signature(&mut self, _: &mut Absent) -> Result<Vec<u8>, Fault> {
        unreachable!()
    }
    fn has_internal(&mut self) -> Result<bool, Fault> {
        unreachable!()
    }
    fn put_signatures(&mut self, _: &[u8]) -> Result<(), Fault> {
        unreachable!()
    }
    fn put_bool(&mut self, _: &str) -> Result<(), Fault> {
        unreachable!()
    }
    fn run_child(&mut self, _: &[String]) -> Result<(), Fault> {
        unreachable!()
    }
}

#[test]
fn third_missing_panda_retry_recovers_then_returns_to_reset() {
    let mut supervisor = Supervisor::default();
    let mut env = Environment::default();
    for _ in 0..5 {
        supervisor.step(&mut env).unwrap();
    }
    assert_eq!(
        env.actions,
        [
            "remove", "dfu_list", "list", "remove", "reset", "sleep3", "dfu_list", "list",
            "remove", "reset", "sleep3", "dfu_list", "list", "remove", "recover", "sleep3",
            "dfu_list", "list", "remove", "reset", "sleep3", "dfu_list", "list"
        ]
    );
}

#[test]
fn failed_enumeration_does_not_count_as_missing_panda() {
    let mut supervisor = Supervisor::default();
    let mut env = Environment {
        dfu_failure: true,
        ..Environment::default()
    };
    supervisor.step(&mut env).unwrap();
    env.dfu_failure = false;
    supervisor.step(&mut env).unwrap();
    assert_eq!(
        env.actions,
        ["remove", "dfu_list", "remove", "dfu_list", "list"]
    );
}
