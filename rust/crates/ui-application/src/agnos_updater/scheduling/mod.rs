use std::{collections::BTreeSet, io};
#[expect(unsafe_code, reason = "audited CXX scalar scheduler syscall boundary")]
mod bridge;
pub trait Platform {
    fn fifo(&mut self, priority: i32) -> io::Result<()>;
    fn affinity(&mut self, cores: &[usize]) -> io::Result<()>;
    fn available(&mut self) -> io::Result<BTreeSet<usize>>;
    fn warning(&mut self, message: String);
}
pub fn configure(platform: &mut impl Platform, pc: bool, tici: bool) -> io::Result<()> {
    if !pc {
        platform.fifo(51)?;
        platform.affinity(&[0])?;
    }
    if tici && platform.affinity(&[5]).is_err() {
        let cores = platform.available()?;
        platform.warning(format!(
            "Skipping core 5 affinity for updater process, available cores: {:?}",
            cores.into_iter().collect::<Vec<_>>()
        ));
    }
    Ok(())
}
pub struct Native;
impl Platform for Native {
    fn fifo(&mut self, priority: i32) -> io::Result<()> {
        let result = bridge::ffi::set_fifo(priority);
        if result.error == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(result.error))
        }
    }
    fn affinity(&mut self, cores: &[usize]) -> io::Result<()> {
        let mut set = rustix::thread::CpuSet::new();
        for &core in cores {
            set.set(core);
        }
        Ok(rustix::thread::sched_setaffinity(None, &set)?)
    }
    fn available(&mut self) -> io::Result<BTreeSet<usize>> {
        let set = rustix::thread::sched_getaffinity(None)?;
        Ok((0..rustix::thread::CpuSet::MAX_CPU)
            .filter(|core| set.is_set(*core))
            .collect())
    }
    fn warning(&mut self, message: String) {
        openpilot_startup_ui::logging::emit(openpilot_logging::record::Level::Warning, message);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        calls: Vec<String>,
        fail: bool,
    }
    impl Platform for Fake {
        fn fifo(&mut self, p: i32) -> io::Result<()> {
            self.calls.push(format!("fifo:{p}"));
            Ok(())
        }
        fn affinity(&mut self, c: &[usize]) -> io::Result<()> {
            self.calls.push(format!("affinity:{c:?}"));
            if self.fail && c == [5] {
                Err(io::Error::from_raw_os_error(22))
            } else {
                Ok(())
            }
        }
        fn available(&mut self) -> io::Result<BTreeSet<usize>> {
            self.calls.push("available".into());
            Ok([0, 1].into())
        }
        fn warning(&mut self, m: String) {
            self.calls.push(m);
        }
    }
    #[test]
    fn source_order_and_board_fallback() {
        let mut p = Fake {
            calls: vec![],
            fail: true,
        };
        configure(&mut p, false, true).unwrap();
        assert_eq!(
            p.calls,
            vec![
                "fifo:51",
                "affinity:[0]",
                "affinity:[5]",
                "available",
                "Skipping core 5 affinity for updater process, available cores: [0, 1]"
            ]
        );
        p.calls.clear();
        configure(&mut p, true, false).unwrap();
        assert!(p.calls.is_empty());
    }
}
