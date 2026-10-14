fn main() {
    if let Err(error) = openpilot_dev_tools::ffmpeg_build::cli(true) {
        eprintln!("native runtime FFmpeg build: {error}");
        std::process::exit(1);
    }
}
