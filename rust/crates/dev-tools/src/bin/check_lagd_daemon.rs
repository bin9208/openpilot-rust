fn main() {
    std::process::exit(openpilot_dev_tools::lagd_live_check::cli(
        openpilot_dev_tools::lagd_live_check::Kind::Daemon,
    ));
}
