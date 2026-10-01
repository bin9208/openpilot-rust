fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, font, text, output] = args.as_slice() else {
        return Err("usage: emoji_raster FONT TEXT OUTPUT".into());
    };
    let pixels = openpilot_ui_framework::emoji::rasterize(&std::fs::read(font)?, text)?;
    let mut encoder = png::Encoder::new(std::fs::File::create(output)?, 128, 128);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
