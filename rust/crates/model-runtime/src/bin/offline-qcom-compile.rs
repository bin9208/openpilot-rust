use std::{
    io::{Read, Write},
    path::PathBuf,
};

// Internal ordered mode only. Poll deadlines cover the entire frame, including
// partial headers/payloads; raw source compilation and final v1/v2 are unchanged.
fn export_stream() -> Result<(), Box<dyn std::error::Error>> {
    use openpilot_model_runtime::compiler_schedule::export::{
        StreamFrame, StreamSession, MAX_MESSAGE, MAX_STREAM_FRAMES,
    };
    use std::{
        fs::File,
        io,
        os::fd::{AsFd, AsRawFd},
        time::{Duration, Instant},
    };
    const IDLE: Duration = Duration::from_secs(900);
    fn ready(file: &File, events: i16, deadline: Instant) -> io::Result<()> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "export stream frame timeout",
                ));
            }
            let mut fd = libc::pollfd {
                fd: file.as_raw_fd(),
                events,
                revents: 0,
            };
            // SAFETY: one initialized pollfd is valid for the duration of poll.
            let result = unsafe {
                libc::poll(
                    &mut fd,
                    1,
                    remaining.as_millis().max(1).min(i32::MAX as u128) as i32,
                )
            };
            if result > 0 {
                if fd.revents & libc::POLLNVAL != 0 {
                    return Err(io::Error::other("invalid export stream FD"));
                }
                return Ok(());
            }
            if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(io::Error::last_os_error());
            }
        }
    }
    fn read_all(file: &mut File, mut bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
        while !bytes.is_empty() {
            ready(file, libc::POLLIN, deadline)?;
            match file.read(bytes) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "incomplete export stream frame",
                    ))
                }
                Ok(count) => bytes = &mut bytes[count..],
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    fn write_all(file: &mut File, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
        while !bytes.is_empty() {
            ready(file, libc::POLLOUT, deadline)?;
            match file.write(bytes) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "export stream write stopped",
                    ))
                }
                Ok(count) => bytes = &bytes[count..],
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    let mut input: File = std::io::stdin().as_fd().try_clone_to_owned()?.into();
    let mut output: File = std::io::stdout().as_fd().try_clone_to_owned()?.into();
    for file in [&input, &output] {
        // SAFETY: these live owned FDs are used only by this stdio protocol.
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return Err(io::Error::last_os_error().into());
        }
    }
    let mut session = StreamSession::default();
    let (mut incoming, mut outgoing) = (0usize, 0usize);
    for _ in 0..MAX_STREAM_FRAMES {
        let deadline = Instant::now() + IDLE;
        let mut header = [0; 4];
        read_all(&mut input, &mut header, deadline)?;
        let length = u32::from_be_bytes(header) as usize;
        incoming = incoming
            .checked_add(4 + length)
            .filter(|n| *n <= MAX_MESSAGE)
            .ok_or("export stream input exceeds bound")?;
        if length == 0 {
            return Err("empty export stream frame".into());
        }
        let mut raw = vec![0; length];
        read_all(&mut input, &mut raw, deadline)?;
        let frame: StreamFrame = serde_json::from_slice(&raw)?;
        let reply = session.handle(frame).map_err(io::Error::other)?;
        let final_reply = reply.is_final();
        if final_reply {
            // Final cannot escape before explicit EOF. Trailing input is fatal.
            loop {
                ready(&input, libc::POLLIN, deadline)?;
                match input.read(&mut [0; 1]) {
                    Ok(0) => break,
                    Ok(_) => return Err("trailing export stream input".into()),
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                        ) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let raw = serde_json::to_vec(&reply)?;
        outgoing = outgoing
            .checked_add(4 + raw.len())
            .filter(|n| *n <= MAX_MESSAGE)
            .ok_or("export stream output exceeds bound")?;
        let deadline = Instant::now() + IDLE;
        write_all(&mut output, &(raw.len() as u32).to_be_bytes(), deadline)?;
        write_all(&mut output, &raw, deadline)?;
        if final_reply {
            return Ok(());
        }
    }
    Err("export stream frame count exceeded".into())
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let library = match args.next() {
        Some(key) if key == "--library" => {
            PathBuf::from(args.next().ok_or("--library requires a path")?)
        }
        _ => return Err(
            "usage: offline-qcom-compile --library PINNED_LIBLLVM_QCOM [--export-plan|--export-stream] < input > output"
                .into(),
        ),
    };
    let mode = match args.next() {
        None => 0,
        Some(flag) if flag == "--export-plan" => 1,
        Some(flag) if flag == "--export-stream" => 2,
        _ => return Err("unexpected argument".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    if mode == 2 {
        return export_stream();
    }
    if mode == 1 {
        use openpilot_model_runtime::compiler_schedule::export::{prepare, Request, MAX_MESSAGE};
        let mut request = Vec::new();
        std::io::stdin()
            .take(MAX_MESSAGE as u64 + 1)
            .read_to_end(&mut request)?;
        if request.len() > MAX_MESSAGE {
            return Err("export-plan input exceeds bound".into());
        }
        let request: Request = serde_json::from_slice(&request)?;
        let output = serde_json::to_vec(&prepare(request).map_err(std::io::Error::other)?)?;
        if output.len() > MAX_MESSAGE {
            return Err("export-plan output exceeds bound".into());
        }
        std::io::stdout().write_all(&output)?;
        return Ok(());
    }
    let mut source = String::new();
    std::io::stdin()
        .take(64 * 1024 * 1024 + 1)
        .read_to_string(&mut source)?;
    let output =
        openpilot_model_runtime::qcom_compiler::Compiler::open(&library)?.compile(&source)?;
    std::io::stdout().write_all(&output)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("offline QCOM compiler: {error}");
        std::process::exit(1);
    }
}
