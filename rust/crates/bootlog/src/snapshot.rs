//! Params copy and detached startup worker from manager/helpers.py::save_bootlog.
use openpilot_process_supervision::CapturedCommand;
use std::{
    ffi::OsStr,
    fs::{self, File, Metadata},
    io,
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error("Params copy failed; snapshot retained at {}: {failures:?}", .snapshot.display())]
    Copy {
        snapshot: PathBuf,
        failures: Vec<(PathBuf, String)>,
    },
}

pub struct Snapshot {
    path: PathBuf,
}

impl Snapshot {
    pub fn capture(params_directory: &Path, temp_root: &Path) -> Result<Self, Error> {
        // Keep the directory on copy/thread/spawn errors, matching mkdtemp without finally.
        let path = tempfile::Builder::new()
            .prefix("bootlog-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(temp_root)?
            .keep();
        let name = params_directory
            .file_name()
            .unwrap_or_else(|| OsStr::new(""));
        let mut failures = Vec::new();
        copy_tree(params_directory, &path.join(name), &mut failures);
        if !failures.is_empty() {
            return Err(Error::Copy {
                snapshot: path,
                failures,
            });
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn run(self, loggerd_directory: &Path, launcher: &Path) -> Result<(), Error> {
        let binary = loggerd_directory.join("bootlog");
        if binary.exists() {
            // An unsuccessful child exit still returns normally and triggers cleanup.
            let mut child = CapturedCommand {
                launcher: launcher.to_owned(),
                cwd: loggerd_directory.to_owned(),
                argv: vec![binary.into_os_string()],
            }
            .spawn_inherited_with_env(&[(
                "PARAMS_COPY_PATH".into(),
                self.path.clone().into_os_string(),
            )])?;
            child.process.wait()?;
        }
        fs::remove_dir_all(self.path)?;
        Ok(())
    }

    pub fn launch(
        self,
        loggerd_directory: PathBuf,
        launcher: PathBuf,
    ) -> io::Result<JoinHandle<Result<(), Error>>> {
        // Dropping the handle detaches the worker; Rust does not join it at process exit.
        thread::Builder::new()
            .name("bootlog".into())
            .spawn(move || {
                let result = self.run(&loggerd_directory, &launcher);
                if let Err(error) = &result {
                    eprintln!("bootlog snapshot worker: {error}");
                }
                result
            })
    }
}

pub fn save_bootlog(
    params_directory: &Path,
    loggerd_directory: &Path,
    launcher: &Path,
) -> Result<JoinHandle<Result<(), Error>>, Error> {
    let snapshot = Snapshot::capture(params_directory, &std::env::temp_dir())?;
    Ok(snapshot.launch(loggerd_directory.to_owned(), launcher.to_owned())?)
}

fn copy_tree(source: &Path, destination: &Path, failures: &mut Vec<(PathBuf, String)>) {
    let mut operation = || -> io::Result<()> {
        let entries = fs::read_dir(source)?.collect::<io::Result<Vec<_>>>()?;
        fs::create_dir_all(destination)?;
        for entry in entries {
            let source = entry.path();
            let target = destination.join(entry.file_name());
            let metadata = match fs::metadata(&source) {
                Ok(metadata) => metadata,
                Err(error) => {
                    failures.push((source, error.to_string()));
                    continue;
                }
            };
            if metadata.is_dir() {
                copy_tree(&source, &target, failures);
            } else {
                let result = if metadata.file_type().is_fifo() {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "named pipe in Params copy",
                    ))
                } else {
                    copy_file(&source, &target)
                        .and_then(|_| copy_stat(&source, &target, &fs::metadata(&source)?))
                };
                if let Err(error) = result {
                    failures.push((source, error.to_string()));
                }
            }
        }
        copy_stat(source, destination, &fs::metadata(source)?)
    };
    if let Err(error) = operation() {
        failures.push((source.to_owned(), error.to_string()));
    }
}

fn copy_file(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = File::open(source)?;
    let result = (|| {
        let mut output = File::create(destination)?;
        let copied = io::copy(&mut input, &mut output);
        nix::unistd::close(output)?;
        copied.map(|_| ())
    })();
    nix::unistd::close(input)?;
    result
}

fn copy_stat(source: &Path, destination: &Path, metadata: &Metadata) -> io::Result<()> {
    let times = rustix::fs::Timestamps {
        last_access: rustix::fs::Timespec {
            tv_sec: metadata.atime(),
            tv_nsec: metadata.atime_nsec(),
        },
        last_modification: rustix::fs::Timespec {
            tv_sec: metadata.mtime(),
            tv_nsec: metadata.mtime_nsec(),
        },
    };
    rustix::fs::utimensat(
        rustix::fs::CWD,
        destination,
        &times,
        rustix::fs::AtFlags::empty(),
    )?;
    copy_xattrs(source, destination)?;
    fs::set_permissions(destination, metadata.permissions())
}

fn copy_xattrs(source: &Path, destination: &Path) -> io::Result<()> {
    use rustix::{
        fs::{getxattr, listxattr, setxattr, XattrFlags},
        io::Errno,
    };
    let mut names = vec![0u8; 65536];
    let size = match listxattr(source, names.as_mut_slice()) {
        Ok(size) => size,
        Err(Errno::NOTSUP | Errno::NODATA | Errno::INVAL) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for name in names[..size]
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let mut value = vec![0u8; 65536];
        let result =
            getxattr(source, OsStr::from_bytes(name), value.as_mut_slice()).and_then(|size| {
                setxattr(
                    destination,
                    OsStr::from_bytes(name),
                    &value[..size],
                    XattrFlags::empty(),
                )
            });
        match result {
            Ok(())
            | Err(Errno::PERM | Errno::NOTSUP | Errno::NODATA | Errno::INVAL | Errno::ACCESS) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
