use crate::Error;
use std::{
    ffi::{CString, OsStr, OsString},
    io,
    path::Path,
};

fn exec_at(path: &Path, arguments: &[CString], environment: &[CString]) -> io::Error {
    let path = match CString::new(path.as_os_str().as_encoded_bytes()) {
        Ok(path) => path,
        Err(error) => return io::Error::new(io::ErrorKind::InvalidInput, error),
    };
    match nix::unistd::execve(&path, arguments, environment) {
        Ok(never) => match never {},
        Err(error) => error.into(),
    }
}

pub(crate) fn environment(name: Option<&str>) -> Result<Vec<CString>, Error> {
    let mut environment = Vec::new();
    for (key, mut value) in std::env::vars_os() {
        if key == OsStr::new("MANAGER_DAEMON") {
            if let Some(name) = name {
                value = name.into();
            }
        }
        let mut entry = key;
        entry.push("=");
        entry.push(value);
        environment.push(CString::new(entry.as_encoded_bytes())?);
    }
    if std::env::var_os("MANAGER_DAEMON").is_none() {
        if let Some(name) = name {
            environment.push(CString::new(format!("MANAGER_DAEMON={name}"))?);
        }
    }
    Ok(environment)
}

pub(crate) fn validate_arguments(arguments: &[OsString]) -> Result<Vec<CString>, Error> {
    if arguments.is_empty() {
        return Err(Error::EmptyCommand);
    }
    Ok(arguments
        .iter()
        .map(|arg| CString::new(arg.as_encoded_bytes()))
        .collect::<Result<_, _>>()?)
}

pub(crate) fn execute(arguments: &[OsString], environment: &[CString]) -> Result<(), Error> {
    let program = arguments.first().ok_or(Error::EmptyCommand)?;
    let argv = validate_arguments(arguments)?;
    if program.as_encoded_bytes().contains(&b'/') {
        return Err(exec_at(Path::new(program), &argv, environment).into());
    }
    let paths = std::env::var_os("PATH").unwrap_or_else(|| "/bin:/usr/bin".into());
    let mut last = io::Error::from(io::ErrorKind::NotFound);
    let mut first_other = None;
    for directory in std::env::split_paths(&paths) {
        let error = exec_at(&directory.join(program), &argv, environment);
        if !matches!(
            error.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
        ) && first_other.is_none()
        {
            first_other = Some(error);
        } else {
            last = error;
        }
    }
    Err(first_other.unwrap_or(last).into())
}
