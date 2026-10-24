//! Publish emitted model-catalog bytes; this command never loads model providers.
#[cfg(feature = "native-skip-miri")]
#[path = "../catalog_publish.rs"]
mod catalog_publish;

#[cfg(feature = "native-skip-miri")]
fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args_os().skip(1);
    let mut paths = Vec::new();
    for name in ["--root", "--staged-generation", "--entries"] {
        if args.next().as_deref() != Some(std::ffi::OsStr::new(name)) {
            return Err(format!("expected {name} PATH").into());
        }
        paths.push(std::path::PathBuf::from(
            args.next().ok_or("missing catalog path")?,
        ));
    }
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let result = catalog_publish::publish(&paths[0], &paths[1], &paths[2])?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    #[cfg(feature = "native-skip-miri")]
    if let Err(error) = run() {
        eprintln!("native model catalog publication: {error}");
        std::process::exit(1);
    }
    #[cfg(not(feature = "native-skip-miri"))]
    {
        eprintln!("native model catalog publication requires native-skip-miri");
        std::process::exit(1);
    }
}
