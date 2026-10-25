//! Source-bound SDK receipt verification shared by the builder and Cargo gate.
use std::{error::Error, fs, path::Path, process::Command};
pub const LIBRARIES: [&str; 9] = [
    "avif", "dav1d", "yuv", "openjp2", "tiff", "jpeg", "lzma", "zstd", "z",
];
pub const HEADER: &str = "schema=1\nlibavif=1.4.2\ndav1d=1.5.3\nlibyuv=1924\nopenjpeg=2.5.4\nlibtiff=4.7.1\nlibjpeg-turbo=3.1.4.1\nliblzma=5.8.3\nzstd=1.5.7\nzlib=1.3\n";
pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub fn digest(path: &Path) -> Result<String> {
    let output = Command::new("sha256sum").arg("--").arg(path).output()?;
    if !output.status.success() {
        return Err(format!("could not hash SDK input {}", path.display()).into());
    }
    let text = String::from_utf8(output.stdout)?;
    let value = text.split_whitespace().next().ok_or("missing SDK digest")?;
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid SDK digest".into());
    }
    Ok(value.to_owned())
}

pub fn verify(root: &Path) -> Result<()> {
    let text = fs::read_to_string(root.join("openpilot-image-codecs-sdk.txt"))?;
    let rest = text
        .strip_prefix(HEADER)
        .ok_or("SDK version receipt mismatch")?;
    let rows = rest.lines().collect::<Vec<_>>();
    if rows.len() != LIBRARIES.len() {
        return Err("SDK library receipt count mismatch".into());
    }
    for (row, name) in rows.iter().zip(LIBRARIES) {
        let file = root.join("lib").join(format!("lib{name}.a"));
        let (expected, recorded_path) = row.split_once("  ").ok_or("invalid SDK receipt row")?;
        if recorded_path != file.to_str().ok_or("SDK path is not UTF-8")?
            || expected != digest(&file)?
        {
            return Err(format!("SDK library receipt mismatch: {name}").into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modified_library_or_incomplete_receipt_is_rejected() {
        let root = std::env::temp_dir().join(format!("sdk-receipt-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("lib")).unwrap();
        let mut receipt = HEADER.to_string();
        for name in LIBRARIES {
            let path = root.join("lib").join(format!("lib{name}.a"));
            fs::write(&path, name).unwrap();
            receipt.push_str(&format!("{}  {}\n", digest(&path).unwrap(), path.display()));
        }
        let path = root.join("openpilot-image-codecs-sdk.txt");
        fs::write(&path, &receipt).unwrap();
        verify(&root).unwrap();
        fs::write(root.join("lib/libavif.a"), "substituted build").unwrap();
        assert!(verify(&root).is_err());
        fs::write(root.join("lib/libavif.a"), "avif").unwrap();
        fs::write(&path, HEADER).unwrap();
        assert!(verify(&root).is_err());
        fs::write(&path, receipt.replace("libavif=1.4.2", "libavif=1.0.0")).unwrap();
        assert!(verify(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
