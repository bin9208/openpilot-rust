use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    io::Read,
    os::unix::{ffi::OsStringExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

#[derive(Clone, Debug)]
pub struct NativeCommand {
    pub launcher: PathBuf,
    pub basedir: PathBuf,
    pub cwd: PathBuf,
    pub argv: Vec<OsString>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum LaunchRequest {
    Native {
        name: String,
        cwd: Vec<u8>,
        argv: Vec<Vec<u8>>,
    },
    Persistent {
        argv: Vec<Vec<u8>>,
    },
}

impl NativeCommand {
    pub(crate) fn spawn(&self, name: &str) -> Result<ChildHandle, Error> {
        let request = LaunchRequest::Native {
            name: name.into(),
            cwd: self.basedir.join(&self.cwd).into_os_string().into_vec(),
            argv: self
                .argv
                .iter()
                .map(|arg| arg.as_encoded_bytes().into())
                .collect(),
        };
        let mut descriptor = tempfile::NamedTempFile::new()?;
        serde_json::to_writer(descriptor.as_file_mut(), &request)?;
        let process = Command::new(&self.launcher)
            .arg(descriptor.path())
            .spawn()?;
        Ok(ChildHandle {
            process,
            _descriptor: descriptor,
        })
    }
}

pub(crate) struct ChildHandle {
    pub process: Child,
    _descriptor: tempfile::NamedTempFile,
}

pub fn run_child(input: impl Read) -> Result<(), Error> {
    let request: LaunchRequest = serde_json::from_reader(input)?;
    match request {
        LaunchRequest::Native { name, cwd, argv } => {
            let environment = crate::exec::environment(Some(&name))?;
            std::env::set_current_dir(PathBuf::from(OsString::from_vec(cwd)))?;
            let arguments: Vec<_> = argv.into_iter().map(OsString::from_vec).collect();
            crate::exec::execute(&arguments, &environment)
        }
        LaunchRequest::Persistent { argv } => {
            let arguments: Vec<_> = argv.into_iter().map(OsString::from_vec).collect();
            crate::detached_child::run(&arguments)
        }
    }
}

pub(crate) fn spawn_persistent(launcher: &Path, arguments: &[OsString]) -> Result<Child, Error> {
    crate::exec::validate_arguments(arguments)?;
    let request = LaunchRequest::Persistent {
        argv: arguments
            .iter()
            .map(|arg| arg.as_encoded_bytes().into())
            .collect(),
    };
    let mut descriptor = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(descriptor.as_file_mut(), &request)?;
    let mut child = Command::new(launcher)
        .arg(descriptor.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    let Some(pipe) = child.stdout.take() else {
        child.kill()?;
        child.wait()?;
        return Err(Error::LaunchProtocol("missing error pipe"));
    };
    let mut report = Vec::new();
    if let Err(error) = pipe.take(5).read_to_end(&mut report) {
        child.kill()?;
        child.wait()?;
        return Err(error.into());
    }
    if report.is_empty() {
        return Ok(child);
    }
    if report.len() != 4 {
        child.kill()?;
        child.wait()?;
        return Err(Error::LaunchProtocol("invalid error packet"));
    }
    child.wait()?;
    let bytes = report
        .try_into()
        .map_err(|_| Error::LaunchProtocol("invalid errno width"))?;
    Err(std::io::Error::from_raw_os_error(i32::from_ne_bytes(bytes)).into())
}
