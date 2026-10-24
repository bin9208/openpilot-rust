//! 121aa5 실제 원문만 읽는 test helper. 원본 실행/expected 생성 fallback 없음.
#![cfg(feature = "route-vault-bundle-native")]
use openpilot_dev_tools::qa_process::Captured;
use rustix::fs::{mkdirat, openat, Mode, OFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, File, Permissions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{symlink, MetadataExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

const MANIFEST_SHA: &str = "76240cd14a66a55b7ef91459b49919cdbd582d19161437221a6f65f7d22a362b";
const CAPTURE_COMMIT: &str = "121aa5b86ddf427697a8cabb25d2ed337d80f429";
const CAPTURE_HARNESS: &str = "5d888f50c5e57483d55b5ab87fe3f8678406cb8bc875651d765573d225390f4f";
const CAPTURE_ZIP: &str = "b6ecc542f61d83ee1bfa518d3926ccfb45e6d2f52240eb7f85fd42c8cdc3f50a";
const ORIGINAL: &str = "tools/carrot_route_vault/build_bundle.py";
const ORIGINAL_SHA: &str = "05551fc470864a4c70758a66b1a156c2b6d9e95e8deeb1f2a74ac13fb5259aa9";
const DIRECTORY: &str = "rust/crates/dev-tools/tests/fixtures/route-vault-bundle-original";
const READ_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);
const DIR_FLAGS: OFlags = READ_FLAGS.union(OFlags::DIRECTORY);
const SOURCES: &[&str] = &[
    "rust/crates/dev-tools/src/route_vault_bundle.rs",
    "rust/crates/dev-tools/src/argparse_scan.rs",
    "rust/crates/dev-tools/src/qa_process.rs",
    "rust/crates/dev-tools/src/seal.rs",
    "rust/crates/dev-tools/src/lib.rs",
    "rust/crates/dev-tools/src/bin/build_route_vault_bundle.rs",
    "rust/crates/dev-tools/Cargo.toml",
    "rust/Cargo.toml",
    "rust/Cargo.lock",
    "rust/rust-toolchain.toml",
];
const GROUPS: &[(&str, usize)] = &[
    (
        "synthetic_original_native_committed_bytes_types_filter_and_worktree_exclusion",
        1,
    ),
    (
        "synthetic_original_native_invalid_ref_existing_destination_and_partial_failure",
        3,
    ),
    (
        "synthetic_original_native_all_seven_fixed_failures_match_partial_effects",
        7,
    ),
    (
        "actual_original_and_native_argparse_exit_and_creation_effects_match",
        12,
    ),
];
static USED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn sha(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn text(value: &Value) -> &str {
    value.as_str().expect("필수 capture 문자열")
}
fn unhex(value: &Value) -> Vec<u8> {
    let value = text(value);
    assert!(
        value.len().is_multiple_of(2)
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn path_bytes(value: &Value) -> Vec<u8> {
    let bytes = unhex(&value["bytes_hex"]);
    assert!(!bytes.contains(&0));
    assert_eq!(value["utf8"].as_str(), std::str::from_utf8(&bytes).ok());
    bytes
}
fn relative(bytes: Vec<u8>) -> PathBuf {
    assert!(
        !bytes.contains(&0)
            && (bytes.is_empty()
                || bytes
                    .split(|&b| b == b'/')
                    .all(|p| !p.is_empty() && p != b"." && p != b"..")),
        "raw 출력 경로의 slash/dot/NUL 정규화는 허용하지 않는다"
    );
    let path = PathBuf::from(OsString::from_vec(bytes));
    assert!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "출력 상대 경로 탈출"
    );
    path
}
fn open_path(path: &Path, flags: OFlags) -> File {
    assert!(path.is_absolute());
    let mut file = File::from(rustix::fs::open("/", DIR_FLAGS, Mode::empty()).unwrap());
    let mut parts = path.components().peekable();
    while let Some(part) = parts.next() {
        let Component::Normal(name) = part else {
            assert_eq!(part, Component::RootDir);
            continue;
        };
        file = File::from(
            openat(
                &file,
                name,
                if parts.peek().is_some() {
                    DIR_FLAGS
                } else {
                    flags
                },
                Mode::empty(),
            )
            .unwrap(),
        );
    }
    file
}
fn absent(root: &Path) -> bool {
    matches!(fs::symlink_metadata(root.join(ORIGINAL)),Err(error) if error.kind()==std::io::ErrorKind::NotFound)
}
fn identity(m: &fs::Metadata) -> Vec<i128> {
    [
        i128::from(m.dev()),
        i128::from(m.ino()),
        i128::from(m.mode()),
        i128::from(m.uid()),
        i128::from(m.gid()),
        i128::from(m.len()),
        i128::from(m.mtime()),
        i128::from(m.mtime_nsec()),
        i128::from(m.ctime()),
        i128::from(m.ctime_nsec()),
    ]
    .to_vec()
}
struct Held {
    path: PathBuf,
    file: File,
    seal: (Vec<i128>, String),
    limit: u64,
}
impl Held {
    fn open(path: PathBuf, limit: u64, pin: Option<&str>) -> (Self, Vec<u8>) {
        let file = open_path(&path, READ_FLAGS);
        let mut held = Self {
            path,
            file,
            seal: (vec![], String::new()),
            limit,
        };
        let (bytes, seal) = held.read();
        if let Some(pin) = pin {
            assert!(sha(pin));
            assert_eq!(seal.1, pin, "raw file SHA 불일치");
        }
        held.seal = seal;
        (held, bytes)
    }
    fn read(&mut self) -> (Vec<u8>, (Vec<i128>, String)) {
        let before = self.file.metadata().unwrap();
        assert!(before.is_file() && before.len() <= self.limit);
        assert_eq!(
            identity(&before),
            identity(&open_path(&self.path, READ_FLAGS).metadata().unwrap())
        );
        self.file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        Read::by_ref(&mut self.file)
            .take(self.limit + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes.len() as u64, before.len());
        assert_eq!(identity(&before), identity(&self.file.metadata().unwrap()));
        assert_eq!(
            identity(&before),
            identity(&open_path(&self.path, READ_FLAGS).metadata().unwrap())
        );
        let hash = digest(&bytes);
        (bytes, (identity(&before), hash))
    }
    fn verify(&mut self) {
        let (_, after) = self.read();
        assert_eq!(self.seal, after, "held source/fixture/ELF 변경");
    }
}

pub(crate) struct Frozen {
    root: PathBuf,
    rows: Vec<(Value, Value)>,
    held: RefCell<Vec<Held>>,
    snapshot: fn(&Path) -> Value,
    calls: Cell<usize>,
    expected_calls: usize,
}
pub(crate) struct Input<'a> {
    pub(crate) repo: &'a Path,
    pub(crate) initial: &'a str,
    pub(crate) head: &'a str,
    pub(crate) temp: &'a Path,
    pub(crate) cwd: &'a Path,
    pub(crate) args: &'a [OsString],
    pub(crate) dest: &'a Path,
    pub(crate) reference: Option<&'a str>,
}
impl Frozen {
    pub(crate) fn open(root: &Path, snapshot: fn(&Path) -> Value) -> Self {
        assert!(
            absent(root),
            "원본 부재 필수: 파일/dir/broken symlink 모두 거부"
        );
        assert!(sha(MANIFEST_SHA), "실제 capture manifest pin 필수");
        let (manifest_file, bytes) = Held::open(
            root.join(DIRECTORY).join("manifest.json"),
            1024 * 1024,
            Some(MANIFEST_SHA),
        );
        let manifest: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(manifest["schema"], "openpilot.bundle-frozen-input.v1");
        assert_eq!(manifest["capture_source_commit"], CAPTURE_COMMIT);
        assert_eq!(manifest["retired_source_sha256"], ORIGINAL_SHA);
        assert_eq!(manifest["capture_run"], 40093557875_u64);
        assert_eq!(manifest["capture_job"], 120871017872_u64);
        assert_eq!(manifest["capture_harness_sha256"], CAPTURE_HARNESS);
        assert_eq!(manifest["artifact_zip_sha256"], CAPTURE_ZIP);
        assert_eq!(manifest["capture_artifact"], 12560845618_u64);
        assert_eq!(manifest["raw_total_bytes"], 184435_u64);
        assert_eq!(
            manifest["current_native_sha256"].as_object().unwrap().len(),
            SOURCES.len()
        );
        let mut held = vec![manifest_file];
        for path in SOURCES {
            let pin = text(&manifest["current_native_sha256"][*path]);
            held.push(Held::open(root.join(path), 8 * 1024 * 1024, Some(pin)).0);
        }
        // manifest에 자기 source SHA를 넣어 hash 순환을 만들지 않고 실제 컴파일 bytes와 대조한다.
        for (path, compiled) in [
            (
                "rust/crates/dev-tools/tests/route_vault_bundle.rs",
                include_bytes!("../route_vault_bundle.rs").as_slice(),
            ),
            (
                "rust/crates/dev-tools/tests/route_vault_bundle/frozen.rs",
                include_bytes!("frozen.rs").as_slice(),
            ),
        ] {
            let (file, bytes) = Held::open(root.join(path), 2 * 1024 * 1024, None);
            assert_eq!(bytes, compiled, "컴파일된 test와 현재 source 불일치");
            held.push(file);
        }
        // 별도 host target의 public helper ELF는 이 integration CLI를 대신하지 않는다.
        for path in [
            std::env::current_exe().unwrap(),
            PathBuf::from(env!("CARGO_BIN_EXE_build-route-vault-bundle")),
        ] {
            let (file, bytes) = Held::open(path, 128 * 1024 * 1024, None);
            assert!(bytes.starts_with(b"\x7fELF"), "test/CLI ELF magic 필수");
            held.push(file);
        }
        let pins = manifest["rows"].as_array().unwrap();
        assert_eq!(pins.len(), 23);
        let mut rows = Vec::new();
        let mut names = BTreeSet::new();
        let mut ordinals = BTreeSet::new();
        let mut total = 0;
        for pin in pins {
            let name = relative(text(&pin["file"]).as_bytes().to_vec());
            assert_eq!(name.components().count(), 1, "raw JSON 파일명만 허용");
            assert!(names.insert(name.clone()));
            let (file, bytes) = Held::open(
                root.join(DIRECTORY).join(name),
                8 * 1024 * 1024,
                Some(text(&pin["sha256"])),
            );
            total += bytes.len();
            assert!(total <= 64 * 1024 * 1024);
            assert_eq!(bytes.len() as u64, pin["bytes"].as_u64().unwrap());
            let row: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                row["schema"],
                "openpilot.route-vault-bundle.original-capture.v1"
            );
            assert_eq!(row["source_commit"], CAPTURE_COMMIT);
            assert_eq!(row["row"], pin["row"]);
            assert!(ordinals.insert(row["row"].as_u64().unwrap()));
            assert_eq!(row["source_script_path"], ORIGINAL);
            for key in [
                "source_script_sha256",
                "script_sha256_before",
                "script_sha256_after",
            ] {
                assert_eq!(row[key], ORIGINAL_SHA);
            }
            assert_eq!(row["outcome"]["kind"], "captured");
            assert_eq!(row["timeout_seconds"], 20);
            held.push(file);
            rows.push((pin.clone(), row));
        }
        assert_eq!(total, 184435);
        assert_eq!(ordinals, (0..23_u64).collect::<BTreeSet<_>>());
        for (group, count) in GROUPS {
            assert_eq!(
                rows.iter()
                    .filter(|(_, r)| r["test_name"] == *group)
                    .count(),
                *count
            );
        }
        assert_eq!(GROUPS.iter().map(|(_, n)| n).sum::<usize>(), 23);
        let thread = std::thread::current();
        let name = thread.name().unwrap();
        let expected_calls = match name {
            "synthetic_original_native_committed_bytes_types_filter_and_worktree_exclusion"
            | "synthetic_original_native_invalid_ref_existing_destination_and_partial_failure"
            | "synthetic_original_native_all_seven_fixed_failures_match_partial_effects" => 1,
            "actual_original_and_native_argparse_exit_and_creation_effects_match" => 12,
            "cli_help_unknowns_and_compiled_source_root_do_not_follow_invocation_cwd" => 0,
            name if name.ends_with(
                "frozen_controls_reject_wrong_bindings_tamper_escape_and_source_reappearance",
            ) =>
            {
                0
            }
            _ => panic!("검토하지 않은 frozen caller: {name}"),
        };
        Self {
            root: root.to_owned(),
            rows,
            held: RefCell::new(held),
            snapshot,
            calls: Cell::new(0),
            expected_calls,
        }
    }
    fn verify(&self) {
        assert!(absent(&self.root));
        for file in self.held.borrow_mut().iter_mut() {
            file.verify();
        }
        assert!(absent(&self.root));
    }
    pub(crate) fn replay(&self, input: Input<'_>) -> Captured {
        let Input {
            repo,
            initial,
            head,
            temp,
            cwd,
            args,
            dest,
            reference,
        } = input;
        self.verify();
        assert!(cwd.starts_with(temp) && dest.starts_with(temp));
        assert_eq!(dest.parent(), Some(temp));
        assert_eq!(cwd, if reference.is_some() { repo } else { dest });
        assert!(!repo.starts_with(dest) && !dest.starts_with(repo));
        let thread = std::thread::current();
        let name = thread.name().unwrap();
        let before = (self.snapshot)(dest);
        let selected = self
            .rows
            .iter()
            .filter(|(pin, row)| {
                if row["test_name"] != name
                    || row["fixture_initial_head"] != initial
                    || row["fixture_head"] != head
                    || row["input_reference"].as_str() != reference
                    || row["destination_before"] != before
                    || row["destination_scope_kind"]
                        != if reference.is_some() {
                            "destination"
                        } else {
                            "parser-empty-cwd"
                        }
                {
                    return false;
                }
                let old = [
                    path_bytes(&row["fixture_root"]),
                    path_bytes(&row["destination_scope"]),
                    path_bytes(&row["provider_cwd"]),
                ];
                let new = [
                    repo.as_os_str().as_bytes(),
                    dest.as_os_str().as_bytes(),
                    cwd.as_os_str().as_bytes(),
                ];
                let argv = row["input_argv_hex"].as_array().unwrap();
                argv.len() == args.len()
                    && pin["argv_spans"].as_array().unwrap().len() == args.len()
                    && argv.iter().zip(args).enumerate().all(|(i, (raw, arg))| {
                        bind(&unhex(raw), &pin["argv_spans"][i], &old, &new) == arg.as_bytes()
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            selected.len(),
            1,
            "test/argv/ref/HEAD selector는 원문 한 행만 선택해야 한다"
        );
        let (pin, row) = selected[0];
        assert!(
            USED.lock().unwrap().insert(text(&pin["sha256"]).to_owned()),
            "원문 행 중복 소비"
        );
        let old = [
            path_bytes(&row["fixture_root"]),
            path_bytes(&row["destination_scope"]),
            path_bytes(&row["provider_cwd"]),
        ];
        let new = [
            repo.as_os_str().as_bytes(),
            dest.as_os_str().as_bytes(),
            cwd.as_os_str().as_bytes(),
        ];
        assert_eq!(before, row["destination_before"], "재현 전 상태 불일치");
        materialize(dest, &row["destination_before"], &row["destination_after"]);
        assert_eq!((self.snapshot)(dest), row["destination_after"]);
        let output = Captured {
            code: i32::try_from(row["outcome"]["code"].as_i64().unwrap()).unwrap(),
            stdout: bind(
                &unhex(&row["outcome"]["stdout_hex"]),
                &pin["stdout_spans"],
                &old,
                &new,
            ),
            stderr: bind(
                &unhex(&row["outcome"]["stderr_hex"]),
                &pin["stderr_spans"],
                &old,
                &new,
            ),
        };
        self.verify();
        self.calls.set(self.calls.get() + 1);
        println!(
            "BUNDLE_FROZEN_ROW {}",
            json!({"test_name":name,"raw_sha256":pin["sha256"],"historical_source":CAPTURE_COMMIT,"fresh_original_executions":0})
        );
        output
    }
}
impl Drop for Frozen {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        self.verify(); // Repo 수명이 끝날 때까지 기존 native assertion 뒤도 검사한다.
        assert_eq!(
            self.calls.get(),
            self.expected_calls,
            "기존 caller의 정확한 row 소비 횟수"
        );
    }
}
// raw JSON은 그대로 유지한다. 부모 승인 manifest의 명시 offset에만 ROOT/DEST/CWD bytes를 치환한다.
fn bind(raw: &[u8], spans: &Value, old: &[Vec<u8>; 3], new: &[&[u8]; 3]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut end = 0;
    let mut covered = Vec::new();
    for i in 0..3 {
        assert!(!old[i].is_empty());
        for j in 0..i {
            if old[i] == old[j] {
                assert_eq!(new[i], new[j], "원문의 alias 관계도 보존해야 한다");
            }
        }
    }
    for span in spans
        .as_array()
        .expect("명시 span 배열 필수; 변경 없음도 []")
    {
        let token = match text(&span["token"]) {
            "ROOT" => 0,
            "DEST" => 1,
            "CWD" => 2,
            _ => panic!("허용하지 않은 placeholder"),
        };
        let at = usize::try_from(span["offset"].as_u64().unwrap()).unwrap();
        assert!(!old[token].is_empty() && at >= end);
        let next = at.checked_add(old[token].len()).unwrap();
        assert_eq!(
            raw.get(at..next),
            Some(old[token].as_slice()),
            "명시 raw span 불일치"
        );
        result.extend_from_slice(&raw[end..at]);
        result.extend_from_slice(new[token]);
        covered.push(at..next);
        end = next;
    }
    // 치환하지 않은 동적 경로를 raw stdout/stderr에 남기는 누락도 거부한다.
    for anchor in old {
        for (at, bytes) in raw.windows(anchor.len()).enumerate() {
            if bytes == anchor {
                assert!(
                    covered
                        .iter()
                        .any(|range| range.start <= at && at + anchor.len() <= range.end),
                    "동적 경로의 명시 span 누락"
                );
            }
        }
    }
    result.extend_from_slice(&raw[end..]);
    result
}
fn materialize(root: &Path, before: &Value, after: &Value) {
    let rows = after["entries"].as_array().unwrap();
    assert!(!rows.is_empty() && rows.len() <= 128);
    let prior = before["entries"].as_array().unwrap();
    assert_eq!(
        prior.len(),
        1,
        "실제23행의 before는 missing 또는 빈 destination뿐이다"
    );
    assert!(path_bytes(&prior[0]["relative_path"]).is_empty());
    assert_eq!(before["file_bytes"], 0);
    let parent = open_path(root.parent().unwrap(), DIR_FLAGS);
    let metadata = parent.metadata().unwrap();
    assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
    assert_eq!(
        metadata.mode() & 0o777,
        0o700,
        "authored TempDir만 재현한다"
    );
    let name = root.file_name().unwrap();
    assert!(path_bytes(&rows[0]["relative_path"]).is_empty());
    if rows[0]["kind"] == "missing" {
        assert_eq!(before, after);
        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rustix::fs::statat(&parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW),
            Err(rustix::io::Errno::NOENT)
        ));
        return;
    }
    assert_eq!(rows[0]["kind"], "directory");
    let missing = prior[0]["kind"] == "missing";
    if missing {
        mkdirat(&parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR).unwrap();
    } else {
        assert_eq!(prior[0], rows[0], "기존 빈 destination은 변경하지 않는다");
    }
    let destination = File::from(openat(&parent, name, DIR_FLAGS, Mode::empty()).unwrap());
    let mut paths = BTreeSet::new();
    let mut total = 0;
    let mut modes = Vec::new();
    for row in rows {
        let relative = relative(path_bytes(&row["relative_path"]));
        assert!(paths.insert(relative.clone()), "중복 출력 경로");
        let mode = u32::try_from(row["mode"].as_u64().unwrap()).unwrap();
        let kind = text(&row["kind"]);
        let expected_type = match kind {
            "directory" => 0o040000,
            "file" => 0o100000,
            _ => panic!("실제23행에 없는 출력 종류; symlink/special 재현 금지"),
        };
        assert_eq!(
            mode & !0o777,
            expected_type,
            "특수 permission/type은 미지원"
        );
        if relative.as_os_str().is_empty() {
            assert_eq!(kind, "directory");
            if missing {
                modes.push((destination.try_clone().unwrap(), mode));
            }
            continue;
        }
        let mut directory = destination.try_clone().unwrap();
        for part in relative.parent().unwrap().components() {
            let Component::Normal(name) = part else {
                panic!("비정상 parent");
            };
            directory = File::from(openat(&directory, name, DIR_FLAGS, Mode::empty()).unwrap());
        }
        let name = relative.file_name().unwrap();
        let file = if kind == "directory" {
            mkdirat(&directory, name, Mode::RUSR | Mode::WUSR | Mode::XUSR).unwrap();
            File::from(openat(&directory, name, DIR_FLAGS, Mode::empty()).unwrap())
        } else {
            let bytes = unhex(&row["bytes_hex"]);
            assert!(bytes.len() <= 1024 * 1024);
            total += bytes.len();
            assert!(total <= 2 * 1024 * 1024);
            assert_eq!(digest(&bytes), text(&row["sha256"]));
            let mut file = File::from(
                openat(
                    &directory,
                    name,
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::NONBLOCK
                        | OFlags::CLOEXEC,
                    Mode::RUSR | Mode::WUSR,
                )
                .unwrap(),
            );
            file.write_all(&bytes).unwrap();
            file
        };
        modes.push((file, mode));
    }
    assert_eq!(after["file_bytes"].as_u64().unwrap(), total as u64);
    for (file, mode) in modes.into_iter().rev() {
        file.set_permissions(Permissions::from_mode(mode & 0o777))
            .unwrap();
    }
}

#[test]
fn frozen_controls_reject_wrong_bindings_tamper_escape_and_source_reappearance() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let _actual = Frozen::open(source, |_| {
        panic!("거부 통제는 원문 출력을 재현하지 않는다")
    });
    // 독립적인 거부 통제용 bytes다. 실제 original outcome/fixture를 합성하지 않는다.
    let old = [
        b"/old/root".to_vec(),
        b"/old/dest".to_vec(),
        b"/old/cwd".to_vec(),
    ];
    let new = [
        b"/new/root".as_slice(),
        b"/new/dest".as_slice(),
        b"/new/cwd".as_slice(),
    ];
    for span in [
        json!([{"token":"SOURCE","offset":0}]),
        json!([{"token":"ROOT","offset":1}]),
        json!([{"token":"ROOT","offset":0},{"token":"ROOT","offset":0}]),
        json!([]),
    ] {
        assert!(std::panic::catch_unwind(|| bind(b"/old/root", &span, &old, &new)).is_err());
    }
    assert!(std::panic::catch_unwind(|| relative(b"../escape".to_vec())).is_err());
    for path in [b"/escape".as_slice(), b"a//b", b"a/./b", b"a\0b"] {
        assert!(std::panic::catch_unwind(|| relative(path.to_vec())).is_err());
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let raw = root.join("raw.json");
    fs::write(&raw, b"not an oracle fixture").unwrap();
    assert!(
        std::panic::catch_unwind(|| Held::open(raw.clone(), 1024, Some(&"0".repeat(64)))).is_err()
    );
    for kind in ["same-size", "truncate", "replace", "symlink"] {
        let raw = root.join(format!("held-{kind}"));
        let original = b"not an oracle fixture";
        fs::write(&raw, original).unwrap();
        let (mut held, _) = Held::open(raw.clone(), 1024, None);
        match kind {
            "same-size" => {
                let mut changed = original.to_vec();
                changed[3] ^= 1;
                fs::write(&raw, changed).unwrap();
            }
            "truncate" => fs::write(&raw, b"changed").unwrap(),
            "replace" | "symlink" => {
                let moved = root.join(format!("old-{kind}"));
                fs::rename(&raw, &moved).unwrap();
                if kind == "replace" {
                    fs::write(&raw, original).unwrap();
                } else {
                    symlink(&moved, &raw).unwrap();
                }
            }
            _ => unreachable!(),
        }
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| held.verify())).is_err());
        if kind == "same-size" {
            // 테스트 안에서 metadata만 재바인딩해도 이전 전체 SHA가 별도로 거부해야 한다.
            held.seal.0 = identity(&fs::metadata(&raw).unwrap());
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| held.verify())).is_err()
            );
        }
    }
    let retired = root.join(ORIGINAL);
    fs::create_dir_all(retired.parent().unwrap()).unwrap();
    assert!(absent(root));
    fs::write(&retired, b"reappearance control").unwrap();
    assert!(!absent(root));
    fs::remove_file(&retired).unwrap();
    symlink("absent-target", &retired).unwrap();
    assert!(!absent(root));
    fs::remove_file(&retired).unwrap();
    fs::create_dir(&retired).unwrap();
    assert!(!absent(root));
}
