use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Arguments(&'static str),
    #[error("invalid OPENPILOT_PREFIX: {0}")]
    Prefix(#[from] env::VarError),
}
pub struct Options {
    pub root: PathBuf,
    pub endpoint: String,
    pub frames: Option<u64>,
}
impl Options {
    pub fn parse(arguments: impl Iterator<Item = OsString>) -> Result<Option<Self>, Error> {
        let mut arguments = arguments;
        let mut root = None;
        let mut endpoint = None;
        let mut frames = None;
        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--help") => {
                    println!("openpilot-logmessaged [--frames N] [--log-root DIR] [--endpoint IPC]\n\nRuns the continuous original ZMQ-to-swaglog/cereal collector. Omit --frames\nto run continuously. Production manager selection remains unchanged.\nDefaults follow /TICI, HOME and OPENPILOT_PREFIX; explicit paths are host QA overrides.");
                    return Ok(None);
                }
                Some("--log-root") if root.is_none() => {
                    root = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or(Error::Arguments("missing log root"))?,
                    ))
                }
                Some("--endpoint") if endpoint.is_none() => {
                    endpoint = Some(
                        arguments
                            .next()
                            .ok_or(Error::Arguments("missing endpoint"))?
                            .into_string()
                            .map_err(|_| Error::Arguments("endpoint must be UTF-8"))?,
                    )
                }
                Some("--frames") if frames.is_none() => {
                    let value = arguments
                        .next()
                        .ok_or(Error::Arguments("missing frame count"))?;
                    let count = value
                        .to_str()
                        .and_then(|value| value.parse::<u64>().ok())
                        .filter(|&count| count > 0)
                        .ok_or(Error::Arguments("frame count must be positive"))?;
                    frames = Some(count);
                }
                _ => return Err(Error::Arguments("unknown or duplicate option; see --help")),
            }
        }
        let prefix = match env::var("OPENPILOT_PREFIX") {
            Ok(value) => value,
            Err(env::VarError::NotPresent) => String::new(),
            Err(error) => return Err(error.into()),
        };
        let root = match root {
            Some(path) => path,
            None if Path::new("/TICI").is_file() => PathBuf::from("/data/log/"),
            None => home::home_dir()
                .ok_or(Error::Arguments("home directory is unavailable"))?
                .join(format!(".comma{prefix}"))
                .join("log"),
        };
        Ok(Some(Self {
            root,
            endpoint: endpoint.unwrap_or_else(|| format!("ipc:///tmp/logmessage{prefix}")),
            frames,
        }))
    }
}
