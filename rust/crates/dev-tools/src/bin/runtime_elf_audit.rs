use openpilot_dev_tools::elf_audit::audit;
use std::{fs, io::Write, path::PathBuf};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut roots = Vec::new();
    let mut libraries = Vec::new();
    let mut output = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str().ok_or("invalid argument")? {
            "--root" => roots.push(PathBuf::from(args.next().ok_or("missing --root value")?)),
            "--library" => {
                libraries.push(PathBuf::from(args.next().ok_or("missing --library value")?))
            }
            "--output" => {
                if output.is_some() {
                    return Err("duplicate --output".into());
                }
                output = Some(PathBuf::from(args.next().ok_or("missing --output value")?));
            }
            "--help" | "-h" => {
                println!("Usage: runtime-elf-audit --root ELF [--root ELF ...] --library DIRECTORY [--library DIRECTORY ...] --output REPORT.json");
                return Ok(());
            }
            _ => return Err("unknown argument".into()),
        }
    }
    if roots.is_empty() || libraries.is_empty() {
        return Err("--root and --library required".into());
    }
    let output = output.ok_or("--output required")?;
    let report = audit(&roots, &libraries, 183)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(output)?;
    serde_json::to_writer_pretty(&mut file, &report)?;
    file.write_all(b"\n")?;
    println!(
        "{}",
        serde_json::json!({"compatible":report["compatible"],"objects":report["objects"].as_array().ok_or("missing objects")?.len(),"roots":report["roots"].as_array().ok_or("missing roots")?.len()})
    );
    if report["compatible"] != true {
        return Err("native library closure is incompatible".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("runtime ELF audit: {e}");
        std::process::exit(1);
    }
}
