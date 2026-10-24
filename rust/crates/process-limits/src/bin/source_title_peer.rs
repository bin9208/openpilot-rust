//! Test-only owned source child: real prepare, then title, then an acknowledged stop.
use openpilot_process_limits::source_title::SourceTitle;
use std::{
    io::{self, BufRead, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
fn phase(output: &mut impl Write, value: &str) -> io::Result<()> {
    writeln!(output, "{value} {}", std::process::id())?;
    output.flush()
}
fn line(input: &mut impl BufRead) -> io::Result<String> {
    let mut value = String::new();
    if input.read_line(&mut value)? == 0 {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    Ok(value.trim_end().into())
}
fn run() -> io::Result<()> {
    // The opt-in startup boundary has already quarantined original argv/env
    // before Rust main or any worker threads. Claim makes no title change.
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "--other-mode") {
        let claimed = SourceTitle::claim();
        let unsupported =
            matches!(&claimed, Err(error) if error.kind() == io::ErrorKind::Unsupported);
        let mut output = io::stdout().lock();
        phase(
            &mut output,
            if unsupported {
                "UNSUPPORTED"
            } else {
                "CLAIMED"
            },
        )?;
        assert_eq!(line(&mut io::BufReader::new(io::stdin().lock()))?, "finish");
        return Ok(());
    }
    assert!(std::thread::spawn(|| SourceTitle::claim().is_err())
        .join()
        .expect("non-main owner probe failed"));
    let mut title = SourceTitle::claim()?;
    assert!(SourceTitle::claim().is_err());
    let mut input = io::BufReader::new(io::stdin().lock());
    let mut output = io::stdout().lock();
    phase(&mut output, "READY")?;
    assert_eq!(line(&mut input)?, "prepare");
    if std::fs::read(&args[1]).is_err() {
        phase(&mut output, "PREPARE_ERROR")?;
        assert_eq!(line(&mut input)?, "finish");
        return Ok(());
    }
    phase(&mut output, "PREPARED")?;
    let length = line(&mut input)?
        .parse::<usize>()
        .map_err(io::Error::other)?;
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    let sentinel = std::env::var_os("QA_SOURCE_TITLE_SENTINEL");
    let done = Arc::new(AtomicBool::new(false));
    let check_done = done.clone();
    let original_args = args.clone();
    let reader = std::thread::spawn(move || {
        // These reads must never alias the mutable kernel title region.
        let mut count = 0;
        while !check_done.load(Ordering::Acquire) || count < 1000 {
            assert_eq!(std::env::args_os().collect::<Vec<_>>(), original_args);
            assert_eq!(std::env::var_os("QA_SOURCE_TITLE_SENTINEL"), sentinel);
            count += 1;
        }
    });
    let named = title.set(&bytes);
    done.store(true, Ordering::Release);
    reader.join().expect("argv/environment reader failed");
    let inherited = std::process::Command::new("/usr/bin/env").output()?;
    assert!(inherited.status.success());
    let expected = std::env::var("QA_SOURCE_TITLE_SENTINEL").map_err(io::Error::other)?;
    let expected = format!("QA_SOURCE_TITLE_SENTINEL={expected}");
    assert!(inherited
        .stdout
        .split(|byte| *byte == b'\n')
        .any(|line| line == expected.as_bytes()));
    phase(
        &mut output,
        if named.is_ok() { "NAMED" } else { "NAME_ERROR" },
    )?;
    if let Err(error) = named {
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
    assert_eq!(line(&mut input)?, "finish");
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
