fn main() {
    if let Err(error) = openpilot_dev_tools::pandad_firmware_spi_build::cli() {
        eprintln!("Panda firmware SPI build: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::pandad_firmware_spi_build::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
