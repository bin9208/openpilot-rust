//! Actual source API and resource cycles against an explicit provider.
use openpilot_jpeg::{Color, Layout, Options, Quality, TurboJpeg};
use std::{env, fs, io, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 6 {
        return Err(
            io::Error::other("native_turbojpeg LIBRARY INPUT OUTPUT WIDTH HEIGHT QUALITY").into(),
        );
    }
    let pixels = fs::read(&args[1])?;
    let layout = Layout::new(args[3].parse()?, args[4].parse()?, Color::Rgbx)?;
    let options = Options::new(Quality::new(args[5].parse()?)?);
    let mut golden = None;
    for _ in 0..20 {
        let mut turbo = TurboJpeg::open(Path::new(&args[0]))?;
        let encoded = turbo.encode(&pixels, layout, options)?;
        assert_eq!(encoded, turbo.encode(&pixels, layout, options)?);
        if let Some(previous) = &golden {
            assert_eq!(&encoded, previous);
        }
        assert!(turbo
            .encode(&pixels[..pixels.len() - 1], layout, options)
            .is_err());
        assert!(turbo
            .encode(&[0; 6], Layout::new(2, 1, Color::Rgb)?, options)
            .is_err());
        golden = Some(encoded);
        fs::write(
            Path::new(&args[2]).with_extension("maps.txt"),
            fs::read("/proc/self/maps")?,
        )?;
    }
    fs::write(
        &args[2],
        golden.ok_or_else(|| io::Error::other("missing encoded output"))?,
    )?;
    println!("20 library/40 compressor cycles and invalid lengths/colors passed");
    Ok(())
}
