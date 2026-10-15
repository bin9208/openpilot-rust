//! Final automatic backend process ownership. Process operations are explicit so
//! source sequencing can be compared without granting a fixture GPU ownership.
use crate::backend::{Backend, Request};
use std::time::Duration;
pub type Error = Box<dyn std::error::Error>;

pub trait Owner {
    fn pid(&self) -> u32;
    fn is_alive(&mut self) -> Result<bool, Error>;
    fn terminate(&mut self) -> Result<(), Error>;
    fn join(&mut self, timeout: Duration) -> Result<(), Error>;
    fn kill(&mut self) -> Result<(), Error>;
    fn exit_code(&mut self) -> Result<Option<i32>, Error>;
    fn close(self);
}
pub trait Runtime {
    type Child: Owner;
    fn selection(&mut self) -> Result<Request, Error>;
    fn requested(&mut self) -> Result<bool, Error>;
    fn neural_requested(&mut self) -> Result<bool, Error>;
    fn automatic_backend(&mut self) -> Result<Backend, Error>;
    fn start(&mut self, backend: Backend) -> Result<Self::Child, Error>;
    fn event(&mut self, backend: Backend, pid: u32) -> Result<(), Error>;
    fn now(&mut self) -> f64;
    fn wait(&mut self, duration: Duration);
    fn interrupted(&self) -> bool;
}
fn stop<C: Owner>(child: &mut Option<C>) -> Result<(), Error> {
    if let Some(owner) = child.as_mut() {
        if owner.is_alive()? {
            owner.terminate()?;
        }
        owner.join(Duration::from_secs(2))?;
        if owner.is_alive()? {
            owner.kill()?;
            owner.join(Duration::from_secs(2))?;
        }
        if owner.is_alive()? {
            return Err("signal backend did not stop".into());
        }
        child.take().expect("checked child").close();
    }
    Ok(())
}
pub fn run<R: Runtime>(io: &mut R, duration: Option<Duration>) -> Result<(), Error> {
    let selection = io.selection()?;
    let mut child: Option<R::Child> = None;
    let mut selected = None;
    let start = io.now();
    let outcome = (|| {
        while !io.interrupted()
            && io.requested()?
            && io.neural_requested()?
            && io.selection()? == selection
            && duration.is_none_or(|limit| io.now() - start < limit.as_secs_f64())
        {
            let backend = io.automatic_backend()?;
            if selected != Some(backend) {
                stop(&mut child)?;
                child = Some(io.start(backend)?);
                selected = Some(backend);
                io.event(backend, child.as_ref().expect("started child").pid())?;
            } else if child
                .as_mut()
                .expect("selected child")
                .exit_code()?
                .is_some()
            {
                let name = match backend {
                    Backend::TrackingCpu => "tracking_cpu",
                    Backend::Cpu => "cpu",
                    Backend::Qcom => "qcom",
                };
                let code = child
                    .as_mut()
                    .expect("selected child")
                    .exit_code()?
                    .map_or_else(|| "None".to_owned(), |code| code.to_string());
                return Err(format!("signal backend {name} exited: {code}").into());
            }
            io.wait(Duration::from_millis(100));
        }
        Ok(())
    })();
    // Preserve source finally semantics, including stopping the old owner on a
    // Params failure and retrying cleanup if a transition itself failed.
    stop(&mut child)?;
    outcome
}
