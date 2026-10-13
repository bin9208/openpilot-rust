use std::{
    os::unix::{net::UnixStream, process::ExitStatusExt},
    process::{Command, ExitCode},
    sync::{atomic::AtomicBool, Arc},
};
fn main() -> ExitCode {
    let result = (|| -> Result<_, openpilot_runtime_launcher::Error> {
        let stopped = Arc::new(AtomicBool::new(false));
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            signal_hook::flag::register(signal, Arc::clone(&stopped))?;
        }
        let mut signals = signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
        ])?;
        let mut stream =
            UnixStream::connect(std::env::var_os("CARROT_BOOT_LOCK_SOCKET").ok_or_else(|| {
                openpilot_runtime_launcher::Error::Configuration("lock socket missing".into())
            })?)?;
        let lock = openpilot_process_supervision::receive_boot_lock(&mut stream)?;
        let mut args = std::env::args_os().skip(1);
        let mut command = Command::new(args.next().ok_or_else(|| {
            openpilot_runtime_launcher::Error::Configuration("manager missing".into())
        })?);
        command.args(args);
        openpilot_runtime_launcher::handoff::run_locked_with_signals(
            command,
            &lock,
            &stopped,
            &mut signals,
        )
    })();
    match result {
        Ok(status) => ExitCode::from(u8::try_from(status.into_raw() >> 8).unwrap_or(1)),
        Err(error) => {
            eprintln!("manager handoff: {error}");
            ExitCode::FAILURE
        }
    }
}
