#![feature(rustc_private)]
extern crate libc;
#[path = "../crates/loggerd/src/raw_file.rs"]
mod raw_file;
use raw_file::RawFile;
use std::{io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("expected private output directory")?,
    );
    std::fs::create_dir_all(&root)?;
    let path = root.join("normal");
    let mut file = RawFile::create(&path)?;
    Write::write_all(&mut file, b"checked-close")?;
    Write::flush(&mut file)?;
    file.finish_checked()?;
    assert_eq!(std::fs::read(&path)?, b"checked-close");
    let mut small = RawFile::create(std::path::Path::new("/dev/full"))?;
    Write::write_all(&mut small, b"buffered")?;
    small.finish_checked()?;
    let mut large = RawFile::create(std::path::Path::new("/dev/full"))?;
    assert!(Write::write_all(&mut large, &[7; 16384]).is_err());
    drop(large);
    let path = root.join("existing-finish");
    let mut existing = RawFile::create(&path)?;
    existing.write(b"existing")?;
    existing.finish();
    assert_eq!(std::fs::read(&path)?, b"existing");
    assert!(RawFile::create(&root).is_err());
    if std::env::var("BOOTLOG_OUTPUT_FAULT").as_deref() == Ok("close") {
        std::fs::create_dir_all(root.join("boot"))?;
        let mut checked = RawFile::create(&root.join("boot/close.zst"))?;
        checked.write(b"close failure")?;
        assert!(checked.finish_checked().is_err());
    }
    println!("PASS normal Write/flush/checked-close, small final-flush failure, large write failure/drop, existing finish, failed open");
    Ok(())
}
