use super::platform::{remove, Paths};
use crate::Error;
use std::{fs, os::unix::fs::symlink};
pub(crate) fn composite(paths: &Paths) -> Result<(), Error> {
    paths.detached()?;
    let function = paths.function();
    if function.exists() {
        paths.owned()?;
    } else {
        fs::create_dir(&function)?;
        fs::write(paths.state.join("ncm-owned"), "ncm.jetlink\n")?;
    }
    for (name, value) in [
        ("bcdDevice", "0x0101"),
        ("bDeviceClass", "0xEF"),
        ("bDeviceSubClass", "0x02"),
        ("bDeviceProtocol", "0x01"),
    ] {
        fs::write(paths.gadget.join(name), format!("{value}\n"))?;
    }
    for (name, value) in [
        ("dev_addr", "02:4a:4c:60:00:01"),
        ("host_addr", "02:4a:4c:60:00:02"),
    ] {
        if let Err(error) = fs::write(function.join(name), format!("{value}\n")) {
            eprintln!("mobile kernel rejected fixed {name}, retaining generated MAC: {error}");
        }
    }
    let link = paths.link();
    if link.is_symlink() {
        if fs::canonicalize(&link)? != fs::canonicalize(&function)? {
            return Err(Error::Contract("unexpected NCM configuration link"));
        }
    } else {
        symlink(&function, link)?;
    }
    Ok(())
}
pub(crate) fn detach(paths: &Paths) -> Result<(), Error> {
    paths.detached()?;
    if paths.function().is_dir() && !paths.state.join("ncm-owned").is_file() {
        return Err(Error::Contract("refusing to detach unowned NCM"));
    }
    if paths.state.join("ncm-owned").exists() {
        paths.owned()?;
        let link = paths.link();
        if link.is_symlink() {
            if fs::canonicalize(&link)? != fs::canonicalize(paths.function())? {
                return Err(Error::Contract("refusing unexpected NCM link"));
            }
            remove(&link)?;
        }
        for (name, value) in [
            ("bcdDevice", "0x0100"),
            ("bDeviceClass", "0x00"),
            ("bDeviceSubClass", "0x00"),
            ("bDeviceProtocol", "0x00"),
        ] {
            fs::write(paths.gadget.join(name), format!("{value}\n"))?;
        }
    }
    // Retain the detached kernel NCM instance: AGNOS4.9 configfs rmdir fault.
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::super::platform::text;
    use super::*;
    #[test]
    fn composite_ownership_and_detached_retention_match_source() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            gadget: dir.path().join("gadget"),
            state: dir.path().join("state"),
            net: dir.path().join("net"),
            leases: dir.path().join("leases"),
            proc: dir.path().join("proc"),
        };
        fs::create_dir_all(paths.gadget.join("functions")).unwrap();
        fs::create_dir_all(paths.gadget.join("configs/c.1")).unwrap();
        fs::create_dir_all(&paths.state).unwrap();
        fs::write(paths.gadget.join("UDC"), "controller").unwrap();
        assert!(composite(&paths).is_err());
        fs::write(paths.gadget.join("UDC"), "").unwrap();
        composite(&paths).unwrap();
        assert_eq!(text(&paths.gadget.join("bDeviceClass")).unwrap(), "0xEF");
        assert!(paths.link().is_symlink());
        detach(&paths).unwrap();
        assert!(paths.function().is_dir());
        assert!(!paths.link().exists());
        assert_eq!(text(&paths.gadget.join("bcdDevice")).unwrap(), "0x0100");
        composite(&paths).unwrap();
        assert!(paths.link().is_symlink());
        remove(&paths.state.join("ncm-owned")).unwrap();
        assert!(detach(&paths).is_err());
    }
}
