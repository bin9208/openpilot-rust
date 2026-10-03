#![cfg(feature = "native-skip-miri")]

mod support;

use openpilot_msgq::{Error, Publisher, Subscriber};
use std::{
    env,
    fs::File,
    io::{BufRead, BufReader, Write},
    os::unix::fs::FileExt,
    process::{Child, ChildStdin, ChildStdout, Stdio},
    time::Duration,
};

fn isolated(name: &str) -> bool {
    if env::var("IPC194_RACE_CASE").as_deref() == Ok(name) {
        return true;
    }
    let directory = tempfile::Builder::new()
        .prefix("msgq_rust-probe-r194-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = directory
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let status = support::command(env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("IPC194_RACE_CASE", name)
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .status()
        .unwrap();
    assert!(status.success());
    false
}

fn request(input: &mut impl BufRead) -> String {
    let mut line = String::new();
    assert_ne!(input.read_line(&mut line).unwrap(), 0);
    line.trim().to_owned()
}

struct Peer {
    process: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Peer {
    fn start(name: &str) -> Self {
        let mut process = support::command(env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--quiet"])
            .env("IPC194_RACE_PEER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = process.stdin.take().unwrap();
        let mut output = BufReader::new(process.stdout.take().unwrap());
        while request(&mut output) != "READY" {}
        Self {
            process,
            input,
            output,
        }
    }
    fn send(&mut self, command: &str) {
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
    }
    fn reply(&mut self, expected: &str) {
        assert_eq!(request(&mut self.output), expected);
    }
    fn stop(mut self) {
        self.send("stop");
        self.reply("DONE");
        assert!(self.process.wait().unwrap().success());
    }
}

#[test]
fn concurrent_subscribers_create_one_shared_queue_and_survive_publisher_start() {
    const NAME: &str = "concurrent_subscribers_create_one_shared_queue_and_survive_publisher_start";
    if env::var_os("IPC194_RACE_PEER").is_some() {
        println!("READY");
        let mut input = std::io::stdin().lock();
        assert_eq!(request(&mut input), "go");
        let mut subscriber = Subscriber::with_capacity("startup", false, 4096).unwrap();
        println!("OPEN");
        assert_eq!(request(&mut input), "reset");
        assert!(subscriber.receive(Duration::ZERO).unwrap().is_none());
        println!("RESET");
        assert_eq!(request(&mut input), "receive");
        assert_eq!(
            subscriber.receive(Duration::from_secs(2)).unwrap(),
            Some(b"after concurrent startup".to_vec())
        );
        println!("FRAME");
        assert_eq!(request(&mut input), "stop");
        drop(subscriber);
        println!("DONE");
        return;
    }
    if !isolated(NAME) {
        return;
    }
    let mut peers: Vec<_> = (0..12).map(|_| Peer::start(NAME)).collect();
    for peer in &mut peers {
        peer.send("go");
    }
    for peer in &mut peers {
        peer.reply("OPEN");
    }
    let path = format!(
        "/dev/shm/msgq_{}/startup",
        env::var("OPENPILOT_PREFIX").unwrap()
    );
    let file = File::open(path).unwrap();
    let mut count = [0; 8];
    file.read_exact_at(&mut count, 0).unwrap();
    assert_eq!(u64::from_ne_bytes(count), 12);
    let mut publisher = Publisher::with_capacity("startup", 4096).unwrap();
    for peer in &mut peers {
        peer.send("reset");
    }
    for peer in &mut peers {
        peer.reply("RESET");
    }
    file.read_exact_at(&mut count, 0).unwrap();
    assert_eq!(u64::from_ne_bytes(count), 12);
    publisher.send(b"after concurrent startup").unwrap();
    for peer in &mut peers {
        peer.send("receive");
    }
    for peer in &mut peers {
        peer.reply("FRAME");
    }
    assert!(publisher.readers_caught_up());
    for peer in peers {
        peer.stop();
    }
}

#[test]
fn concurrent_rust_publishers_hold_exactly_one_process_lock() {
    const NAME: &str = "concurrent_rust_publishers_hold_exactly_one_process_lock";
    if env::var_os("IPC194_RACE_PEER").is_some() {
        println!("READY");
        let mut input = std::io::stdin().lock();
        assert_eq!(request(&mut input), "go");
        let publisher = match Publisher::with_capacity("competition", 4096) {
            Ok(publisher) => {
                println!("OWNER");
                Some(publisher)
            }
            Err(Error::Io(_, error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                println!("REFUSED");
                None
            }
            Err(error) => panic!("unexpected publisher result: {error}"),
        };
        assert_eq!(request(&mut input), "stop");
        drop(publisher);
        println!("DONE");
        return;
    }
    if !isolated(NAME) {
        return;
    }
    let mut peers: Vec<_> = (0..8).map(|_| Peer::start(NAME)).collect();
    for peer in &mut peers {
        peer.send("go");
    }
    let mut owners = 0;
    for peer in &mut peers {
        match request(&mut peer.output).as_str() {
            "OWNER" => owners += 1,
            "REFUSED" => {}
            other => panic!("invalid publisher result: {other}"),
        }
    }
    assert_eq!(owners, 1);
    assert!(Publisher::with_capacity("competition", 4096).is_err());
    for peer in peers {
        peer.stop();
    }
    assert!(Publisher::with_capacity("competition", 4096).is_ok());
}
