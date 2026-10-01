use crate::{
    logging, runtime,
    state::{Config, Shared, Stop},
    supervisor, Error,
};
use openpilot_crash_reporting::{sdk::NativeSdk, Project, Reporter, RuntimeInputs};
use openpilot_managed_entry::StepError;
use std::{process::ExitCode, sync::Arc};

pub fn main(supervise: bool) -> ExitCode {
    match run(supervise) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("athena: {error}");
            ExitCode::FAILURE
        }
    }
}
fn run(supervise: bool) -> Result<(), Error> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("Native Athena RPC/transfers and reconnect runtime; use openpilot-manage-athenad for restart supervision. Paths follow Params/Paths and OPENPILOT_BASEDIR (default current directory).");
        return Ok(());
    }
    if !args.is_empty() {
        return Err(Error::Contract("unexpected argument"));
    }
    let stop = Stop::default();
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.signal_flag())?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.signal_flag())?;
    let shared = Arc::new(Shared::new(Config::for_runtime()?)?);
    if supervise {
        return supervisor::run(&shared, &stop);
    }
    logging::inherit(&shared)?;
    shared.factory.bind_global(
        [(
            "daemon".into(),
            openpilot_logging::Value::Text("athenad".into()),
        )]
        .into_iter()
        .collect(),
    )?;
    let mut reporter = Reporter {
        sdk: NativeSdk::default(),
        inputs: RuntimeInputs::new(shared.config.basedir.clone()),
        logger: shared.factory.logger(),
    };
    reporter.init(Project::Selfdrive)?;
    openpilot_managed_entry::launch(
        &mut reporter,
        "openpilot.system.athena.athenad",
        "athenad",
        || Ok(()),
        |()| Ok(shared),
        |shared, _| runtime::run(shared, &stop).map_err(StepError::raised),
    )?;
    Ok(())
}
