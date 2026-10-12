use std::{
    collections::BTreeSet,
    io::{self, Read},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) fn names(
    executable: &Path,
    argument: &str,
    encoder: bool,
) -> io::Result<BTreeSet<String>> {
    let mut child = Command::new(executable)
        .args(["-hide_banner", argument])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing ffmpeg probe pipe"))?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let wait = loop {
        match child.try_wait() {
            Ok(Some(_)) => break Ok(()),
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "ffmpeg provider probe timed out",
                ))
            }
            Err(error) => break Err(error),
        }
    };
    if wait.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let bytes = reader
        .join()
        .map_err(|_| io::Error::other("ffmpeg provider reader panicked"))??;
    wait?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let flags = fields.next()?;
            let name = fields.next()?;
            ((encoder && flags.starts_with('V')) || (!encoder && flags.ends_with('E')))
                .then(|| name.to_owned())
        })
        .collect())
}
