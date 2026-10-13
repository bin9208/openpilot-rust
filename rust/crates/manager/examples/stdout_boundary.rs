use std::{os::unix::process::ExitStatusExt, process::ExitCode};
fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let result = (|| {
        let request = openpilot_process_supervision::CapturedCommand {
            launcher: args
                .next()
                .ok_or(openpilot_manager::Error::Contract("helper missing"))?
                .into(),
            cwd: args
                .next()
                .ok_or(openpilot_manager::Error::Contract("cwd missing"))?
                .into(),
            argv: args.collect(),
        };
        openpilot_manager::output::run(&request)
    })();
    match result {
        Ok(status) => ExitCode::from(u8::try_from(status.into_raw() >> 8).unwrap_or(1)),
        Err(error) => {
            eprintln!("stdout boundary: {error}");
            ExitCode::FAILURE
        }
    }
}
