fn main() {
    if let Err(error) = openpilot_dev_tools::services_header::cli() {
        eprintln!("native build tool: {error}");
        std::process::exit(1);
    }
}
