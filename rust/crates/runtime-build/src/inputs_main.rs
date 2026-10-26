use openpilot_runtime_build::{inputs::Tree, Result};
use std::{fs::File, io::Write, path::PathBuf};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut root = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-inputs --root INPUT_TREE --output RECEIPT_JSON\nCapture actual unpacked input files, modes and contained file links. The caller must bind this receipt to independently verified source or asset provenance before reuse.");
                return Ok(());
            }
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("missing input tree")?))
            }
            Some("--output") if output.is_none() => {
                output = Some(PathBuf::from(args.next().ok_or("missing receipt path")?))
            }
            _ => return Err("unknown or duplicate input receipt argument".into()),
        }
    }
    let root = root.ok_or("--root is required")?;
    let output = output.ok_or("--output is required")?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."))
        .canonicalize()?;
    if parent.starts_with(root.canonicalize()?) {
        return Err("input receipt must be outside the input tree".into());
    }
    let tree = Tree::capture(&root)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
    serde_json::to_writer_pretty(&mut temporary, &tree)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(&output)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime inputs: {error}");
        std::process::exit(1);
    }
}
