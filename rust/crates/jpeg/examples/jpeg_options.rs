use openpilot_jpeg::{Color, Layout, Options, Quality};
use std::{env, fs, io, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 6 {
        return Err(io::Error::other(
            "jpeg_options INPUT OUTPUT WIDTH HEIGHT rgb|gray|legacy QUALITY",
        )
        .into());
    }
    let data = fs::read(&args[0])?;
    let width = args[2].parse()?;
    let height = args[3].parse()?;
    let quality = Quality::new(args[5].parse()?)?;
    let encoded = match args[4].as_str() {
        "legacy" => {
            if quality.value() != 75 {
                return Err(io::Error::other("legacy API has fixed quality75").into());
            }
            openpilot_jpeg::encode(&data, width, height)?
        }
        "rgb" => openpilot_jpeg::encode_with(
            &data,
            Layout::new(width, height, Color::Rgb)?,
            Options::new(quality),
        )?,
        "gray" => openpilot_jpeg::encode_with(
            &data,
            Layout::new(width, height, Color::Gray)?,
            Options::new(quality),
        )?,
        color => {
            return Err(io::Error::other(format!("unsupported JPEG trace color {color}")).into());
        }
    };
    let output = PathBuf::from(&args[1]);
    fs::write(
        output.with_extension("maps.txt"),
        fs::read("/proc/self/maps")?,
    )?;
    fs::write(output, encoded)?;
    Ok(())
}
