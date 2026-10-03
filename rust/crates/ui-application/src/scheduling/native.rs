use super::{bridge::ffi, Platform};
use rustix::{
    process::{getpriority_process, setpriority_process, Pid},
    thread::{sched_getaffinity, sched_setaffinity, CpuSet},
};
use std::{collections::BTreeSet, io, path::PathBuf};
pub struct Native {
    pub root: PathBuf,
}
fn result(value: ffi::SchedulerResult) -> io::Result<i32> {
    if value.error != 0 {
        Err(io::Error::from_raw_os_error(value.error))
    } else {
        Ok(value.value)
    }
}
fn pid(tid: i32) -> io::Result<Option<Pid>> {
    if tid == 0 {
        Ok(None)
    } else {
        Pid::from_raw(tid)
            .map(Some)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid thread id"))
    }
}
impl Platform for Native {
    fn online(&mut self, core: usize) -> bool {
        std::fs::read_to_string(
            self.root
                .join(format!("sys/devices/system/cpu/cpu{core}/online")),
        )
        .is_ok_and(|value| value.trim() == "1")
    }
    fn threads(&mut self, pid: Option<i32>) -> io::Result<Vec<i32>> {
        let directory = self.root.join(format!(
            "proc/{}/task",
            pid.map_or_else(|| "self".into(), |value| value.to_string())
        ));
        match std::fs::read_dir(directory) {
            Ok(entries) => entries
                .map(|entry| {
                    entry?
                        .file_name()
                        .to_string_lossy()
                        .parse::<i32>()
                        .map_err(io::Error::other)
                })
                .collect(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }
    fn policy(&mut self, tid: i32) -> io::Result<i32> {
        result(ffi::get_policy(tid))
    }
    fn set_other(&mut self, tid: i32) -> io::Result<()> {
        result(ffi::set_other(tid)).map(|_| ())
    }
    fn priority(&mut self, tid: i32) -> io::Result<i32> {
        Ok(getpriority_process(pid(tid)?)?)
    }
    fn set_priority(&mut self, tid: i32, value: i32) -> io::Result<()> {
        Ok(setpriority_process(pid(tid)?, value)?)
    }
    fn affinity(&mut self, tid: i32) -> io::Result<BTreeSet<usize>> {
        let set = sched_getaffinity(pid(tid)?)?;
        Ok((0..CpuSet::MAX_CPU)
            .filter(|index| set.is_set(*index))
            .collect())
    }
    fn set_affinity(&mut self, tid: i32, cores: &BTreeSet<usize>) -> io::Result<()> {
        let mut set = CpuSet::new();
        for &core in cores {
            if core >= CpuSet::MAX_CPU {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "CPU out of range",
                ));
            }
            set.set(core);
        }
        Ok(sched_setaffinity(pid(tid)?, &set)?)
    }
}
