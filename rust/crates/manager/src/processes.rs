use crate::Error;
use openpilot_manager_catalog::{Descriptor, GpsPaths, Parameters, State};
use openpilot_process_supervision::{ManagedProcess, ProcessPolicy, ProcessState, StopOptions};

/// Catalog-order binding. A missing native implementation remains represented in
/// managerState and becomes an error if its predicate requests execution.
pub struct Entry {
    pub descriptor: Descriptor,
    pub process: Option<ManagedProcess>,
}
pub struct Processes {
    pub entries: Vec<Entry>,
}
impl Processes {
    pub fn prepare(&self) {
        for entry in &self.entries {
            if let Some(process) = &entry.process {
                process.prepare();
            }
        }
    }
    pub fn ensure(
        &mut self,
        state: State,
        params: &mut impl Parameters,
        paths: &mut impl GpsPaths,
        ignore: &[String],
    ) -> Result<(), Error> {
        let mut running = Vec::new();
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if let Some(process) = &mut entry.process {
                if process.name() != entry.descriptor.name {
                    return Err(Error::Contract("bound process does not match catalog name"));
                }
                process.policy = ProcessPolicy {
                    enabled: entry.descriptor.enabled,
                    sigkill: entry.descriptor.sigkill,
                    restart_if_crash: entry.descriptor.restart_if_crash,
                };
                if process.has_process() && process.exit_code()?.is_some() {
                    process.stop(StopOptions {
                        block: false,
                        ..StopOptions::default()
                    })?;
                }
            }
            let wanted = entry.descriptor.enabled
                && !ignore.iter().any(|name| name == entry.descriptor.name)
                && entry.descriptor.predicate.evaluate(state, params, paths)?;
            if wanted {
                let process = entry
                    .process
                    .as_mut()
                    .ok_or_else(|| Error::Unavailable(entry.descriptor.name.into()))?;
                if entry.descriptor.restart_if_crash
                    && process.has_process()
                    && process.exit_code()?.is_some()
                {
                    process.restart()?;
                }
                running.push(index);
            } else if let Some(process) = &mut entry.process {
                process.stop(StopOptions {
                    block: false,
                    ..StopOptions::default()
                })?;
            }
        }
        for index in running {
            let process = self.entries[index]
                .process
                .as_mut()
                .ok_or(Error::Contract("selected process missing"))?;
            process.start()?;
        }
        Ok(())
    }
    pub fn stop(&mut self, block: bool) -> Result<(), Error> {
        for entry in &mut self.entries {
            if let Some(process) = &mut entry.process {
                process.stop(StopOptions {
                    block,
                    ..StopOptions::default()
                })?;
            }
        }
        Ok(())
    }
    pub fn states(&mut self) -> Result<Vec<ProcessState>, Error> {
        self.entries
            .iter_mut()
            .map(|entry| match &mut entry.process {
                Some(process) => Ok(process.state()?),
                None => Ok(ProcessState {
                    name: entry.descriptor.name.into(),
                    pid: 0,
                    running: false,
                    should_be_running: false,
                    exit_code: 0,
                }),
            })
            .collect()
    }
}
