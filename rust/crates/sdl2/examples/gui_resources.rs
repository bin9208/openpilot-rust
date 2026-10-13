#[cfg(feature = "gui")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use openpilot_sdl2::gui::{Context, Tk};
    use std::path::Path;
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        return Err("expected SDL TTF FONT TCL TK".into());
    }
    for index in 0..30 {
        let context = Context::load(&args[1], &args[2])?;
        let font = context.font(Path::new(&args[3]), 16, index % 2 == 0)?;
        assert!(font.render("bad\0text", [255; 3]).is_err());
        let image = font.render("replay 한국어 Å β [literal]", [230, 233, 240])?;
        let mut window = context.window(
            "Owned SDL GUI resources",
            image.width.max(1),
            image.height,
            (0, 0),
            false,
        )?;
        window.present(&image.rgba, image.width, image.height)?;
        let captured = window.capture()?;
        assert_eq!(captured.width, image.width);
        assert_eq!(captured.height, image.height);
    }
    for _ in 0..3 {
        let mut tk = Tk::load(&args[4], &args[5])?;
        tk.eval(&["wm".into(), "withdraw".into(), ".".into()])?;
        let literal = "{[exit]} $env(HOME) ; \"유니코드\"";
        assert_eq!(
            tk.eval(&["set".into(), "owned_literal".into(), literal.into()])?,
            literal
        );
        assert_eq!(tk.eval(&["set".into(), "owned_literal".into()])?, literal);
        tk.eval(&["destroy".into(), ".".into()])?;
    }
    println!(
        "30 SDL context/window/font/surface cycles +3 Tcl literal-object/interpreter cycles passed"
    );
    Ok(())
}
#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("requires gui feature");
    std::process::exit(1);
}
