use openpilot_hardware_control::reboot::sound;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let library = PathBuf::from(args.next().ok_or("fixture library")?);
    let asset = PathBuf::from(args.next().ok_or("asset")?);
    let setting = args.next();
    let volume = sound::volume(setting.as_deref())?;
    if volume == 0.0 {
        return Ok(());
    }
    let pcm = sound::load(&asset, volume)?;
    openpilot_portaudio::play_blocking(&library, &pcm.samples, pcm.channels, pcm.rate)?;
    println!(
        "{}",
        serde_json::json!({"rate":pcm.rate,"channels":pcm.channels,"samples":pcm.samples.len()})
    );
    Ok(())
}
