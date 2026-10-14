use std::{env, fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    let root = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("crate path missing"))?,
    );
    let output = PathBuf::from(
        env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("output path missing"))?,
    );
    for name in ["index.html", "recovery.js", "recovery-api.js"] {
        let source = root.join("assets").join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(source, output.join(name))?;
    }
    Ok(())
}
