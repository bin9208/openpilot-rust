use openpilot_startup_ui::egl::{Context, FrameLayout};
use std::os::fd::AsFd;
fn descriptors() -> Result<usize, std::io::Error> {
    Ok(std::fs::read_dir("/proc/self/fd")?.count())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = descriptors()?;
    let layout = FrameLayout {
        width: 1928,
        height: 1208,
        stride: 2048,
        uv_offset: 2473984,
    };
    let mut initialized = false;
    let mut created = 0;
    let mut rejected = 0;
    match Context::new() {
        Ok(context) => {
            initialized = true;
            let file = std::fs::File::open("/dev/null")?;
            for _ in 0..3 {
                match context.create(layout, file.as_fd()) {
                    Ok(image) => {
                        image.bind(17);
                        created += 1;
                    }
                    Err(_) => rejected += 1,
                }
            }
        }
        Err(error) => eprintln!("EGL initialization: {error}"),
    }
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"initialized":initialized,"created":created,"rejected":rejected,"fd_delta":isize::try_from(descriptors()?)?-isize::try_from(count)?})
        )?
    );
    Ok(())
}
