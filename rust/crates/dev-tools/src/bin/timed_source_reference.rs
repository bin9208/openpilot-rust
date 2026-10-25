fn run() -> openpilot_dev_tools::seal::Result<()> {
    #[cfg(feature = "timed-live-native")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--live")) {
        return openpilot_dev_tools::timed_live::source_cli();
    }
    openpilot_dev_tools::timed_reference::cli()
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
