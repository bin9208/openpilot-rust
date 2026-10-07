use openpilot_ui_application::scheduling::{self, Platform, Request, Scheduler};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
#[derive(Clone, Deserialize, Serialize)]
struct Worker {
    policy: i32,
    nice: i32,
    cores: BTreeSet<usize>,
}
#[derive(Deserialize)]
struct Step {
    now: f64,
    onroad: bool,
    force: bool,
    online: bool,
    child: bool,
    #[serde(default)]
    gone: Option<i32>,
    #[serde(default)]
    fail_affinity: Option<i32>,
    #[serde(default)]
    deny_restore: bool,
}
#[derive(Deserialize)]
struct Scene {
    enabled: bool,
    workers: BTreeMap<i32, Worker>,
    steps: Vec<Step>,
}
struct Mock {
    workers: BTreeMap<i32, Worker>,
    calls: Vec<serde_json::Value>,
    online: bool,
    gone: Option<i32>,
    fail: Option<i32>,
    deny: bool,
}
impl Mock {
    fn worker(&mut self, tid: i32) -> io::Result<&mut Worker> {
        if self.gone == Some(tid) {
            return Err(io::Error::from_raw_os_error(3));
        }
        self.workers
            .get_mut(&tid)
            .ok_or_else(|| io::Error::from_raw_os_error(3))
    }
}
impl Platform for Mock {
    fn online(&mut self, core: usize) -> bool {
        self.calls.push(serde_json::json!(["online", core]));
        self.online
    }
    fn threads(&mut self, pid: Option<i32>) -> io::Result<Vec<i32>> {
        self.calls.push(serde_json::json!(["threads", pid]));
        Ok(if pid.is_some() { vec![3] } else { vec![1, 2] })
    }
    fn policy(&mut self, tid: i32) -> io::Result<i32> {
        self.calls.push(serde_json::json!(["policy", tid]));
        Ok(self.worker(tid)?.policy)
    }
    fn set_other(&mut self, tid: i32) -> io::Result<()> {
        self.calls.push(serde_json::json!(["other", tid]));
        self.worker(tid)?.policy = 0;
        Ok(())
    }
    fn priority(&mut self, tid: i32) -> io::Result<i32> {
        self.calls.push(serde_json::json!(["priority", tid]));
        Ok(self.worker(tid)?.nice)
    }
    fn set_priority(&mut self, tid: i32, value: i32) -> io::Result<()> {
        self.calls.push(serde_json::json!(["nice", tid, value]));
        if self.deny && value == 0 {
            return Err(io::Error::from_raw_os_error(13));
        }
        self.worker(tid)?.nice = value;
        Ok(())
    }
    fn affinity(&mut self, tid: i32) -> io::Result<BTreeSet<usize>> {
        self.calls.push(serde_json::json!(["affinity", tid]));
        Ok(self.worker(tid)?.cores.clone())
    }
    fn set_affinity(&mut self, tid: i32, cores: &BTreeSet<usize>) -> io::Result<()> {
        self.calls
            .push(serde_json::json!(["set_affinity", tid, cores]));
        if self.fail == Some(tid) {
            self.fail = None;
            return Err(io::Error::from_raw_os_error(22));
        }
        self.worker(tid)?.cores = cores.clone();
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene: Scene = serde_json::from_reader(std::io::stdin())?;
    let mut scheduler = Scheduler::new(scene.enabled);
    let mut mock = Mock {
        workers: scene.workers,
        calls: Vec::new(),
        online: false,
        gone: None,
        fail: None,
        deny: false,
    };
    let mut output = Vec::new();
    for step in scene.steps {
        mock.online = step.online;
        mock.gone = step.gone;
        mock.fail = step.fail_affinity;
        mock.deny = step.deny_restore;
        scheduler.update(
            Request {
                now: step.now,
                onroad: step.onroad,
                force: step.force,
                child_pid: step.child.then_some(123),
            },
            &mut mock,
        )?;
        output.push(serde_json::json!({"onroad":scheduler.onroad,"next_check":scheduler.next_check,"workers":mock.workers,"calls":mock.calls}));
        mock.calls.clear();
    }
    // These harmless syscall calls exercise the native CXX policy boundary in this owned process.
    let mut native = scheduling::Native { root: "/".into() };
    native.set_other(0)?;
    let policy = native.policy(0)?;
    if policy != 0 {
        return Err("native SCHED_OTHER readback failed".into());
    }
    eprintln!("PASS native owned process SCHED_OTHER readback={policy}");
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
