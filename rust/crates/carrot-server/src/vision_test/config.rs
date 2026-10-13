use crate::{youtube_test::CommandSpec, Error};
use openpilot_params::Params;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Config {
    pub repository: PathBuf,
    pub state: PathBuf,
    pub log: PathBuf,
    pub params_root: Option<PathBuf>,
    pub launcher: PathBuf,
    pub runner: CommandSpec,
    pub children: [(String, CommandSpec); 3],
    pub port: u16,
}
impl Config {
    pub fn original() -> Result<Self, Error> {
        let binary = std::env::current_exe()?;
        let siblings = binary
            .parent()
            .ok_or_else(|| Error::Source("native executable directory missing".into()))?;
        let runner = siblings.join("openpilot-vision-test");
        let specs = [
            ("camerad", "openpilot-camerad", Vec::new()),
            (
                "stream_encoderd",
                "openpilot-encoderd",
                vec!["--carrot-vision-road".into()],
            ),
            ("webrtcd", "openpilot-carrot-webrtcd", Vec::new()),
        ];
        Ok(Self {
            repository: crate::config::runtime_repository()?,
            state: "/tmp/carrot-vision-test-state.json".into(),
            log: "/tmp/carrot-vision-test.log".into(),
            params_root: None,
            launcher: siblings.join("openpilot-process-child"),
            runner: CommandSpec {
                pattern: runner.to_string_lossy().into_owned(),
                path: runner,
                args: Vec::new(),
            },
            children: specs.map(
                |(name, binary, args): (&str, &str, Vec<std::ffi::OsString>)| {
                    let path = siblings.join(binary);
                    let mut pattern = path.to_string_lossy().into_owned();
                    for arg in &args {
                        pattern.push('\0');
                        pattern.push_str(&arg.to_string_lossy());
                    }
                    (
                        name.into(),
                        CommandSpec {
                            path,
                            args,
                            pattern,
                        },
                    )
                },
            ),
            port: 5001,
        })
    }
    pub(super) fn params(&self) -> Result<Params, Error> {
        Ok(match &self.params_root {
            Some(root) => Params::for_runtime_at(root)?,
            None => Params::for_runtime()?,
        })
    }
    pub(super) fn boolean(&self, name: &str) -> Result<bool, Error> {
        Ok(self
            .params()?
            .get(name)
            .ok()
            .flatten()
            .is_some_and(|bytes| bytes == b"1"))
    }
    pub(super) fn snapshot(&self, active: bool) -> Result<(), Error> {
        crate::youtube_test::cleanup::snapshot_active(
            &self.params()?,
            &self.repository,
            active,
            "vision_test",
        )
    }
}
