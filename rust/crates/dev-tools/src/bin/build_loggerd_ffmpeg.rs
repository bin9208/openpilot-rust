fn main() {
    if let Err(error) = openpilot_dev_tools::ffmpeg_build::cli(false) {
        eprintln!("native loggerd FFmpeg build: {error}");
        std::process::exit(1);
    }
}
