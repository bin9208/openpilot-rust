use openpilot_navd::{
    native::{self, Options},
    Error,
};
use std::{env, process::ExitCode};

fn options() -> Result<Option<Options>, Error> {
    let mut options = Options::default();
    let mut args = env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help") if args.next().is_none() => {
                println!("openpilot-navd [--frames N] [--mapbox-host URL] [--persist-root DIR]");
                return Ok(None);
            }
            Some("--frames") => {
                options.frames = Some(
                    args.next()
                        .and_then(|value| value.into_string().ok())
                        .ok_or(Error::Runtime("missing frame count"))?
                        .parse()
                        .map_err(|_| Error::Runtime("frames must be a positive integer"))?,
                );
            }
            Some("--mapbox-host") => {
                let host = args
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or(Error::Runtime("missing HTTP host"))?;
                let url = url::Url::parse(&host)?;
                if !matches!(url.scheme(), "http" | "https") {
                    return Err(Error::Runtime("HTTP host must use http or https"));
                }
                options.mapbox_host = Some(host);
            }
            Some("--persist-root") => {
                options.persist_root = Some(
                    args.next()
                        .ok_or(Error::Runtime("missing persist root"))?
                        .into(),
                )
            }
            _ => return Err(Error::Runtime("unknown arguments; see --help")),
        }
    }
    Ok(Some(options))
}

fn main() -> ExitCode {
    match options().and_then(|options| options.map_or(Ok(()), native::run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("navd: {error}");
            ExitCode::FAILURE
        }
    }
}
