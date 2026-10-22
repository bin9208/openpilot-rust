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
        let endpoint =
            selected_endpoint(endpoint, &prefix, env::var_os("ROUTE_UPLOAD_QA_IPC_ROOT"))?;
        Ok(Some(Self {
            root,
            endpoint,
            frames,
        }))
    }
}

fn selected_endpoint(
    explicit: Option<String>,
    prefix: &str,
    qa_root: Option<OsString>,
) -> Result<String, Error> {
    match explicit {
        Some(endpoint) => Ok(endpoint),
        None => qa_endpoint(prefix, qa_root),
    }
}
fn qa_endpoint(prefix: &str, qa_root: Option<OsString>) -> Result<String, Error> {
    Ok(match qa_root {
        None => format!("ipc:///tmp/logmessage{prefix}"),
        Some(path) => {
            let path = PathBuf::from(path);
            if path.parent() != Some(Path::new("/tmp"))
                || !path
                    .file_name()
                    .is_some_and(|name| name.as_encoded_bytes().starts_with(b"route-upload-ipc-"))
                || !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir())
            {
                return Err(Error::Arguments("invalid route-upload QA IPC root"));
            }
            format!(
                "ipc://{}/logmessage",
                path.to_str()
                    .ok_or(Error::Arguments("QA IPC root must be UTF-8"))?
            )
        }
    })
}

#[cfg(test)]
mod qa_endpoint_tests {
    use super::*;
    #[test]
    fn explicit_endpoint_precedes_an_invalid_qa_default() {
        assert_eq!(
            selected_endpoint(
                Some("ipc:///explicit-collector".into()),
                "-original",
                Some("/tmp".into())
            )
            .unwrap(),
            "ipc:///explicit-collector"
        );
        assert!(selected_endpoint(None, "-original", Some("/tmp".into())).is_err());
    }
    #[test]
    fn default_and_owned_qa_collector_endpoint_are_explicit_and_invalid_roots_fail() {
        assert_eq!(
            qa_endpoint("-original", None).unwrap(),
            "ipc:///tmp/logmessage-original"
        );
        let root = tempfile::Builder::new()
            .prefix("route-upload-ipc-")
            .tempdir_in("/tmp")
            .unwrap();
        assert_eq!(
            qa_endpoint("-original", Some(root.path().as_os_str().to_owned())).unwrap(),
            format!("ipc://{}/logmessage", root.path().display())
        );
        for path in [
            Path::new("/tmp"),
            Path::new("relative"),
            Path::new("/tmp/route-upload-ipc-missing"),
        ] {
            assert!(qa_endpoint("-original", Some(path.as_os_str().to_owned())).is_err());
        }
        let alias = root.path().with_extension("alias");
        std::os::unix::fs::symlink(root.path(), &alias).unwrap();
        assert!(qa_endpoint("-original", Some(alias.as_os_str().to_owned())).is_err());
        std::fs::remove_file(alias).unwrap();
    }
}
