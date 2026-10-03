//! Exact display_scheduling.py sweep/order and verified SCHED_OTHER bootstrap.
use std::{collections::BTreeSet, io};
#[expect(
    unsafe_code,
    reason = "CXX scalar scheduler syscall boundary; no pointers or retained memory"
)]
mod bridge;
mod native;
pub use native::Native;
pub trait Platform {
    fn online(&mut self, core: usize) -> bool;
    fn threads(&mut self, pid: Option<i32>) -> io::Result<Vec<i32>>;
    fn policy(&mut self, tid: i32) -> io::Result<i32>;
    fn set_other(&mut self, tid: i32) -> io::Result<()>;
    fn priority(&mut self, tid: i32) -> io::Result<i32>;
    fn set_priority(&mut self, tid: i32, value: i32) -> io::Result<()>;
    fn affinity(&mut self, tid: i32) -> io::Result<BTreeSet<usize>>;
    fn set_affinity(&mut self, tid: i32, cores: &BTreeSet<usize>) -> io::Result<()>;
}
#[derive(Clone, Copy)]
pub struct Request {
    pub onroad: bool,
    pub force: bool,
    pub child_pid: Option<i32>,
    pub now: f64,
}
pub struct Scheduler {
    pub enabled: bool,
    pub core: usize,
    pub onroad: Option<bool>,
    pub next_check: f64,
}
impl Scheduler {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            core: 6,
            onroad: None,
            next_check: 0.0,
        }
    }
    pub fn update(&mut self, request: Request, platform: &mut impl Platform) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if !request.force && Some(request.onroad) == self.onroad && request.now < self.next_check {
            return Ok(());
        }
        self.onroad = Some(request.onroad);
        self.next_check = request.now + 0.5;
        let big = request.onroad && platform.online(self.core);
        let cores = if big {
            BTreeSet::from([self.core])
        } else {
            little()
        };
        let mut workers = platform.threads(None)?;
        if let Some(pid) = request.child_pid {
            workers.extend(platform.threads(Some(pid))?);
        }
        for tid in workers {
            match worker(platform, tid, Placement { cores: &cores, big }) {
                Err(error) if error.raw_os_error() == Some(3) => {}
                result => result?,
            }
        }
        Ok(())
    }
}
fn little() -> BTreeSet<usize> {
    BTreeSet::from([0, 1, 2, 3])
}
struct Placement<'a> {
    cores: &'a BTreeSet<usize>,
    big: bool,
}
fn worker(platform: &mut impl Platform, tid: i32, placement: Placement<'_>) -> io::Result<()> {
    let Placement { cores, big } = placement;
    if platform.policy(tid)? != 0 {
        platform.set_other(tid)?;
    }
    if big && platform.priority(tid)? != 19 {
        platform.set_priority(tid, 19)?;
    }
    if platform.affinity(tid)? != *cores && platform.set_affinity(tid, cores).is_err() {
        platform.set_affinity(tid, &little())?;
    }
    if !big && platform.priority(tid)? != 0 {
        match platform.set_priority(tid, 0) {
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
            result => result?,
        }
    }
    Ok(())
}
pub fn bootstrap(platform: &mut impl Platform, board: bool) -> io::Result<()> {
    if !board {
        return Ok(());
    }
    platform.set_affinity(0, &BTreeSet::from([0]))?;
    for _ in 0..2 {
        if platform.set_other(0).is_ok() && platform.policy(0).is_ok_and(|policy| policy == 0) {
            return Ok(());
        }
    }
    Err(io::Error::other(
        "UI must run SCHED_OTHER (realtime preemption contract)",
    ))
}
