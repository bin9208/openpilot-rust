use crate::{Error, Value};
use std::{os::unix::process::ExitStatusExt, process::Command};

#[derive(Clone, Debug)]
pub enum Invocation {
    Argv(Vec<String>),
    Shell(String),
}

impl Invocation {
    pub fn repr(&self) -> Result<String, Error> {
        match self {
            Self::Argv(args) => {
                Ok(Value::Array(args.iter().map(|arg| Value::text(arg)).collect()).repr()?)
            }
            Self::Shell(command) => Ok(command.clone()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("{0}")]
    Exit(String),
    #[error(transparent)]
    Boundary(#[from] Error),
}

pub fn run(invocation: &Invocation) -> Result<(), Failure> {
    let mut command = match invocation {
        Invocation::Argv(args) => {
            let program = args
                .first()
                .ok_or_else(|| Error::Source("empty command".into()))?;
            let mut command = Command::new(program);
            command.args(&args[1..]);
            command
        }
        Invocation::Shell(text) => {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", text]);
            command
        }
    };
    let status = command.status().map_err(|error| {
        let filename = match invocation {
            Invocation::Argv(args) => args[0].as_str(),
            Invocation::Shell(_) => "/bin/sh",
        };
        Failure::Boundary(crate::state::io_error(
            error,
            std::path::Path::new(filename),
        ))
    })?;
    if status.success() {
        return Ok(());
    }
    let command = invocation.repr()?;
    let message = if let Some(signal) = status.signal() {
        let name = match signal {
            1 => Some("HUP"),
            2 => Some("INT"),
            3 => Some("QUIT"),
            4 => Some("ILL"),
            5 => Some("TRAP"),
            6 => Some("ABRT"),
            7 => Some("BUS"),
            8 => Some("FPE"),
            9 => Some("KILL"),
            10 => Some("USR1"),
            11 => Some("SEGV"),
            12 => Some("USR2"),
            13 => Some("PIPE"),
            14 => Some("ALRM"),
            15 => Some("TERM"),
            16 => Some("STKFLT"),
            17 => Some("CHLD"),
            18 => Some("CONT"),
            19 => Some("STOP"),
            20 => Some("TSTP"),
            21 => Some("TTIN"),
            22 => Some("TTOU"),
            23 => Some("URG"),
            24 => Some("XCPU"),
            25 => Some("XFSZ"),
            26 => Some("VTALRM"),
            27 => Some("PROF"),
            28 => Some("WINCH"),
            29 => Some("IO"),
            30 => Some("PWR"),
            31 => Some("SYS"),
            34 => Some("RTMIN"),
            64 => Some("RTMAX"),
            _ => None,
        };
        match name {
            Some(name) => format!("Command '{command}' died with <Signals.SIG{name}: {signal}>."),
            None => format!("Command '{command}' died with unknown signal {signal}."),
        }
    } else {
        format!(
            "Command '{command}' returned non-zero exit status {}.",
            status.code().unwrap_or(1)
        )
    };
    Err(Failure::Exit(message))
}
