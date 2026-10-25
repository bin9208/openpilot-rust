#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    openpilot_dev_tools::changed_files_check::cli()
}
#[cfg(not(target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("native changed-files candidate currently requires Linux".into())
}
