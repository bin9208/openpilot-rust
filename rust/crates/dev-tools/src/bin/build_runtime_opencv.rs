fn main() {
    if let Err(error) =
        openpilot_dev_tools::opencv_build::cli(openpilot_dev_tools::opencv_build::Profile::Runtime)
    {
        eprintln!("OpenCV provider build: {error}");
        std::process::exit(1);
    }
}
