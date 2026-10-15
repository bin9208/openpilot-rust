fn main() {
    if let Err(error) = openpilot_dev_tools::panda_oracle_build::cli("spi") {
        eprintln!("Panda oracle build: {error}");
        std::process::exit(1);
    }
}
