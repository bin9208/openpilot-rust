use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    Output,
    Sleep,
    CloseSleep,
    Signal,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Behavior {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: u8,
    padding: usize,
    mode: Mode,
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let directory =
        PathBuf::from(std::env::var_os("CHECKOUT_GIT_FIXTURE").ok_or("missing fixture directory")?);
    let behavior: Behavior = serde_json::from_slice(&fs::read(directory.join("behavior.json"))?)?;
    let record = serde_json::json!({
        "pid": std::process::id(),
        "cwd": std::env::current_dir()?,
        "argv": std::env::args_os().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
    });
    let mut journal = OpenOptions::new()
        .append(true)
        .create(true)
        .open(directory.join("calls.jsonl"))?;
    journal.write_all(format!("{record}\n").as_bytes())?;
    match behavior.mode {
        Mode::Output | Mode::Signal => {
            let padding = vec![b' '; behavior.padding];
            std::io::stdout().write_all(&padding)?;
            std::io::stdout().write_all(&behavior.stdout)?;
            std::io::stderr().write_all(&padding)?;
            std::io::stderr().write_all(&behavior.stderr)?;
            if matches!(behavior.mode, Mode::Signal) {
                rustix::process::kill_process(
                    rustix::process::getpid(),
                    rustix::process::Signal::TERM,
                )?;
            }
        }
        Mode::Sleep => std::thread::sleep(Duration::from_secs(30)),
        Mode::CloseSleep => {
            let null = OpenOptions::new().write(true).open("/dev/null")?;
            rustix::stdio::dup2_stdout(&null)?;
            rustix::stdio::dup2_stderr(&null)?;
            std::thread::sleep(Duration::from_secs(30));
        }
    }
    Ok(behavior.exit_code)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("Git fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
