//! Native synthetic journalctl process. Its only journal behavior is forwarding
//! commanded bytes; the independent checker still runs the real daemon.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::net::UnixStream,
    path::Path,
};
struct Wake {
    read: UnixStream,
    registration: signal_hook::SigId,
}
impl Wake {
    fn new() -> Result<Self> {
        let (read, write) = UnixStream::pair()?;
        let registration =
            signal_hook::low_level::pipe::register(signal_hook::consts::SIGTERM, write)?;
        Ok(Self { read, registration })
    }
    fn pending(&self) -> Result<bool> {
        let mut fds = [PollFd::new(&self.read, PollFlags::IN)];
        Ok(poll(
            &mut fds,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        )? > 0)
    }
    fn wait(&self, fd: impl std::os::fd::AsFd, flags: PollFlags) -> Result<bool> {
        loop {
            let mut fds = [
                PollFd::new(&self.read, PollFlags::IN),
                PollFd::new(&fd, flags),
            ];
            match poll(&mut fds, None) {
                Ok(_) => return Ok(!fds[0].revents().is_empty()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
}
impl Drop for Wake {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.registration);
    }
}
fn monotonic() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    (t.tv_sec as u64) * 1_000_000_000 + t.tv_nsec as u64
}
fn record(
    trace: &Path,
    event: &str,
    argv: &[String],
    hook: &mut dyn FnMut(&str) -> Result<()>,
) -> Result<()> {
    let value = serde_json::json!({"event":event,"pid":std::process::id(),"ppid":rustix::process::getppid().map_or(0, |pid| pid.as_raw_nonzero().get()),"monotonic_ns":monotonic(),"argv":argv});
    // No buffered writer can be reentered by a signal handler. The handler only
    // writes its self-pipe; termination is recorded by the owning main loop.
    writeln!(
        OpenOptions::new().create(true).append(true).open(trace)?,
        "{value}"
    )?;
    hook(event)
}
fn bad_descriptor() -> Box<dyn std::error::Error> {
    std::io::Error::from_raw_os_error(9).into()
}
// Python b64decode(validate=False): ignore nonalphabet ASCII, permit leading
// and extra padding, and stop at the first completed padding quantum.
fn decode_base64(text: &str) -> Result<Vec<u8>> {
    if !text.is_ascii() {
        return Err("base64 string must contain only ASCII".into());
    }
    let mut result = vec![];
    let mut quad = 0u8;
    let mut pads = 0u8;
    let mut bits = 0u8;
    let mut pending = 0u32;
    for byte in text.bytes() {
        if byte == b'=' {
            if quad >= 2 {
                pads += 1;
                if quad + pads >= 4 {
                    return Ok(result);
                }
            }
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        pads = 0;
        pending = (pending << 6) | u32::from(value);
        bits += 6;
        quad = (quad + 1) % 4;
        if bits >= 8 {
            bits -= 8;
            result.push((pending >> bits) as u8);
            pending &= (1 << bits) - 1;
        }
    }
    if quad != 0 {
        return Err("invalid base64 padding or data length".into());
    }
    Ok(result)
}
/// Hook is an ordinary callback for a test-owned process, never an environment
/// switch in the production fixture. It reproduces SIGTERM during trace append.
pub fn run_with_hook(
    trace: &Path,
    argv: &[String],
    hook: &mut dyn FnMut(&str) -> Result<()>,
) -> Result<i32> {
    if argv != ["-f", "-o", "json"] {
        return Err("journalctl fixture requires -f -o json".into());
    }
    let wake = Wake::new()?;
    let input = std::io::stdin();
    let output = std::io::stdout();
    let flags = fcntl_getfl(&output)?;
    fcntl_setfl(&output, flags | OFlags::NONBLOCK)?;
    record(trace, "started", argv, hook)?;
    let mut pending = vec![];
    let mut eof = false;
    let mut closed = false;
    loop {
        if wake.pending()? {
            record(trace, "signal-15", argv, hook)?;
            return Ok(0);
        }
        let line = if let Some(end) = pending.iter().position(|b| *b == b'\n') {
            let mut line = pending.drain(..=end).collect::<Vec<_>>();
            line.pop();
            Some(line)
        } else if eof {
            if pending.is_empty() {
                record(trace, "stdin-eof", argv, hook)?;
                return Ok(0);
            }
            Some(std::mem::take(&mut pending))
        } else {
            None
        };
        let Some(line) = line else {
            if wake.wait(&input, PollFlags::IN)? {
                continue;
            }
            let mut bytes = [0; 8192];
            match rustix::io::read(&input, &mut bytes) {
                Ok(0) => eof = true,
                Ok(count) => pending.extend_from_slice(&bytes[..count]),
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
            }
            continue;
        };
        let command = JsonValue::parse(std::str::from_utf8(&line)?)?;
        let operation = command
            .get("op")
            .and_then(|v| v.to_utf8())
            .ok_or("journal fixture missing string operation")?;
        match operation.as_str() {
            "write" => {
                let encoded = command
                    .get("base64")
                    .and_then(|v| v.to_utf8())
                    .ok_or("journal fixture base64 must be text")?;
                let payload = decode_base64(&encoded)?;
                let mut at = 0;
                while at < payload.len() {
                    if wake.pending()? {
                        record(trace, "signal-15", argv, hook)?;
                        return Ok(0);
                    }
                    if closed {
                        return Err(bad_descriptor());
                    }
                    if wake.wait(&output, PollFlags::OUT)? {
                        continue;
                    }
                    match rustix::io::write(&output, &payload[at..]) {
                        Ok(count) => at += count,
                        Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            "close" => {
                if closed {
                    return Err(bad_descriptor());
                }
                // Keep Rust's standard descriptor valid while ending the
                // external pipe and preserving subsequent EBADF semantics.
                rustix::stdio::dup2_stdout(File::open("/dev/null")?)?;
                closed = true;
                record(trace, "stdout-closed", argv, hook)?;
            }
            "exit" => {
                record(trace, "exited", argv, hook)?;
                let value = command
                    .get("status")
                    .ok_or("journal fixture exit status missing")?;
                return match value.view() {
                    JsonView::Null => Ok(0),
                    JsonView::Bool(value) => Ok(i32::from(value)),
                    // CPython PyLong_AsLong uses -1 on C-long overflow;
                    // process exit keeps the low byte of all in-range values.
                    JsonView::Integer(value) => Ok(value.parse::<i64>().map_or(-1, |v| v as i32)),
                    _ => {
                        eprintln!("{}", value.to_json()?);
                        Ok(1)
                    }
                };
            }
            _ => return Err("unknown journal fixture operation".into()),
        }
    }
}
pub fn cli() -> Result<i32> {
    let trace =
        std::env::var_os("JOURNAL_FIXTURE_TRACE").ok_or("JOURNAL_FIXTURE_TRACE required")?;
    run_with_hook(
        Path::new(&trace),
        &std::env::args().skip(1).collect::<Vec<_>>(),
        &mut |_| Ok(()),
    )
}
