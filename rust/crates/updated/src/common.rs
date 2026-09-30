use crate::{paths::Paths, Error};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::Path,
};

pub fn get_consistent_flag(path: &Path) -> bool {
    path.join(".overlay_consistent").is_file()
}
pub fn touch(path: &Path) -> Result<(), Error> {
    let now = rustix::fs::Timestamps {
        last_access: rustix::time::Timespec {
            tv_sec: 0,
            tv_nsec: rustix::fs::UTIME_NOW,
        },
        last_modification: rustix::time::Timespec {
            tv_sec: 0,
            tv_nsec: rustix::fs::UTIME_NOW,
        },
    };
    if rustix::fs::utimensat(rustix::fs::CWD, path, &now, rustix::fs::AtFlags::empty()).is_err() {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
    }
    Ok(())
}
pub fn unlink_missing_ok(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub fn set_consistent_flag(paths: &Paths, path: &Path, consistent: bool) -> Result<(), Error> {
    paths.sync()?;
    let flag = path.join(".overlay_consistent");
    if consistent {
        touch(&flag)?;
    } else {
        unlink_missing_ok(&flag)?;
    }
    paths.sync()?;
    Ok(())
}
pub fn is_mount(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return false;
    }
    let Ok(parent) = fs::symlink_metadata(path.join("..")) else {
        return false;
    };
    meta.dev() != parent.dev() || meta.ino() == parent.ino()
}
pub fn remove_tree(path: &Path) -> Result<(), Error> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(Error::Contract("cannot rmtree a symlink"));
    }
    Ok(fs::remove_dir_all(path)?)
}
pub fn copy_tree(source: &Path, target: &Path) -> Result<(), Error> {
    let mut failures = Vec::new();
    copy_directory(source, target, &mut failures);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::CopyTree(failures))
    }
}
fn copy_directory(source: &Path, target: &Path, failures: &mut Vec<(std::path::PathBuf, String)>) {
    let copy = (|| -> Result<(), Error> {
        let entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
        fs::create_dir(target)?;
        for entry in entries {
            let from = entry.path();
            let to = target.join(entry.file_name());
            let result = (|| -> Result<(), Error> {
                let metadata = fs::symlink_metadata(&from)?;
                if metadata.file_type().is_symlink() {
                    symlink(fs::read_link(&from)?, &to)?;
                } else if metadata.is_dir() {
                    copy_directory(&from, &to, failures);
                    return Ok(());
                } else if metadata.is_file() {
                    fs::copy(&from, &to)?;
                } else {
                    return Err(Error::Contract("copytree source is not a regular file"));
                }
                copy_stat(&from, &to, &fs::symlink_metadata(&from)?)
            })();
            if let Err(error) = result {
                failures.push((from, error.to_string()));
            }
        }
        copy_stat(source, target, &fs::metadata(source)?)
    })();
    if let Err(error) = copy {
        failures.push((source.into(), error.to_string()));
    }
}
fn copy_stat(source: &Path, target: &Path, metadata: &fs::Metadata) -> Result<(), Error> {
    let link = metadata.file_type().is_symlink();
    let times = rustix::fs::Timestamps {
        last_access: rustix::time::Timespec {
            tv_sec: metadata.atime(),
            tv_nsec: metadata.atime_nsec(),
        },
        last_modification: rustix::time::Timespec {
            tv_sec: metadata.mtime(),
            tv_nsec: metadata.mtime_nsec(),
        },
    };
    rustix::fs::utimensat(
        rustix::fs::CWD,
        target,
        &times,
        if link {
            rustix::fs::AtFlags::SYMLINK_NOFOLLOW
        } else {
            rustix::fs::AtFlags::empty()
        },
    )
    .map_err(std::io::Error::from)?;
    copy_xattrs(source, target, link)?;
    if !link {
        fs::set_permissions(target, fs::Permissions::from_mode(metadata.mode()))?;
    }
    Ok(())
}
fn copy_xattrs(source: &Path, target: &Path, link: bool) -> Result<(), Error> {
    use rustix::{
        fs::{getxattr, lgetxattr, listxattr, llistxattr, lsetxattr, setxattr, XattrFlags},
        io::Errno,
    };
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let mut names = vec![0_u8; 65536];
    let result = if link {
        llistxattr(source, names.as_mut_slice())
    } else {
        listxattr(source, names.as_mut_slice())
    };
    let length = match result {
        Ok(length) => length,
        Err(Errno::NOTSUP | Errno::NODATA | Errno::INVAL) => return Ok(()),
        Err(error) => return Err(std::io::Error::from(error).into()),
    };
    let mut value = vec![0_u8; 65536];
    for name in names[..length].split(|b| *b == 0).filter(|v| !v.is_empty()) {
        let name = OsStr::from_bytes(name);
        let result = (|| {
            let length = if link {
                lgetxattr(source, name, value.as_mut_slice())
            } else {
                getxattr(source, name, value.as_mut_slice())
            }?;
            if link {
                lsetxattr(target, name, &value[..length], XattrFlags::empty())
            } else {
                setxattr(target, name, &value[..length], XattrFlags::empty())
            }
        })();
        match result {
            Ok(()) | Err(Errno::PERM | Errno::NOTSUP | Errno::NODATA | Errno::INVAL) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
    }
    Ok(())
}
