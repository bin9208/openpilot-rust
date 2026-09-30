//! Small Linux scheduler/signal boundary. No hardware ownership lives here.
use std::io;
pub struct SignalMask(libc::sigset_t, std::marker::PhantomData<std::rc::Rc<()>>);
impl SignalMask {
    pub fn block_all() -> io::Result<Self> {
        // SAFETY: FFI layout and initialization: Linux sigset_t is an integer bitset;
        // both writable values are owned on this stack and borrowed only for each call.
        let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
        // SAFETY: same sigset_t bitset invariant; pthread_sigmask initializes old on success.
        let mut old: libc::sigset_t = unsafe { std::mem::zeroed() };
        // SAFETY: sigfillset receives a valid exclusive sigset_t pointer for the duration of the call.
        if unsafe { libc::sigfillset(&mut set) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: initialized set and exclusive old are distinct valid stack allocations.
        let result = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old) };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        Ok(Self(old, std::marker::PhantomData))
    }
}
impl Drop for SignalMask {
    fn drop(&mut self) {
        // SAFETY: self.0 was returned by pthread_sigmask and remains initialized;
        // the optional oldset is null because no returned mask is requested.
        let result =
            unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut()) };
        if result != 0 {
            eprintln!(
                "jetlink signal-mask restore: {}",
                io::Error::from_raw_os_error(result)
            );
        }
    }
}
pub fn scheduler(priority: Option<i32>) -> io::Result<()> {
    // SAFETY: libc sched_param contains integer/time fields; zero initializes all
    // Linux GNU/musl fields. The kernel only reads it during sched_setscheduler.
    let mut parameter: libc::sched_param = unsafe { std::mem::zeroed() };
    parameter.sched_priority = priority.unwrap_or(0);
    // SAFETY: initialized parameter is borrowed during this call; pid 0 selects this thread.
    if unsafe {
        libc::sched_setscheduler(
            0,
            if priority.is_some() {
                libc::SCHED_FIFO
            } else {
                libc::SCHED_OTHER
            },
            &parameter,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub fn background() {
    if let Err(error) = scheduler(None) {
        eprintln!("jetlink background scheduler: {error}");
    }
    if let Err(error) = widen_affinity() {
        eprintln!("jetlink worker affinity: {error}");
    }
}
fn widen_affinity() -> io::Result<()> {
    let inherited = rustix::thread::sched_getaffinity(None)?;
    let mut all = rustix::thread::CpuSet::new();
    let mut count = 0;
    for cpu in 0..1024 {
        if inherited.is_set(cpu) {
            count += 1;
        }
    }
    for entry in std::fs::read_dir("/sys/devices/system/cpu")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(cpu) = name
            .strip_prefix("cpu")
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|cpu| *cpu < 1024)
        {
            if count != 1 || !inherited.is_set(cpu) {
                all.set(cpu);
            }
        }
    }
    if (0..1024).any(|cpu| all.is_set(cpu)) {
        rustix::thread::sched_setaffinity(None, &all)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn signal_mask_restores_and_background_scheduler_runs_on_its_thread() {
        // Given a dedicated native thread, when masking and restoring signals and
        // selecting SCHED_OTHER, then every real syscall succeeds without altering the test runner.
        std::thread::spawn(|| {
            let mask = super::SignalMask::block_all().unwrap();
            drop(mask);
            super::scheduler(None).unwrap();
        })
        .join()
        .unwrap();
    }
}
