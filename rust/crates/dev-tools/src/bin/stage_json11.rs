fn main() {
    if let Err(error) = openpilot_dev_tools::json11_stage::cli() {
        eprintln!("native build tool: {error}");
        std::process::exit(1);
    }
}
