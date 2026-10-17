//! Standalone native SDK staging entrypoint; compile with rustc, then pass
//! source-archive directory, ignored workspace, and install destination.
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
mod receipt;
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const SOURCES: [(&str, &str, &str); 9] = [
    (
        "dav1d-1.5.3.tar.gz",
        "cbe212b02faf8c6eed5b6d55ef8a6e363aaab83f15112e960701a9c3df813686",
        "dav1d-1.5.3",
    ),
    (
        "libavif-1.4.2.tar.gz",
        "2b645287340ba5a631d268b551dc2d72bd73ac33335962dd36dcdb6d8366921d",
        "libavif-1.4.2",
    ),
    (
        "openjpeg-2.5.4.tar.gz",
        "a695fbe19c0165f295a8531b1e4e855cd94d0875d2f88ec4b61080677e27188a",
        "openjpeg-2.5.4",
    ),
    (
        "libyuv-644251f25.tar.gz",
        "ca529dfd9afddd8735a628cb5667cc8318bedc91129bf76c66daa961e7b29ca6",
        "libyuv-1924",
    ),
    (
        "tiff-4.7.1.tar.gz",
        "f698d94f3103da8ca7438d84e0344e453fe0ba3b7486e04c5bf7a9a3fabe9b69",
        "tiff-4.7.1",
    ),
    (
        "xz-5.8.3.tar.gz",
        "8ec1767fa517642ecb4cf08b891ce667ba6f143551e382b07c7ef437bda335e2",
        "xz-5.8.3",
    ),
    (
        "zstd-1.5.7.tar.gz",
        "37d7284556b20954e56e1ca85b80226768902e2edabd3b649e9e72c0c9012ee3",
        "zstd-1.5.7",
    ),
    (
        "zlib-1.3.tar.gz",
        "ff0ba4c292013dbc27530b3a81e1f9a813cd39de01ca5e0f8bf355702efa593e",
        "zlib-1.3",
    ),
    (
        "libjpeg-turbo-3.1.4.1.tar.gz",
        "ecae8008e2cc9ade2f2c1bb9d5e6d4fb73e7c433866a056bd82980741571a022",
        "libjpeg-turbo-3.1.4.1",
    ),
];
fn checked(command: &mut Command) -> Result<()> {
    if !command.status()?.success() {
        return Err(format!("native SDK command failed: {command:?}").into());
    }
    Ok(())
}
fn value(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!("native SDK command failed: {command:?}").into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn new_destination(path: &Path) -> Result<PathBuf> {
    match path.symlink_metadata() {
        Ok(_) => return Err(format!("destination already exists: {}", path.display()).into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let absolute = std::env::current_dir()?.join(path);
    let parent = absolute
        .parent()
        .ok_or("destination parent missing")?
        .canonicalize()?;
    Ok(parent.join(absolute.file_name().ok_or("destination name missing")?))
}
fn stage(args: &[PathBuf]) -> Result<()> {
    if args.len() != 4 {
        return Err("usage: stage-image-codecs ARCHIVES WORK INSTALL MODULE_SDK_SOURCE".into());
    }
    let archives = args[0].canonicalize()?;
    let work = new_destination(&args[1])?;
    let install = new_destination(&args[2])?;
    if work == install {
        return Err("work and install destinations must be distinct".into());
    }
    let source = args[3].canonicalize()?;
    // The caller also coordinates the shared build slot; this native gate enforces
    // the repository floor before any extraction or build allocation.
    let disk = std::env::var_os("CHECK_DISK_SPACE_BINARY")
        .ok_or("set CHECK_DISK_SPACE_BINARY to the verified repository disk-space gate")?;
    for destination in [&work, &install] {
        checked(
            Command::new(&disk)
                .arg("--path")
                .arg(destination.parent().ok_or("destination parent missing")?)
                .arg("--growth-mib")
                .arg("1024"),
        )?;
    }
    // Exclusive creation avoids silently reusing a partial build or old receipt.
    // Failure leaves diagnostic artifacts in these caller-owned new directories.
    fs::create_dir(&work)?;
    fs::create_dir(&install)?;
    for (name, hash, folder) in SOURCES {
        let archive = archives.join(name);
        let digest = value(Command::new("sha256sum").arg(&archive))?;
        if digest.split_whitespace().next() != Some(hash) {
            return Err(format!("source archive hash mismatch: {name}").into());
        }
        let list = value(Command::new("tar").arg("-tzf").arg(&archive))?;
        for entry in list.lines() {
            let p = Path::new(entry);
            if p.is_absolute()
                || p.components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err("unsafe source archive path".into());
            }
        }
        let extract = work.join(folder);
        if extract.exists() {
            return Err(format!(
                "source folder already exists; select a clean staging workspace: {}",
                extract.display()
            )
            .into());
        }
        fs::create_dir(&extract)?;
        let mut command = Command::new("tar");
        command.arg("-xzf").arg(&archive).arg("-C").arg(&extract);
        if !name.starts_with("libyuv-") {
            command.arg("--strip-components=1");
        }
        checked(&mut command)?;
    }
    let build = work.join("build");
    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(&source)
        .arg("-B")
        .arg(&build)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", install.display()));
    for (key, folder) in [
        ("DAV1D_SOURCE", "dav1d-1.5.3"),
        ("LIBAVIF_SOURCE", "libavif-1.4.2"),
        ("OPENJPEG_SOURCE", "openjpeg-2.5.4"),
        ("LIBYUV_SOURCE", "libyuv-1924"),
        ("TIFF_SOURCE", "tiff-4.7.1"),
        ("XZ_SOURCE", "xz-5.8.3"),
        ("ZSTD_SOURCE", "zstd-1.5.7"),
        ("ZLIB_SOURCE", "zlib-1.3"),
        ("JPEG_SOURCE", "libjpeg-turbo-3.1.4.1"),
    ] {
        configure.arg(format!("-D{key}={}", work.join(folder).display()));
    }
    checked(&mut configure)?;
    checked(
        Command::new("cmake")
            .arg("--build")
            .arg(&build)
            .arg("--parallel")
            .arg("2"),
    )?;
    checked(Command::new("cmake").arg("--install").arg(&build))?;
    let mut receipt = String::from(receipt::HEADER);
    for library in receipt::LIBRARIES {
        let file = install.join("lib").join(format!("lib{library}.a"));
        receipt.push_str(&format!(
            "{}  {}\n",
            receipt::digest(&file)?,
            file.display()
        ));
    }
    fs::write(install.join("openpilot-image-codecs-sdk.txt"), receipt)?;
    receipt::verify(&install)?;
    Ok(())
}
fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if let Err(error) = stage(&args) {
        eprintln!("{error}");
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Temporary(PathBuf);
    impl Temporary {
        fn new() -> Self {
            static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "image-sdk-stage-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn args(&self) -> Vec<PathBuf> {
            vec![
                self.0.clone(),
                self.0.join("work"),
                self.0.join("install"),
                self.0.clone(),
            ]
        }
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn existing_install_and_receipt_are_rejected_before_work_is_created() {
        let temp = Temporary::new();
        let args = temp.args();
        fs::create_dir(&args[2]).unwrap();
        let receipt = args[2].join("openpilot-image-codecs-sdk.txt");
        fs::write(&receipt, b"old successful build").unwrap();
        let error = stage(&args).unwrap_err().to_string();
        assert!(error.contains("destination already exists"), "{error}");
        assert!(!args[1].exists());
        assert_eq!(fs::read(receipt).unwrap(), b"old successful build");
    }

    #[test]
    fn existing_workspace_is_rejected_without_creating_install() {
        let temp = Temporary::new();
        let args = temp.args();
        fs::create_dir(&args[1]).unwrap();
        fs::write(args[1].join("keep"), b"existing build").unwrap();
        let error = stage(&args).unwrap_err().to_string();
        assert!(error.contains("destination already exists"), "{error}");
        assert!(!args[2].exists());
        assert_eq!(fs::read(args[1].join("keep")).unwrap(), b"existing build");
    }

    #[test]
    fn dangling_destination_symlink_is_not_followed() {
        let temp = Temporary::new();
        let args = temp.args();
        std::os::unix::fs::symlink(temp.0.join("missing"), &args[2]).unwrap();
        assert!(stage(&args)
            .unwrap_err()
            .to_string()
            .contains("destination already exists"));
        assert!(!args[1].exists());
        assert!(!temp.0.join("missing").exists());
    }

    #[test]
    fn identical_destinations_are_rejected_without_allocations() {
        let temp = Temporary::new();
        let mut args = temp.args();
        args[2] = args[1].clone();
        assert!(stage(&args)
            .unwrap_err()
            .to_string()
            .contains("must be distinct"));
        assert!(!args[1].exists());
    }
}
