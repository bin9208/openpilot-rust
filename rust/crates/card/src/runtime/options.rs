use super::Error;
use std::{ffi::OsString, num::NonZeroU64, path::PathBuf};

pub struct RunOptions {
    pub root: PathBuf,
    pub numerics: PathBuf,
    pub max_steps: Option<NonZeroU64>,
    pub frequency_trace: Option<PathBuf>,
}

pub enum Command {
    Help,
    Run(RunOptions),
}

pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, Error> {
    let mut arguments = arguments.into_iter();
    let mut root = None;
    let mut numerics = None;
    let mut max_steps = None;
    let mut frequency_trace = None;
    while let Some(argument) = arguments.next() {
        let flag = argument
            .to_str()
            .ok_or(Error::Arguments("flag is not Unicode"))?;
        match flag {
            "--help" => return Ok(Command::Help),
            "--root" | "--numerics" => {
                let target = if flag == "--root" {
                    &mut root
                } else {
                    &mut numerics
                };
                if target.is_some() {
                    return Err(Error::Arguments("directory supplied twice"));
                }
                *target = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or(Error::Arguments("missing directory"))?,
                ));
            }
            "--max-steps" => {
                if max_steps.is_some() {
                    return Err(Error::Arguments("step count supplied twice"));
                }
                max_steps = Some(
                    arguments
                        .next()
                        .and_then(|value| value.into_string().ok())
                        .and_then(|value| value.parse::<NonZeroU64>().ok())
                        .ok_or(Error::Arguments("positive integer step count required"))?,
                );
            }
            "--frequency-trace" => {
                if frequency_trace.is_some() {
                    return Err(Error::Arguments("frequency trace supplied twice"));
                }
                frequency_trace = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or(Error::Arguments("missing frequency trace path"))?,
                ));
            }
            _ => return Err(Error::Arguments("unknown argument")),
        }
    }
    if frequency_trace.is_some() && max_steps.is_none() {
        return Err(Error::Arguments(
            "frequency trace requires a fixture step bound",
        ));
    }
    Ok(Command::Run(RunOptions {
        root: root.ok_or(Error::Arguments("--root is required"))?,
        numerics: numerics.ok_or(Error::Arguments("--numerics is required"))?,
        max_steps,
        frequency_trace,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_artifacts_and_fixture_bound_are_typed() {
        let Command::Run(options) = parse(
            [
                "--root",
                "/owned checkout",
                "--numerics",
                "/owned numerics",
                "--max-steps",
                "5",
            ]
            .map(OsString::from),
        )
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(options.root, PathBuf::from("/owned checkout"));
        assert_eq!(options.numerics, PathBuf::from("/owned numerics"));
        assert_eq!(options.max_steps.unwrap().get(), 5);
        for args in [
            vec![],
            vec!["--root"],
            vec!["--root", "/r", "--numerics", "/n", "--max-steps", "0"],
            vec!["--root", "/r", "--root", "/r"],
            vec!["--unknown"],
        ] {
            assert!(parse(args.into_iter().map(OsString::from)).is_err());
        }
    }
}
