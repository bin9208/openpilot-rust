use crate::{Error, ManagedProcess, StopOptions};

pub fn ensure_running(
    processes: &mut [ManagedProcess],
    not_run: &[&str],
    mut should_run: impl FnMut(&str) -> Result<bool, Error>,
) -> Result<Vec<usize>, Error> {
    let mut running = Vec::new();
    for (index, process) in processes.iter_mut().enumerate() {
        if process.has_process() && process.exit_code()?.is_some() {
            process.stop(StopOptions {
                block: false,
                ..StopOptions::default()
            })?;
        }
        if process.policy.enabled
            && !not_run.contains(&process.name())
            && should_run(process.name())?
        {
            if process.policy.restart_if_crash
                && process.has_process()
                && process.exit_code()?.is_some()
            {
                process.report_restart()?;
                process.restart()?;
            }
            running.push(index);
        } else {
            process.stop(StopOptions {
                block: false,
                ..StopOptions::default()
            })?;
        }
    }
    for &index in &running {
        processes[index].start()?;
    }
    Ok(running)
}
