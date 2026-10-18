fn main() {
    std::process::exit(openpilot_dev_tools::lagd_check::cli(
        openpilot_dev_tools::lagd_check::Kind::Loop,
    ));
}
