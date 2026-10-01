use openpilot_controlsd::{runtime, Error};
use std::{env, path::PathBuf, process::ExitCode};
fn run() -> Result<(), Error> {
    let mut frames = None;
    let mut assets = PathBuf::from("opendbc/car/torque_data");
    let mut numerics = env::var_os("CONTROLS_NUMERICS")
        .map(PathBuf::from)
        .unwrap_or(env::current_exe()?.with_file_name("controlsd-numerics"));
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" => {
                println!("openpilot-controlsd [--numerics DIRECTORY] [--assets DIRECTORY] [--frames N]\nNative control loop. Requires pinned OpenBLAS artifact and original torque assets.\n--frames bounds host validation; the normal loop polls selfdriveState continuously.");
                return Ok(());
            }
            "--numerics" => {
                numerics = args
                    .next()
                    .ok_or(Error::Contract("missing numerical directory"))?
                    .into()
            }
            "--assets" => {
                assets = args
                    .next()
                    .ok_or(Error::Contract("missing torque assets"))?
                    .into()
            }
            "--frames" => {
                let value = args
                    .next()
                    .ok_or(Error::Contract("missing frames"))?
                    .parse::<u64>()
                    .map_err(|_| Error::Contract("invalid frames"))?;
                if value == 0 || frames.is_some() {
                    return Err(Error::Contract("positive unique frame count required"));
                }
                frames = Some(value);
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    runtime::run(frames, &assets, &numerics)
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("controlsd: {error}");
            ExitCode::FAILURE
        }
    }
}
