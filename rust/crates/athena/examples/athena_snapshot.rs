use openpilot_athena::{
    snapshot,
    state::{Config, Shared, Stop},
    Error,
};
fn main() -> Result<(), Error> {
    let mut config = Config::for_runtime()?;
    config.pc = false;
    config.process_launcher = std::env::args_os()
        .nth(1)
        .ok_or(Error::Contract("process child path"))?
        .into();
    let shared = Shared::new(config)?;
    let stop = Stop::default();
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.signal_flag())?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.signal_flag())?;
    println!("{}", snapshot::take(&shared, &stop)?);
    Ok(())
}
