#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = openpilot_dev_tools::bootlog_snapshot_check::child_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootlog snapshot child requires Linux");
    std::process::exit(1);
}
