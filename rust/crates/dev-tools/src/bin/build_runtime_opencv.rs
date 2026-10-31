fn main() {
    if let Err(error) = run() {
        eprintln!("OpenCV provider build: {error}");
        std::process::exit(1);
    }
}

fn run() -> openpilot_dev_tools::seal::Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .iter()
        .any(|arg| arg == "--prepare-released-headers" || arg == "--prepare-released-prefix")
    {
        let arguments = arguments
            .into_iter()
            .map(|arg| {
                arg.into_string()
                    .map_err(|_| "non-UTF8 OpenCV header argument")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return if arguments
            .iter()
            .any(|arg| arg == "--prepare-released-prefix")
        {
            openpilot_dev_tools::native_opencv_prefix::cli(&arguments)
        } else {
            openpilot_dev_tools::native_opencv_headers::cli(&arguments)
        };
    }
    openpilot_dev_tools::opencv_build::cli(openpilot_dev_tools::opencv_build::Profile::Runtime)
}
