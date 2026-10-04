use crate::Error;
use std::{num::NonZeroU64, path::PathBuf};

pub struct Options {
    pub numerics: PathBuf,
    pub dbc: PathBuf,
    pub steps: Option<NonZeroU64>,
    pub fixture: Option<super::FixturePaths>,
}

pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Options>, Error> {
    let executable = std::env::current_exe()?;
    let mut options = Options {
        numerics: std::env::var_os("RADARCAN_NUMERICS")
            .map(PathBuf::from)
            .unwrap_or(executable.with_file_name("radarcan-numerics")),
        dbc: std::env::var_os("RADARCAN_DBC")
            .map(PathBuf::from)
            .unwrap_or(executable.with_file_name("radarcan-dbc")),
        steps: None,
        fixture: None,
    };
    let mut ready = None;
    let mut start = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" => {
                println!("openpilot-radarcan [--numerics DIRECTORY] [--dbc DIRECTORY] [--steps N]\nNative non-conflated CAN/carState radar preprocessing.\nRequires pinned NumPy/OpenBLAS and original DBC assets.\n--steps bounds poll iterations for host QA.\n--fixture-constructor-ready PATH --fixture-constructor-start PATH require --steps; pause setup after constructor before initial timers. SIGINT/SIGTERM stop waiting and polling.");
                return Ok(None);
            }
            "--numerics" => {
                options.numerics = arguments
                    .next()
                    .ok_or(Error::Contract("missing numerical directory"))?
                    .into()
            }
            "--dbc" => {
                options.dbc = arguments
                    .next()
                    .ok_or(Error::Contract("missing DBC directory"))?
                    .into()
            }
            "--steps" => {
                options.steps = Some(
                    arguments
                        .next()
                        .ok_or(Error::Contract("missing step count"))?
                        .parse()
                        .map_err(|_| Error::Contract("step count must be positive"))?,
                )
            }
            "--fixture-constructor-ready" => {
                ready = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or(Error::Contract("missing fixture ready path"))?,
                ))
            }
            "--fixture-constructor-start" => {
                start = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or(Error::Contract("missing fixture start path"))?,
                ))
            }
            _ => return Err(Error::Contract("unknown radarcan argument; see --help")),
        }
    }
    options.fixture = match (ready, start, options.steps) {
        (None, None, _) => None,
        (Some(ready), Some(start), Some(_)) if ready != start => {
            Some(super::FixturePaths { ready, start })
        }
        _ => {
            return Err(Error::Contract(
                "constructor fixture requires distinct ready/start paths and --steps",
            ))
        }
    };
    Ok(Some(options))
}
