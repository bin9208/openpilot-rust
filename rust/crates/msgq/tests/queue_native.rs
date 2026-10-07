#![cfg(feature = "native-skip-miri")]

mod support;

use openpilot_msgq::{Error, Publisher, Subscriber};
use std::{
    env,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    os::unix::fs::FileExt,
    process::Stdio,
    time::Duration,
};

fn child_case(name: &str) -> bool {
    if env::var("IPC194_QUEUE_CASE").as_deref() == Ok(name) {
        return true;
    }
    let namespace = tempfile::Builder::new()
        .prefix("msgq_rust-probe-q194-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = namespace
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let status = support::command(env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("IPC194_QUEUE_CASE", name)
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .status()
        .unwrap();
    assert!(status.success());
    false
}

fn line(output: &mut impl BufRead, expected: &str) {
    let mut actual = String::new();
    output.read_line(&mut actual).unwrap();
    assert_eq!(actual.trim(), expected);
}

fn payload(value: u32, length: usize) -> Vec<u8> {
    let mut bytes: Vec<_> = (0..length)
        .map(|index| (index as u8).wrapping_add(value as u8))
        .collect();
    bytes[..4].copy_from_slice(&value.to_le_bytes());
    bytes
}

#[test]
fn original_peer_preserves_wrap_overflow_eviction_and_restart_semantics() {
    if !child_case("original_peer_preserves_wrap_overflow_eviction_and_restart_semantics") {
        return;
    }
    for conflate in [false, true] {
        let mut outgoing = Publisher::with_capacity("rustToNative", 4096).unwrap();
        let mut command = support::command(env!("NATIVE_MSGQ_PEER"));
        command.arg("--scripted");
        if conflate {
            command.arg("conflate");
        }
        let mut peer = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = peer.stdin.take().unwrap();
        let mut output = BufReader::new(peer.stdout.take().unwrap());
        line(&mut output, "READY");
        let mut incoming = Subscriber::with_capacity("nativeToRust", conflate, 4096).unwrap();
        for value in 0..96_u32 {
            let length = 17 + (value as usize % 5) * 16;
            writeln!(input, "receive {value} {length}").unwrap();
            input.flush().unwrap();
            line(&mut output, "WAIT");
            outgoing.send(&payload(value, length)).unwrap();
            line(&mut output, "OK");
            writeln!(input, "send {value} {length}").unwrap();
            input.flush().unwrap();
            line(&mut output, "OK");
            assert_eq!(
                incoming.receive(Duration::ZERO).unwrap(),
                Some(payload(value, length))
            );
        }
        if conflate {
            for value in 1..=3 {
                outgoing.send(&payload(value, 17)).unwrap();
            }
            writeln!(input, "receive 3 17").unwrap();
            input.flush().unwrap();
            line(&mut output, "WAIT");
            line(&mut output, "OK");
            for value in 1..=3 {
                writeln!(input, "send {value} 17").unwrap();
                input.flush().unwrap();
                line(&mut output, "OK");
            }
            assert_eq!(
                incoming.receive(Duration::ZERO).unwrap(),
                Some(payload(3, 17))
            );
        }
        for value in 0..1000 {
            outgoing.send(&payload(value, 128)).unwrap();
        }
        writeln!(input, "empty").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        writeln!(input, "burst 1000 128").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        assert!(incoming.receive(Duration::ZERO).unwrap().is_none());
        writeln!(input, "readers").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        assert!(incoming.receive(Duration::ZERO).unwrap().is_none());
        for _ in 0..40 {
            drop(Subscriber::with_capacity("rustToNative", false, 4096).unwrap());
        }
        writeln!(input, "empty").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        outgoing.send(&payload(1001, 19)).unwrap();
        writeln!(input, "receive 1001 19").unwrap();
        input.flush().unwrap();
        line(&mut output, "WAIT");
        line(&mut output, "OK");
        writeln!(input, "restart").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        assert!(incoming.receive(Duration::ZERO).unwrap().is_none());
        writeln!(input, "send 1002 31").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        assert_eq!(
            incoming.receive(Duration::ZERO).unwrap(),
            Some(payload(1002, 31))
        );
        writeln!(input, "stop").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        assert!(peer.wait().unwrap().success());
    }
}

#[test]
fn corrupt_shared_memory_fails_without_accessing_outside_the_mapping() {
    if !child_case("corrupt_shared_memory_fails_without_accessing_outside_the_mapping") {
        return;
    }
    for (index, (offset, value)) in [
        (8, 3_u64),
        (8, 4096),
        (24, 4096),
        (984, 0),
        (984, u64::MAX),
        (984, 4096),
        (984, u64::MAX - 1),
    ]
    .into_iter()
    .enumerate()
    {
        let endpoint = format!("corrupt{index}");
        let mut publisher = Publisher::with_capacity(&endpoint, 4096).unwrap();
        let mut subscriber = Subscriber::with_capacity(&endpoint, false, 4096).unwrap();
        publisher.send(b"one valid message").unwrap();
        let path = format!(
            "/dev/shm/msgq_{}/{}",
            env::var("OPENPILOT_PREFIX").unwrap(),
            endpoint
        );
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.write_all_at(&value.to_ne_bytes(), offset).unwrap();

        let result = subscriber.receive(Duration::ZERO);

        assert!(
            matches!(result, Err(Error::Corrupt(_))),
            "offset={offset} value={value}"
        );
    }
}

#[test]
fn repeated_signal_interruptions_preserve_the_remaining_receive_timeout() {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Instant;
    static SIGNALS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn observed_signal(_: libc::c_int) {
        SIGNALS.fetch_add(1, Ordering::SeqCst);
    }
    if !child_case("repeated_signal_interruptions_preserve_the_remaining_receive_timeout") {
        return;
    }
    let _publisher = Publisher::with_capacity("signals", 4096).unwrap();
    let mut subscriber = Subscriber::with_capacity("signals", false, 4096).unwrap();
    // SAFETY: this isolated test installs a non-unwinding C signal handler whose
    // only operation is a lock-free atomic increment, after msgq initialization.
    assert_ne!(
        unsafe {
            libc::signal(
                libc::SIGUSR2,
                observed_signal as *const () as libc::sighandler_t,
            )
        },
        libc::SIG_ERR
    );
    // SAFETY: gettid has no pointer arguments and identifies this polling thread.
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    println!("IPC194_SIGNAL_TID={tid}");
    let done = Arc::new(AtomicBool::new(false));
    let finished = Arc::clone(&done);
    let sleep_syscalls = env::var("IPC194_HOST_SLEEP_SYSCALLS")
        .map(|value| {
            value
                .split(',')
                .map(|number| number.parse::<libc::c_long>().unwrap())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|_| vec![libc::SYS_clock_nanosleep, libc::SYS_ppoll]);
    let interrupt = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        for expected in 1..=10 {
            loop {
                if finished.load(Ordering::Acquire) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "polling thread never entered its wait syscall"
                );
                let syscall =
                    std::fs::read_to_string(format!("/proc/self/task/{tid}/syscall")).unwrap();
                if syscall
                    .split_whitespace()
                    .next()
                    .and_then(|number| number.parse::<libc::c_long>().ok())
                    .is_some_and(|number| sleep_syscalls.contains(&number))
                {
                    // SAFETY: the target thread remains alive until this worker is
                    // joined; SIGUSR2 uses the atomic-only handler installed above.
                    assert_eq!(
                        unsafe { libc::syscall(libc::SYS_tkill, tid, libc::SIGUSR2) },
                        0
                    );
                    break;
                }
                std::thread::yield_now();
            }
            while SIGNALS.load(Ordering::SeqCst) < expected {
                assert!(
                    Instant::now() < deadline,
                    "signal handler did not acknowledge delivery"
                );
                std::thread::yield_now();
            }
        }
    });
    let started = Instant::now();

    let result = subscriber.receive(Duration::from_millis(120));

    done.store(true, Ordering::Release);
    interrupt.join().unwrap();
    assert!(result.unwrap().is_none());
    assert_eq!(SIGNALS.load(Ordering::SeqCst), 10);
    assert!(started.elapsed() >= Duration::from_millis(100));
    assert!(started.elapsed() < Duration::from_secs(3));
}
