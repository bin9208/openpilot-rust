use std::{env, fs, io, path::PathBuf};

fn decode(input: &str) -> String {
    let mut chars = input.chars().peekable();
    let mut output = String::new();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some('"') => output.push('"'),
            Some('\'') => output.push('\''),
            Some('\\') => output.push('\\'),
            Some('\n') => {}
            Some(character) => {
                output.push('\\');
                output.push(character);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn main() -> io::Result<()> {
    let root = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("crate path missing"))?,
    );
    let source = root.join("../../../openpilot/selfdrive/carrot/recovery/server.py");
    println!("cargo:rerun-if-changed={}", source.display());
    let text = fs::read_to_string(source)?;
    let output = PathBuf::from(
        env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("output path missing"))?,
    );
    for (name, file) in [
        ("HTML_PAGE", "index.html"),
        ("RECOVERY_JS", "recovery.js"),
        ("RECOVERY_API_JS", "recovery-api.js"),
    ] {
        let marker = format!("{name} = \"\"\"");
        let body = text
            .split_once(&marker)
            .and_then(|(_, body)| body.split_once("\"\"\"").map(|(body, _)| body))
            .ok_or_else(|| io::Error::other(format!("source asset {name} missing")))?;
        fs::write(output.join(file), decode(body))?;
    }
    Ok(())
}
