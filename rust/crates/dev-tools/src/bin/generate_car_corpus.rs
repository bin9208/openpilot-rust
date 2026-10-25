use openpilot_dev_tools::kalman_corpus_generator::{generate, parse_options, success_line, Model};
use std::{io::Write, process::ExitCode};

fn main() -> ExitCode {
    match parse_options(
        std::env::args_os().skip(1),
        "rust/crates/paramsd/tests/data/model.json",
    ) {
        Ok(Some(options)) => match generate(Model::Car, &options) {
            Ok(()) => match std::io::stdout().write_all(&success_line(Model::Car, &options.output))
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("car corpus generator: {error}");
                    ExitCode::FAILURE
                }
            },
            Err(error) => {
                eprintln!("car corpus generator: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(None) => {
            println!("usage: generate-car-corpus --oracle DIRECTORY [--output FILE]");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("car corpus generator: {error}");
            ExitCode::from(2)
        }
    }
}
