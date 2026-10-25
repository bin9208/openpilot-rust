fn main() {
    if let Err(error) = openpilot_dev_tools::planner_ci_controller::cli() {
        eprintln!("{error}");
        let status = error
            .downcast_ref::<openpilot_dev_tools::planner_ci_controller::CheckFailure>()
            .map(|failure| failure.returncode)
            .filter(|code| (1..=255).contains(code))
            .unwrap_or(1);
        std::process::exit(status);
    }
}
