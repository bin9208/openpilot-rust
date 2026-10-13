use openpilot_jpeg::{Color, Layout, Options, Quality, TurboJpeg};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("missing TurboJPEG provider")?;
    let turbo_expected = std::fs::read(
        std::env::args_os()
            .nth(2)
            .ok_or("missing source TurboJPEG golden")?,
    )?;
    let pillow_expected = std::fs::read(
        std::env::args_os()
            .nth(3)
            .ok_or("missing source Pillow golden")?,
    )?;
    let layout = Layout::new(63, 35, Color::Rgbx)?;
    let options = Options::new(Quality::new(82)?);
    let pixels: Vec<_> = (0..layout.len())
        .map(|n| u8::try_from(n % 251).expect("mod251"))
        .collect();
    for _ in 0..100 {
        let mut turbo = TurboJpeg::open(std::path::Path::new(&path))?;
        let first = turbo.encode(&pixels, layout, options)?;
        let second = turbo.encode(&pixels, layout, options)?;
        assert_eq!(first, second);
        assert!(turbo
            .encode(&pixels[..pixels.len() - 1], layout, options)
            .is_err());
        assert!(turbo
            .encode(&[0; 6], Layout::new(2, 1, Color::Rgb)?, options)
            .is_err());
        assert_eq!(first, turbo_expected);
        assert_eq!(
            openpilot_jpeg::encode_with(&pixels, layout, options)?,
            pillow_expected
        );
    }
    println!(
        "100 owned library cycles/200 compressor cycles, invalid lengths/colors, independent source TurboJPEG/Pillow goldens passed"
    );
    Ok(())
}
