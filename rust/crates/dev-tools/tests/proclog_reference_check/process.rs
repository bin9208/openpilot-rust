//! Real CLI/owned-process controls. Replayed responses come only from sealed
//! original Python packets, never from checker::expected_event or a candidate.
use super::*;
use openpilot_dev_tools::qa_process::{Captured, CommandSpec, NativeRunner, Runner};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
    time::Duration,
};
const SCRIPT: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$BT_LAUNCHES"
if [ "$1" = '--queue-size' ]; then
 case "$BT_MODE" in
  wrong-queue) printf '0\n';;
  invalid-queue) printf 'invalid\n';;
  queue-child-failure) exit 7;;
  queue-underscore) printf '10_485_760\n';;
  *) printf '10485760\n';;
 esac
 exit 0
fi
if [ "$BT_MODE" = 'hang' ]; then
 /bin/sleep 60 &
 printf '%s %s %s\n' "$BT_TOKEN" "$$" "$!" > "$BT_MARKER"
 wait
 exit 0
fi
i=0
while IFS= read -r tick; do
 test "$tick" = "$((1000+i))" || exit 8
 if [ "$i" -eq 1 ]; then
  if [ -f "$1/123/smaps_rollup" ]; then file="$1/123/smaps_rollup"; else file="$1/123/smaps"; fi
  IFS= read -r pss < "$file"
  test "$pss" = 'Pss: 99 kB' || exit 10
 fi
 if [ "$BT_MODE" = 'missing-frame' ] && [ "$i" -eq 21 ]; then exit 0; fi
 if [ "$BT_MODE" = 'early-cache' ] && [ "$i" -eq 1 ]; then
  /bin/cat "$BT_DATA/early"
 elif [ "$i" -eq 0 ]; then
  case "$BT_MODE" in
   size-zero|size-limit|partial-prefix|truncated-payload|wrong-valid)
    /bin/cat "$BT_DATA/$BT_MODE"
    case "$BT_MODE" in partial-prefix|truncated-payload) exit 0;; esac;;
   *) /bin/cat "$BT_DATA/$i";;
  esac
 else /bin/cat "$BT_DATA/$i"
 fi
 i=$((i+1))
done
case "$BT_MODE" in
 extra-frame) /bin/cat "$BT_DATA/21";;
 exit9) exit 9;;
 binary-content) printf '\n# content changed\n' >> "$BT_BINARY";;
 binary-replace) /bin/cp "$BT_BINARY" "$BT_SWAP"; /bin/mv "$BT_SWAP" "$BT_BINARY";;
 binary-retarget) /bin/rm "$BT_ARGUMENT"; /bin/ln -s "$BT_OTHER" "$BT_ARGUMENT";;
 source-content) printf '\n# content changed\n' >> "$BT_SOURCE";;
 source-replace) /bin/cp "$BT_SOURCE" "$BT_SWAP"; /bin/mv "$BT_SWAP" "$BT_SOURCE";;
 source-retarget) /bin/rm "$BT_SOURCE"; /bin/ln -s "$BT_OTHER" "$BT_SOURCE";;
esac
"#;
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn original_packets() -> Vec<Vec<u8>> {
    let compressed = include_bytes!("../fixtures/proclog-reference/process-wire.json.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(compressed)),
        "d7fe66e1945d713a81ee3549bf7aa0768b26393d023cb0c4cf3ffff00b109045"
    );
    let mut raw = vec![];
    flate2::read::GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut raw)
        .unwrap();
    assert_eq!(raw.len(), 116781);
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "45dd4023c4116428509ac71276bff13e349ccbefb00932180d2f76bce65d0508"
    );
    let rows: Vec<Value> = serde_json::from_slice(&raw).unwrap();
    let ticks = rustix::param::clock_ticks_per_second();
    let page = rustix::param::page_size() as u64;
    let packets = rows
        .into_iter()
        .filter(|r| r["ticks"] == ticks && r["page"] == page)
        .map(|r| unhex(r["wire"].as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        packets.len(),
        22,
        "independent process-wire fixture missing host ticks={ticks}/page={page}"
    );
    packets
}
fn framed(bytes: &[u8]) -> Vec<u8> {
    [(bytes.len() as u64).to_le_bytes().as_slice(), bytes].concat()
}
struct Fixture {
    temp: tempfile::TempDir,
    source: PathBuf,
    binary: PathBuf,
    argument: PathBuf,
    mode: String,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let manifest: Value =
            serde_json::from_str(include_str!("../fixtures/proclog-reference/manifest.json"))
                .unwrap();
        for (name, _) in manifest["source_pins"].as_object().unwrap() {
            let target = source.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            match fs::copy(repository().join(name), &target) {
                Ok(_) => {}
                Err(e)
                    if name == "rust/tools/check_proclog_reference.py"
                        && e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => panic!("{name}: {e}"),
            }
        }
        let data = temp.path().join("data");
        fs::create_dir(&data).unwrap();
        let packets = original_packets();
        for (cycle, packet) in packets.iter().enumerate() {
            fs::write(data.join(cycle.to_string()), framed(packet)).unwrap();
        }
        fs::write(data.join("size-zero"), [0; 8]).unwrap();
        fs::write(data.join("size-limit"), 1000000_u64.to_le_bytes()).unwrap();
        fs::write(data.join("partial-prefix"), [1]).unwrap();
        fs::write(
            data.join("truncated-payload"),
            [
                (packets[0].len() as u64).to_le_bytes().as_slice(),
                &packets[0][..packets[0].len() - 1],
            ]
            .concat(),
        )
        .unwrap();
        for (name, index) in [("wrong-valid", 0), ("early", 20)] {
            let original =
                capnp::serialize::read_message(packets[index].as_slice(), Default::default())
                    .unwrap();
            let mut message = capnp::message::Builder::new_default();
            message
                .set_root::<event::Owned>(original.get_root::<event::Reader<'_>>().unwrap())
                .unwrap();
            let mut event = message.get_root::<event::Builder<'_>>().unwrap();
            if name == "wrong-valid" {
                event.set_valid(false)
            } else {
                event.set_log_mono_time(1001)
            };
            fs::write(
                data.join(name),
                framed(&capnp::serialize::write_message_to_words(&message)),
            )
            .unwrap();
        }
        let binary = temp.path().join("peer");
        fs::write(&binary, SCRIPT).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let argument = temp.path().join("argument");
        symlink(&binary, &argument).unwrap();
        let fixture = Self {
            temp,
            source,
            binary,
            argument,
            mode: mode.into(),
        };
        if mode == "binary-retarget" {
            fs::copy(&fixture.binary, fixture.temp.path().join("other")).unwrap();
        }
        if mode == "source-retarget" {
            let original = fixture.source.join("openpilot/system/proclogd.py");
            let first = fixture.temp.path().join("source-first");
            fs::copy(&original, &first).unwrap();
            fs::copy(&original, fixture.temp.path().join("other")).unwrap();
            fs::remove_file(&original).unwrap();
            symlink(first, original).unwrap();
        }
        fixture
    }
    fn spec(&self) -> CommandSpec {
        let mut spec = CommandSpec::new_os(
            vec![
                env!("CARGO_BIN_EXE_check-proclog-reference").into(),
                "--binary".into(),
                self.argument.as_os_str().to_owned(),
            ],
            Duration::from_secs(15),
        );
        // This is an outer regression-test resource guard, not a new checker read deadline.
        for (name, path) in [
            ("OPENPILOT_RUST_ROOT", self.source.clone()),
            ("BT_DATA", self.temp.path().join("data")),
            ("BT_LAUNCHES", self.temp.path().join("launches")),
            ("BT_BINARY", self.binary.clone()),
            ("BT_ARGUMENT", self.argument.clone()),
            ("BT_OTHER", self.temp.path().join("other")),
            ("BT_SWAP", self.temp.path().join("swap")),
            (
                "BT_SOURCE",
                self.source.join("openpilot/system/proclogd.py"),
            ),
            ("BT_MARKER", self.temp.path().join("marker")),
        ] {
            spec.env.insert(name.into(), path.to_str().unwrap().into());
        }
        spec.env.insert("PATH".into(), String::new());
        spec.env.insert("BT_MODE".into(), self.mode.clone());
        spec.env.insert(
            "BT_TOKEN".into(),
            self.temp
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .into(),
        );
        spec
    }
    fn run(&self) -> Captured {
        NativeRunner.run(&self.spec()).unwrap()
    }
}
#[test]
fn actual_protocol_replays_preserve_original_acceptance_and_failure_boundaries() {
    let raw = include_bytes!("../fixtures/proclog-reference/protocol.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "53cfa599f8a196f0bb6e20e349ead7efee78cafedf190f9ffde3876b8c99ed83"
    );
    let controls: Vec<Value> = serde_json::from_slice(raw).unwrap();
    assert_eq!(controls.len(), 28);
    for row in controls.iter().filter(|r| r["fallback"] == false) {
        let mode = row["mode"].as_str().unwrap();
        let fixture = Fixture::new(mode);
        let result = fixture.run();
        assert_eq!(
            result.code == 0,
            row["error"].is_null(),
            "{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if result.code == 0 {
            let fallback = controls
                .iter()
                .find(|r| r["mode"] == mode && r["fallback"] == true)
                .unwrap();
            assert_eq!(
                String::from_utf8(result.stdout).unwrap(),
                format!(
                    "{}{}",
                    row["stdout"].as_str().unwrap(),
                    fallback["stdout"].as_str().unwrap()
                )
            );
        } else {
            assert!(result.stdout.is_empty(), "false mode PASS for {mode}");
        }
    }
}
#[test]
fn actual_source_and_executable_identity_changes_cannot_print_pass() {
    for mode in [
        "source-content",
        "source-replace",
        "source-retarget",
        "binary-content",
        "binary-replace",
        "binary-retarget",
    ] {
        let fixture = Fixture::new(mode);
        let result = fixture.run();
        assert_eq!(result.code, 1, "{mode}");
        assert!(result.stdout.is_empty(), "{mode}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("checker input changed"),
            "{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::read_to_string(fixture.temp.path().join("launches"))
                .unwrap()
                .lines()
                .count(),
            2,
            "must reach actual queue+interactive child before mutation rejection"
        );
    }
}
#[test]
fn fifo_source_and_executable_fail_without_launch_or_block() {
    use rustix::fs::{mkfifoat, Mode, CWD};
    for mode in ["source", "binary"] {
        let fixture = Fixture::new("exact");
        let path = if mode == "source" {
            fixture.source.join("openpilot/system/proclogd.py")
        } else {
            fixture.binary.clone()
        };
        fs::remove_file(&path).unwrap();
        mkfifoat(CWD, &path, Mode::RUSR | Mode::WUSR | Mode::XUSR).unwrap();
        let result = fixture.run();
        assert_eq!(result.code, 1);
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("not a regular file"));
        assert!(!fixture.temp.path().join("launches").exists());
    }
}
#[test]
fn fresh_actual_trace_accepts_raw_binary_path_and_all44_cycles() {
    let trace = std::env::var_os("PROCLOG_REFERENCE_TRACE")
        .expect("mandatory fresh actual reference_trace");
    let temp = tempfile::tempdir().unwrap();
    let argument = temp
        .path()
        .join(OsString::from_vec(b"actual-\xff".to_vec()));
    symlink(fs::canonicalize(trace).unwrap(), &argument).unwrap();
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_check-proclog-reference").into(),
            "--binary".into(),
            argument.into_os_string(),
        ],
        Duration::from_secs(15),
    );
    spec.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        repository().display().to_string(),
    );
    spec.env.insert("PATH".into(), String::new());
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap().lines().count(), 2);
    assert!(result.stderr.is_empty());
}
#[test]
fn leader_only_sigint_cleans_acknowledged_candidate_and_descendant() {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        os::fd::OwnedFd,
        process::{Child, Command, Stdio},
        thread,
        time::Instant,
    };
    struct Checker {
        child: Child,
        identity: OwnedFd,
    }
    impl Drop for Checker {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.identity, Signal::INT);
            let end = Instant::now() + Duration::from_secs(2);
            while Instant::now() < end {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = pidfd_send_signal(&self.identity, Signal::KILL);
            let _ = self.child.wait();
        }
    }
    struct Descendants(Vec<OwnedFd>);
    impl Drop for Descendants {
        fn drop(&mut self) {
            for id in &self.0 {
                let _ = pidfd_send_signal(id, Signal::KILL);
            }
        }
    }
    let fixture = Fixture::new("hang");
    let spec = fixture.spec();
    let argv = spec.effective_argv();
    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .envs(spec.effective_env())
        .stdin(Stdio::null())
        .stdout(fs::File::create(fixture.temp.path().join("stdout")).unwrap())
        .stderr(fs::File::create(fixture.temp.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, identity };
    let mut descendants = Descendants(vec![]);
    let marker = fixture.temp.path().join("marker");
    let token = fixture.temp.path().file_name().unwrap().to_str().unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = fs::read_to_string(&marker) {
            let fields = text.split_whitespace().collect::<Vec<_>>();
            if fields.len() == 3 && fields[0] == token {
                for value in &fields[1..] {
                    let pid = Pid::from_raw(value.parse().unwrap()).unwrap();
                    let identity = pidfd_open(pid, PidfdFlags::empty()).unwrap();
                    let environment =
                        fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap();
                    assert!(environment
                        .split(|b| *b == 0)
                        .any(|v| v == format!("BT_TOKEN={token}").as_bytes()));
                    descendants.0.push(identity);
                }
                break;
            }
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before owned acknowledgement"
        );
        assert!(
            Instant::now() < end,
            "candidate did not acknowledge startup"
        );
        thread::sleep(Duration::from_millis(5));
    }
    // Only the checker leader is signalled. The cleanup assertions run before
    // fallback owners can kill either acknowledged nested process.
    pidfd_send_signal(&checker.identity, Signal::INT).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        let status = checker.child.try_wait().unwrap();
        let gone = descendants.0.iter().all(|id| {
            poll(
                &mut [PollFd::new(id, PollFlags::IN)],
                Some(&Timespec::default()),
            )
            .unwrap()
                == 1
        });
        if let Some(status) = status {
            assert!(!status.success());
            if gone {
                break;
            }
        }
        assert!(
            Instant::now() < end,
            "leader-only SIGINT left acknowledged child/descendant alive"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(fs::read(fixture.temp.path().join("stdout"))
        .unwrap()
        .is_empty());
    assert_eq!(
        fs::read_to_string(fixture.temp.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}
