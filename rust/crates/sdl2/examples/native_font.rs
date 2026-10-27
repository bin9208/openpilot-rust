//! Actual glyph, surface and software-window bytes from explicit providers.
use openpilot_sdl2::gui::Context;
use std::{env, fmt::Write as _, fs, io, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(io::Error::other("native_font SDL TTF FONT OUTPUT_DIRECTORY").into());
    }
    let root = Path::new(&args[3]);
    fs::create_dir(root)?;
    let context = Context::load(&args[0], &args[1])?;
    let mut dimensions = String::new();
    for (index, (size, bold, text)) in [
        (12, false, "native replay 63/35"),
        (12, true, "Å β [literal]"),
        (18, false, "native replay 63/35"),
        (18, true, "Å β [literal]"),
    ]
    .into_iter()
    .enumerate()
    {
        let font = context.font(Path::new(&args[2]), size, bold)?;
        assert!(font.render("bad\0text", [230, 233, 240]).is_err());
        let image = font.render(text, [230, 233, 240])?;
        assert!(image.width > 0 && image.height > 0);
        let mut window = context.window(
            "Owned native font",
            image.width,
            image.height,
            (0, 0),
            false,
        )?;
        window.present(&image.rgba, image.width, image.height)?;
        let captured = window.capture()?;
        assert_eq!(
            (captured.width, captured.height),
            (image.width, image.height)
        );
        writeln!(&mut dimensions, "{} {}", image.width, image.height)?;
        fs::write(root.join(format!("{index}.glyph.rgba")), image.rgba)?;
        fs::write(root.join(format!("{index}.window.rgba")), captured.rgba)?;
    }
    fs::write(root.join("dimensions.txt"), dimensions)?;
    fs::write(root.join("provider-maps.txt"), fs::read("/proc/self/maps")?)?;
    println!("4 owned glyph/surface/software-window cycles passed");
    Ok(())
}
