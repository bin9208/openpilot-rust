//! persistent stdio client. 응답은 원본 ASCII decode→float→finite 단계의 문법을 검사한다.
use super::scheduler;
use crate::seal::Result;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, Pid, Signal},
};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::{fd::AsFd, unix::process::ExitStatusExt},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

const REQUEST: Duration = Duration::from_secs(2);
const CLOSE: Duration = Duration::from_secs(3);

pub struct SchedulerClient {
    child: Option<Child>,
    input: Option<ChildStdin>,
    output: Option<ChildStdout>,
    stderr: Option<File>,
    selected: bool,
    failed: bool,
    closed: bool,
    status: Option<i32>,
    requests: usize,
}

fn timed_out() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "native scheduler response exceeded 2 seconds",
    )
}

fn ready(fd: &impl AsFd, reading: bool, deadline: Instant) -> Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(timed_out)?;
        if remaining.is_zero() {
            return Err(timed_out().into());
        }
        let timeout = Timespec {
            tv_sec: remaining.as_secs().try_into()?,
            tv_nsec: remaining.subsec_nanos().into(),
        };
        let mut descriptors = [PollFd::new(
            fd,
            if reading {
                PollFlags::IN
            } else {
                PollFlags::OUT
            },
        )];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => return Err(timed_out().into()),
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResponseError {
    NonAscii,
    InvalidFloat,
    NonFinite,
}
impl std::fmt::Display for ResponseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NonAscii => "native scheduler response must be ASCII",
            Self::InvalidFloat => "could not convert scheduler response to float",
            Self::NonFinite => "native scheduler returned a nonfinite target",
        })
    }
}
impl std::error::Error for ResponseError {}

fn python_ascii_space(byte: u8) -> bool {
    // 원본35의 VT/FF를 포함한다. Rust trim_ascii/is_ascii_whitespace의 집합에 의존하지 않는다.
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}
fn digits(bytes: &[u8], position: &mut usize) -> usize {
    let start = *position;
    while bytes.get(*position).is_some_and(u8::is_ascii_digit) {
        *position += 1;
    }
    *position - start
}

/// framing/256-byte budget은 exchange가 소유한다. 여기는 독립 관측35와 같은 parse 단계다.
/// ASCII decimal 문법과 digit 사이 underscore를 검사하고 수치 변환만 std에 맡긴다.
pub fn parse_response(bytes: &[u8]) -> std::result::Result<f64, ResponseError> {
    if !bytes.is_ascii() {
        return Err(ResponseError::NonAscii);
    }
    let start = bytes
        .iter()
        .position(|b| !python_ascii_space(*b))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !python_ascii_space(*b))
        .map_or(start, |i| i + 1);
    let text = &bytes[start..end];
    let sign = usize::from(matches!(text.first().copied(), Some(b'+' | b'-')));
    let unsigned = &text[sign..];
    if unsigned.eq_ignore_ascii_case(b"nan")
        || unsigned.eq_ignore_ascii_case(b"inf")
        || unsigned.eq_ignore_ascii_case(b"infinity")
    {
        return Err(ResponseError::NonFinite);
    }
    let mut normalized = Vec::with_capacity(text.len());
    for (index, byte) in text.iter().enumerate() {
        if *byte == b'_' {
            if index == 0
                || index + 1 == text.len()
                || !text[index - 1].is_ascii_digit()
                || !text[index + 1].is_ascii_digit()
            {
                return Err(ResponseError::InvalidFloat);
            }
        } else {
            normalized.push(*byte);
        }
    }
    let mut position = sign;
    let integer = digits(&normalized, &mut position);
    let fraction = if normalized.get(position) == Some(&b'.') {
        position += 1;
        digits(&normalized, &mut position)
    } else {
        0
    };
    if integer + fraction == 0 {
        return Err(ResponseError::InvalidFloat);
    }
    if matches!(normalized.get(position).copied(), Some(b'e' | b'E')) {
        position += 1;
        if matches!(normalized.get(position).copied(), Some(b'+' | b'-')) {
            position += 1;
        }
        if digits(&normalized, &mut position) == 0 {
            return Err(ResponseError::InvalidFloat);
        }
    }
    if position != normalized.len() {
        return Err(ResponseError::InvalidFloat);
    }
    let target = std::str::from_utf8(&normalized)
        .map_err(|_| ResponseError::NonAscii)?
        .parse::<f64>()
        .map_err(|_| ResponseError::InvalidFloat)?;
    if !target.is_finite() {
        return Err(ResponseError::NonFinite);
    }
    Ok(target)
}

impl SchedulerClient {
    /// None는 process 없는 기존 순서 비교 정책이다. Some(empty)는 오류다.
    /// child는 상위 native QA group을 상속하며 별도 detached group을 만들지 않는다.
    pub fn canonical(binary: Option<&Path>, stderr_path: &Path) -> Result<Self> {
        Self::canonical_recorded(binary, stderr_path, None)
    }

    pub fn canonical_recorded(
        binary: Option<&Path>,
        stderr_path: &Path,
        evidence: Option<&Path>,
    ) -> Result<Self> {
        let mut client = Self {
            child: None,
            input: None,
            output: None,
            stderr: None,
            selected: binary.is_some(),
            failed: false,
            closed: false,
            status: None,
            requests: 0,
        };
        let Some(binary) = binary else {
            return Ok(client);
        };
        if binary.as_os_str().is_empty() {
            return Err("PLANNER_NATIVE_SCHEDULER must name an executable".into());
        }
        let stderr = File::create(stderr_path)?;
        let mut command = Command::new(binary);
        command
            .arg("schedule")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr.try_clone()?));
        if let Some(path) = evidence {
            command.env("PLANNER_NATIVE_SCHEDULER_EVIDENCE", path);
        }
        let child = command.spawn()?;
        client.stderr = Some(stderr);
        client.child = Some(child);
        let configured = (|| -> Result<()> {
            let child = client.child.as_mut().ok_or("missing scheduler child")?;
            client.input = Some(child.stdin.take().ok_or("missing scheduler stdin")?);
            client.output = Some(child.stdout.take().ok_or("missing scheduler stdout")?);
            let input = client.input.as_ref().ok_or("missing scheduler stdin")?;
            let output = client.output.as_ref().ok_or("missing scheduler stdout")?;
            fcntl_setfl(input, fcntl_getfl(input)? | OFlags::NONBLOCK)?;
            fcntl_setfl(output, fcntl_getfl(output)? | OFlags::NONBLOCK)?;
            Ok(())
        })();
        if let Err(error) = configured {
            return client.finish(Err(error));
        }
        Ok(client)
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }
    pub fn status(&self) -> Option<i32> {
        self.status
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn is_closed(&self) -> bool {
        self.closed && self.input.is_none() && self.output.is_none() && self.stderr.is_none()
    }

    pub fn target(&mut self, nominal: f64, sent: f64, done: f64) -> Result<f64> {
        if !self.selected {
            return Ok(scheduler::next_model_target(nominal, sent, done));
        }
        let result = self.exchange(nominal, sent, done);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn exchange(&mut self, nominal: f64, sent: f64, done: f64) -> Result<f64> {
        if self.failed || self.closed {
            return Err("native scheduler is not running".into());
        }
        let child = self
            .child
            .as_mut()
            .ok_or("native scheduler is not running")?;
        if child.try_wait()?.is_some() {
            return Err("native scheduler is not running".into());
        }
        if ![nominal, sent, done].iter().all(|value| value.is_finite()) {
            return Err("native scheduler wire timestamps must be finite".into());
        }
        let deadline = Instant::now() + REQUEST;
        let request = format!("{nominal:?}\t{sent:?}\t{done:?}\n");
        let input = self.input.as_mut().ok_or("scheduler stdin closed")?;
        let mut bytes = request.as_bytes();
        while !bytes.is_empty() {
            ready(input, false, deadline)?;
            match input.write(bytes) {
                Ok(0) => return Err("native scheduler request pipe closed".into()),
                Ok(count) => bytes = &bytes[count..],
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.requests += 1;
        let output = self.output.as_mut().ok_or("scheduler stdout closed")?;
        let mut response = Vec::new();
        loop {
            if response.len() >= 256 {
                return Err("native scheduler response is too long".into());
            }
            ready(output, true, deadline)?;
            let mut chunk = [0; 256];
            let remaining = 256 - response.len();
            match output.read(&mut chunk[..remaining]) {
                Ok(0) => return Err("native scheduler EOF before target".into()),
                Ok(count) => response.extend_from_slice(&chunk[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(error.into()),
            }
            if let Some(newline) = response.iter().position(|byte| *byte == b'\n') {
                if newline + 1 != response.len() {
                    return Err("native scheduler returned multiple responses".into());
                }
                return parse_response(&response[..newline]).map_err(Into::into);
            }
        }
    }

    fn signal(&mut self, signal: Signal) -> Result<()> {
        if let Some(child) = &mut self.child {
            if child.try_wait()?.is_none() {
                let pid = Pid::from_raw(child.id().try_into()?).ok_or("invalid scheduler PID")?;
                match kill_process(pid, signal) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(())
    }

    fn wait_until(&mut self, deadline: Instant) -> Result<Option<i32>> {
        loop {
            let child = self.child.as_mut().ok_or("missing scheduler child")?;
            if let Some(status) = child.try_wait()? {
                let code = status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1));
                self.status = Some(code);
                return Ok(Some(code));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(
                Duration::from_millis(1).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn close(&mut self, parent_failed: bool) -> Result<()> {
        if self.closed {
            return Err("scheduler session already closed".into());
        }
        self.input.take();
        let outcome = (|| -> Result<()> {
            if self.child.is_none() {
                return Ok(());
            }
            let deadline = Instant::now() + CLOSE;
            if parent_failed || self.failed {
                self.signal(Signal::TERM)?;
            }
            let mut forced = false;
            let mut status =
                self.wait_until((Instant::now() + Duration::from_secs(1)).min(deadline))?;
            if status.is_none() {
                forced = true;
                self.signal(Signal::TERM)?;
                status =
                    self.wait_until((Instant::now() + Duration::from_secs(1)).min(deadline))?;
                if status.is_none() {
                    self.signal(Signal::KILL)?;
                    status = self.wait_until(deadline)?;
                }
            }
            let status =
                status.ok_or("native scheduler could not be reaped before close deadline")?;
            if !parent_failed && (self.failed || forced || status != 0) {
                return Err(format!("native scheduler did not close normally: {status}").into());
            }
            Ok(())
        })();
        self.output.take();
        self.stderr.take();
        self.closed = true;
        outcome
    }

    /// cleanup 자체가 성공한 경우 원래 부모 오류를 유지한다. Drop은 오류 반환을 대체하지 않는다.
    pub fn finish<T>(&mut self, work: Result<T>) -> Result<T> {
        self.close(work.is_err())?;
        work
    }
}

impl Drop for SchedulerClient {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.close(true);
        }
        if let Some(child) = &mut self.child {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.try_wait();
            }
        }
    }
}
