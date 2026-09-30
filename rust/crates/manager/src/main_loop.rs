use crate::{
    initialization::{initialize_main, InitPaths, Startup},
    lifecycle::{self, Environment, Runtime},
    parameters::Parameters,
    Error,
};

/// Entrypoint context. The runtime factory runs after initialization and the
/// PREPAREONLY gate, so prepare-only never opens IPC or installs runtime signals.
pub struct Main<'a, P> {
    pub params: &'a P,
    pub paths: InitPaths<'a>,
    pub environment: Environment,
}
impl<P: Parameters> Main<'_, P> {
    pub fn run<S: Startup, R: Runtime>(
        &self,
        startup: &mut S,
        runtime: impl FnOnce(&mut S) -> Result<R, Error>,
    ) -> Result<(), Error> {
        initialize_main(self.params, startup, &self.paths)?;
        if self.environment.prepare_only {
            return Ok(());
        }
        let mut runtime = runtime(startup)?;
        lifecycle::run(self.params, &mut runtime, &self.environment)
    }
}
