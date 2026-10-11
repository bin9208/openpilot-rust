use std::{env, fs, path::PathBuf};

fn constant(source: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = source
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == name).then_some(value.trim())
        })
        .ok_or_else(|| format!("source constant {name} missing"))?;
    let block = if value == "(" {
        let start = source
            .find(&format!("{name} = ("))
            .ok_or("source constant opening missing")?;
        let remaining = &source[start + name.len() + 4..];
        &remaining[..remaining
            .find(')')
            .ok_or("source constant closing missing")?]
    } else {
        value
    };
    let mut output = String::new();
    for line in block.lines() {
        let line = line.trim();
        if let Some(quote) = line
            .chars()
            .next()
            .filter(|quote| matches!(quote, '\'' | '"'))
        {
            output.push_str(
                line.strip_prefix(quote)
                    .and_then(|line| line.strip_suffix(quote))
                    .ok_or("source constant string missing")?,
            );
        }
    }
    if output.is_empty() {
        return Err("source constant is empty".into());
    }
    Ok(output)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("manifest directory missing")?);
    let path = root.join("../../../openpilot/selfdrive/carrot/server/config.py");
    println!("cargo:rerun-if-changed={}", path.display());
    let source = fs::read_to_string(path)?;
    let webhook = constant(&source, "VISION_DIAG_DEFAULT_DISCORD_WEBHOOK")?;
    let key = constant(&source, "VISION_DIAG_DEFAULT_DISCORD_KEY")?;
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("build output missing")?);
    fs::write(
        output.join("vision_diag_defaults.rs"),
        format!("const DEFAULT_WEBHOOK: &str = {webhook:?};\nconst DEFAULT_KEY: &str = {key:?};\n"),
    )?;
    Ok(())
}
