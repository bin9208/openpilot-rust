//! Same-process observations; the fixture is captured from unchanged setproctitle.
#![cfg(all(target_os = "linux", target_env = "gnu", feature = "qa-process-title"))]
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "source-title-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Peer {
    child: Child,
    lines: mpsc::Receiver<String>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Peer {
    fn start(prepared: &std::path::Path) -> Self {
        Self::start_mode(prepared, false)
    }
    fn start_mode(prepared: &std::path::Path, noenv: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_qa-source-title-peer"));
        command
            .arg(prepared)
            .arg("initial-extra-argument")
            .env(
                "QA_SOURCE_TITLE_SENTINEL",
                "sentinel-".to_owned() + &"x".repeat(4096),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if noenv {
            command.env("SPT_NOENV", "1");
        } else {
            command.env_remove("SPT_NOENV");
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let peer = Self { child, lines };
        peer.phase("READY");
        peer
    }
    fn phase(&self, name: &str) {
        assert_eq!(
            self.lines.recv_timeout(Duration::from_secs(5)).unwrap(),
            format!("{name} {}", self.child.id())
        );
    }
    fn send(&mut self, bytes: &[u8]) {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.flush().unwrap();
    }
    fn cmdline(&self) -> Vec<u8> {
        fs::read(format!("/proc/{}/cmdline", self.child.id())).unwrap()
    }
    fn comm(&self) -> Vec<u8> {
        let mut bytes = fs::read(format!("/proc/{}/comm", self.child.id())).unwrap();
        assert_eq!(bytes.pop(), Some(b'\n'));
        bytes
    }
    fn finish(&mut self) {
        self.send(b"finish\n");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "title peer did not exit after acknowledgement"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn normalized(bytes: &[u8]) -> Vec<u8> {
    let bytes = bytes
        .iter()
        .map(|b| if *b == 0 { b' ' } else { *b })
        .collect::<Vec<_>>();
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    bytes[start..end].to_vec()
}
#[test]
fn same_pid_changes_real_cmdline_only_after_prepare_with_original_name_and_nul_contracts() {
    let directory = Directory::new();
    let prepared = directory.0.join("prepared");
    fs::write(&prepared, b"actual prepare input").unwrap();
    for line in include_str!("fixtures/source-title-original.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let columns = line.split('\t').collect::<Vec<_>>();
        assert_eq!(columns.len(), 4);
        let mut peer = Peer::start(&prepared);
        let initial = peer.cmdline();
        let comm = peer.comm();
        assert!(initial
            .windows(b"initial-extra-argument".len())
            .any(|w| w == b"initial-extra-argument"));
        peer.send(b"prepare\n");
        peer.phase("PREPARED");
        assert_eq!(
            peer.cmdline(),
            initial,
            "prepare may not set argv0/title early"
        );
        assert_eq!(peer.comm(), comm);
        let title = unhex(columns[0]);
        peer.send(format!("{}\n", title.len()).as_bytes());
        peer.send(&title);
        if columns[1] == "OK" {
            peer.phase("NAMED");
            assert_eq!(normalized(&peer.cmdline()), unhex(columns[2]));
            assert_eq!(peer.comm(), unhex(columns[3]));
        } else {
            assert_eq!(columns[1], "ValueError");
            peer.phase("NAME_ERROR");
            assert_eq!(peer.cmdline(), initial);
            assert_eq!(peer.comm(), comm);
        }
        peer.finish();
    }
}
#[test]
fn failed_prepare_leaves_real_process_title_unchanged_and_does_not_enter_name() {
    let directory = Directory::new();
    let mut peer = Peer::start(&directory.0.join("missing"));
    let initial = peer.cmdline();
    let comm = peer.comm();
    peer.send(b"prepare\n");
    peer.phase("PREPARE_ERROR");
    assert_eq!(peer.cmdline(), initial);
    assert_eq!(peer.comm(), comm);
    peer.finish();
}

#[test]
fn original_noenv_option_preserves_proc_environment_and_argv_window_truncation() {
    // Unchanged setproctitle1.3.7 capture: before_bytes505/title_bytes503,
    // environ_unchanged=true. The limit depends on this child's actual argv.
    let directory = Directory::new();
    let prepared = directory.0.join("prepared");
    fs::write(&prepared, b"actual prepare input").unwrap();
    let mut peer = Peer::start_mode(&prepared, true);
    let initial = peer.cmdline();
    let environ = fs::read(format!("/proc/{}/environ", peer.child.id())).unwrap();
    peer.send(b"prepare\n");
    peer.phase("PREPARED");
    let title = vec![b'q'; initial.len() + 50];
    peer.send(format!("{}\n", title.len()).as_bytes());
    peer.send(&title);
    peer.phase("NAMED");
    assert_eq!(normalized(&peer.cmdline()), vec![b'q'; initial.len() - 2]);
    assert_eq!(
        fs::read(format!("/proc/{}/environ", peer.child.id())).unwrap(),
        environ
    );
    peer.finish();
}

#[test]
fn non_source_invocation_leaves_title_unavailable_and_real_argv_environment_unchanged() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qa-source-title-peer"))
        .arg("--other-mode")
        .arg("original-candidate-argument")
        .env("QA_SOURCE_TITLE_SENTINEL", "not-a-source-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let mut peer = Peer { child, lines };
    peer.phase("UNSUPPORTED");
    let cmdline = peer.cmdline();
    assert!(cmdline
        .windows(b"original-candidate-argument".len())
        .any(|part| part == b"original-candidate-argument"));
    let environment = fs::read(format!("/proc/{}/environ", peer.child.id())).unwrap();
    assert!(environment
        .split(|byte| *byte == 0)
        .any(|entry| entry == b"QA_SOURCE_TITLE_SENTINEL=not-a-source-worker"));
    assert_eq!(peer.cmdline(), cmdline);
    peer.finish();
}
