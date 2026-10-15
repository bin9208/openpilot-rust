fn main() {
    if let Err(error) = openpilot_dev_tools::panda_oracle_build::cli("runtime-fixture") {
        eprintln!("Panda oracle build: {error}");
        std::process::exit(1);
    }
}
