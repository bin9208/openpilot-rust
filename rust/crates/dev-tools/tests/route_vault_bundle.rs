//! 121aa5 원본23행을 재현하는 retained17검사. frozen18 기능/빌드는 Actions에서 검증한다.
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    route_vault_bundle::{build, parse, GitFailure, SOURCES},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsString,
    fs,
    io::{self, Read},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const ORIGINAL: &str = "tools/carrot_route_vault/build_bundle.py";
const FIXTURE_GIT_ENV: [(&str, &str); 12] = [
    ("GIT_AUTHOR_NAME", "Fixture"),
    ("GIT_AUTHOR_EMAIL", "fixture@example.invalid"),
    ("GIT_COMMITTER_NAME", "Fixture"),
    ("GIT_COMMITTER_EMAIL", "fixture@example.invalid"),
    ("GIT_AUTHOR_DATE", "2000-01-01T00:00:00+0000"),
    ("GIT_COMMITTER_DATE", "2000-01-01T00:00:00+0000"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_SYSTEM", "/dev/null"),
    ("GIT_CONFIG_COUNT", "0"),
    ("LC_ALL", "C.UTF-8"),
    ("TZ", "UTC"),
];
const FIXED: [&str; 7] = [
    "server.py",
    "viewer.py",
    "radar.py",
    "radar_view.js",
    "requirements.txt",
    "Dockerfile.vault",
    "deploy_probe.py",
];
fn ok(bytes: impl AsRef<[u8]>) -> Captured {
    Captured {
        code: 0,
        stdout: bytes.as_ref().to_vec(),
        stderr: vec![],
    }
}
type Before = Box<dyn FnMut(usize, &CommandSpec)>;

#[derive(Default)]
struct Peer {
    replies: VecDeque<Captured>,
    seen: Vec<CommandSpec>,
    before: Option<Before>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        if let Some(before) = &mut self.before {
            before(self.seen.len(), spec);
        }
        self.seen.push(spec.clone());
        Ok(self.replies.pop_front().expect("unexpected Git command"))
    }
}
fn peer(list: &[u8]) -> Peer {
    let mut p = Peer::default();
    p.replies.extend([ok(format!("{COMMIT}\n")), ok(list)]);
    p
}
fn argv(args: &[&str]) -> Vec<OsString> {
    args.iter().map(|s| OsString::from(*s)).collect()
}
#[test]
fn destination_creation_precedes_ref_resolution_and_existing_dest_runs_no_git() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("missing-repo");
    let dest = t.path().join("nested/bundle");
    let observed = dest.clone();
    let mut p = Peer {
        before: Some(Box::new(move |_, _| assert!(observed.is_dir()))),
        ..Peer::default()
    };
    p.replies.push_back(Captured {
        code: 128,
        stdout: vec![],
        stderr: vec![],
    });
    let e = build(&root, &dest, "bad".as_ref(), &mut p, &mut vec![]).unwrap_err();
    assert_eq!(e.downcast_ref::<GitFailure>().unwrap().code, 128);
    assert!(dest.is_dir());
    let mut p = Peer::default();
    let e = build(&root, &dest, "bad".as_ref(), &mut p, &mut vec![]).unwrap_err();
    assert_eq!(
        e.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(p.seen.is_empty());
}
#[test]
fn selected_parent_is_created_before_failed_show_and_target_is_not_opened() {
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("bundle");
    let target = dest.join("openpilot/cereal/sub/missing.py");
    let observed = target.clone();
    let mut p = peer(b"openpilot/cereal/sub/missing.py\n");
    p.before = Some(Box::new(move |n, _| {
        if n == 2 {
            assert!(observed.parent().unwrap().is_dir());
            assert!(!observed.exists());
        }
    }));
    p.replies.push_back(Captured {
        code: 128,
        stdout: b"partial blob".to_vec(),
        stderr: vec![],
    });
    let mut out = vec![];
    assert!(build(t.path(), &dest, "HEAD".as_ref(), &mut p, &mut out).is_err());
    assert!(target.parent().unwrap().is_dir());
    assert!(!target.exists());
    assert!(out.is_empty());
    assert!(!dest.join("SOURCE_COMMIT").exists());
}
#[test]
fn exact_git_argv_source_order_fixed_order_marker_and_stdout() {
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("bundle");
    let mut p = peer(b"openpilot/cereal/z.capnp\nopenpilot/cereal/a.py\nignored.txt\n");
    p.replies.extend([ok([0, 255, 1]), ok(b"a\n")]);
    for name in FIXED {
        p.replies.push_back(ok(name));
    }
    let mut out = vec![];
    build(t.path(), &dest, "topic".as_ref(), &mut p, &mut out).unwrap();
    assert_eq!(
        p.seen[0].effective_argv(),
        argv(&["git", "rev-parse", "--verify", "topic^{commit}"])
    );
    let mut expected = argv(&["git", "ls-tree", "-r", "--name-only", COMMIT, "--"]);
    expected.extend(SOURCES.map(OsString::from));
    assert_eq!(p.seen[1].effective_argv(), expected);
    assert_eq!(
        p.seen[2].effective_argv(),
        argv(&["git", "show", &format!("{COMMIT}:openpilot/cereal/z.capnp")])
    );
    for (i, name) in FIXED.iter().enumerate() {
        assert_eq!(
            p.seen[i + 4].effective_argv(),
            argv(&[
                "git",
                "show",
                &format!("{COMMIT}:tools/carrot_route_vault/{name}")
            ])
        );
    }
    assert!(p.seen.iter().all(|s| s.cwd.as_deref() == Some(t.path())
        && s.inherit_stderr
        && s.inherit_stdin
        && s.stdout_limit == usize::MAX));
    assert_eq!(
        fs::read(dest.join("openpilot/cereal/z.capnp")).unwrap(),
        [0, 255, 1]
    );
    assert_eq!(
        fs::read(dest.join("SOURCE_COMMIT")).unwrap(),
        format!("{COMMIT}\n").as_bytes()
    );
    assert_eq!(
        out,
        format!("Built {} from {COMMIT}\n", dest.display()).as_bytes()
    );
}
#[test]
fn missing_fixed_file_at_each_position_preserves_only_earlier_copies() {
    for fail in 0..7 {
        let t = tempfile::tempdir().unwrap();
        let dest = t.path().join("bundle");
        let mut p = peer(b"");
        for name in FIXED.iter().take(fail) {
            p.replies.push_back(ok(name));
        }
        p.replies.push_back(Captured {
            code: 128,
            stdout: vec![],
            stderr: vec![],
        });
        let mut out = vec![];
        assert!(build(t.path(), &dest, "HEAD".as_ref(), &mut p, &mut out).is_err());
        for (i, name) in FIXED.iter().enumerate() {
            assert_eq!(dest.join(name).exists(), i < fail);
        }
        assert!(!dest.join("SOURCE_COMMIT").exists());
        assert!(out.is_empty());
        assert_eq!(p.seen.len(), 3 + fail);
    }
}
#[test]
fn unicode_text_splitlines_and_path_suffix_keep_original_quirks() {
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("bundle");
    let mut p=peer(b".py\nkeep.py\r\ntrailing.py.\x0bkeep.capnp\x0cUP.PY\x1ckeep.dbc\x1d\"quoted.py\"\x1eskip.txt\xc2\x85.hidden\xe2\x80\xa8tail.py\xe2\x80\xa9");
    for _ in 0..4 {
        p.replies.push_back(ok(b"blob"));
    }
    for name in FIXED {
        p.replies.push_back(ok(name));
    }
    build(t.path(), &dest, "HEAD".as_ref(), &mut p, &mut vec![]).unwrap();
    for name in ["keep.py", "keep.capnp", "keep.dbc", "tail.py"] {
        assert!(dest.join(name).is_file());
    }
    for name in [
        ".py",
        "trailing.py.",
        "UP.PY",
        "\"quoted.py\"",
        "skip.txt",
        ".hidden",
    ] {
        assert!(!dest.join(name).exists());
    }
}
#[test]
fn undecodable_listing_fails_before_any_blob_or_parent_copy() {
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("bundle");
    let mut p = peer(b"ok.py\nraw-\xff.py\n");
    assert!(build(t.path(), &dest, "HEAD".as_ref(), &mut p, &mut vec![]).is_err());
    assert_eq!(p.seen.len(), 2);
    assert!(fs::read_dir(dest).unwrap().next().is_none());
}
#[test]
fn argparse_fixed_surface_duplicate_abbreviation_help_and_literal_destination() {
    for args in [
        vec!["out"],
        vec!["--ref", "HEAD", "out"],
        vec!["out", "--r=first", "--re", "last"],
        vec!["--", "--ref"],
    ] {
        let p = parse(&argv(&args)).unwrap();
        assert!(!p.help);
        assert_eq!(p.positionals.len(), 1);
        if args.len() == 4 {
            assert_eq!(p.values["--ref"], "last");
        }
    }
    for args in [
        vec![],
        vec!["out", "extra"],
        vec!["out", "--root", "repo"],
        vec!["out", "--ref"],
        vec!["out", "--unknown"],
        vec!["--help=x"],
    ] {
        assert!(parse(&argv(&args)).is_err(), "{args:?}");
    }
    for args in [vec!["--help"], vec!["-h"], vec!["--unknown", "--help"]] {
        assert!(parse(&argv(&args)).unwrap().help);
    }
    let raw = OsString::from_vec(b"raw-\xff".to_vec());
    assert_eq!(
        parse(std::slice::from_ref(&raw)).unwrap().positionals[0],
        raw
    );
}
#[test]
fn stdout_error_occurs_after_final_commit_marker() {
    struct Broken;
    impl io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("bundle");
    let mut p = peer(b"");
    for name in FIXED {
        p.replies.push_back(ok(name));
    }
    let e = build(t.path(), &dest, "HEAD".as_ref(), &mut p, &mut Broken).unwrap_err();
    assert_eq!(
        e.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        fs::read(dest.join("SOURCE_COMMIT")).unwrap(),
        format!("{COMMIT}\n").as_bytes()
    );
}

fn command(root: &Path, args: Vec<OsString>) -> Captured {
    let mut s = CommandSpec::new_os(args, Duration::from_secs(20));
    s.cwd = Some(root.into());
    NativeRunner.run(&s).unwrap()
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let mut a = argv(&["git"]);
    a.extend(argv(args));
    let mut spec = CommandSpec::new_os(a, Duration::from_secs(20));
    spec.cwd = Some(root.into());
    spec.env
        .extend(FIXTURE_GIT_ENV.map(|(name, value)| (name.to_owned(), value.to_owned())));
    // fixture 구성만 고정한다. PATH 등 나머지 환경과 원본/네이티브의 Git 실행은 유지한다.
    spec.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| {
            key.as_bytes().starts_with(b"GIT_")
                && !key.to_str().is_some_and(|name| spec.env.contains_key(name))
        })
        .collect();
    let o = NativeRunner.run(&spec).unwrap();
    assert_eq!(o.code, 0, "{}", String::from_utf8_lossy(&o.stderr));
    o.stdout
}
struct Repo {
    _temp: tempfile::TempDir,
    root: PathBuf,
    commit: String,
    frozen: bundle_frozen_replay::Frozen,
}
impl Repo {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        // tempdir 기본 mode를 가정하지 않고 이 fixture의 재현 경계를 직접 보호한다.
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        git(
            &root,
            &[
                "init",
                "-q",
                "--initial-branch=fixture",
                "--object-format=sha1",
                "--template=",
            ],
        );
        git(&root, &["config", "user.name", "Fixture"]);
        git(&root, &["config", "user.email", "fixture@example.invalid"]);
        git(&root, &["config", "core.autocrlf", "false"]);
        git(&root, &["config", "core.quotePath", "true"]);
        git(&root, &["config", "core.hooksPath", "/dev/null"]);
        git(&root, &["config", "commit.gpgSign", "false"]);
        git(&root, &["config", "commit.cleanup", "verbatim"]);
        git(&root, &["config", "core.attributesFile", "/dev/null"]);
        let files = [
            (
                "openpilot/common/filter_simple.py",
                b"committed\r\n\0\xff".as_slice(),
            ),
            ("openpilot/cereal/a.capnp", b"schema"),
            ("opendbc_repo/opendbc/dbc/a.dbc", b"dbc"),
            ("openpilot/cereal/no.txt", b"ignored"),
            ("outside/hidden.py", b"excluded"),
            ("openpilot/cereal/space name.py", b"space"),
            ("openpilot/cereal/unicode-한.py", b"quoted skipped"),
            ("openpilot/cereal/break\u{2028}tail.py", b"splitline quirk"),
            ("openpilot/cereal/new\nline.py", b"quoted skipped"),
            ("openpilot/cereal/.py", b"hidden skipped"),
        ];
        for (name, blob) in files {
            let p = root.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, blob).unwrap();
        }
        for name in FIXED {
            let p = root.join("tools/carrot_route_vault").join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, format!("fixed {name}\0\n")).unwrap();
        }
        symlink("nonexistent-target", root.join("openpilot/cereal/link.py")).unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "fixture"]);
        let commit = String::from_utf8(git(&root, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .into();
        // 초기 Git tree/commit 뒤의 untracked script 복사만 제거한다. fixture 파일/날짜는 같다.
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let frozen = bundle_frozen_replay::Frozen::open(&source_root, output_snapshot);
        assert!(git(&root, &["ls-files", "--", ORIGINAL]).is_empty());
        Self {
            _temp: temp,
            root,
            commit,
            frozen,
        }
    }
    fn native(&self, dest: &Path, reference: &str) -> (bool, Vec<u8>) {
        let mut out = vec![];
        let success = build(
            &self.root,
            dest,
            reference.as_ref(),
            &mut NativeRunner,
            &mut out,
        )
        .is_ok();
        (success, out)
    }
    fn original(&self, dest: &Path, reference: &str) -> Captured {
        self.original_args(
            &self.root,
            vec![dest.as_os_str().into(), "--ref".into(), reference.into()],
            dest,
            Some(reference),
        )
    }
    fn original_args(
        &self,
        cwd: &Path,
        args: Vec<OsString>,
        dest_scope: &Path,
        reference: Option<&str>,
    ) -> Captured {
        let head = String::from_utf8(git(&self.root, &["rev-parse", "HEAD"])).unwrap();
        self.frozen.replay(bundle_frozen_replay::Input {
            repo: &self.root,
            initial: &self.commit,
            head: head.trim(),
            temp: self._temp.path(),
            cwd,
            args: &args,
            dest: dest_scope,
            reference,
        })
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    output
}
fn raw_path(path: &Path) -> Value {
    json!({"bytes_hex":hex(path.as_os_str().as_bytes()), "utf8":path.to_str()})
}
fn output_snapshot(root: &Path) -> Value {
    fn visit(root: &Path, at: &Path, rows: &mut Vec<Value>, total: &mut usize) {
        assert!(rows.len() < 128, "synthetic output entry limit");
        let metadata = match fs::symlink_metadata(at) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound && at == root => {
                rows.push(json!({"relative_path":raw_path(Path::new("")), "kind":"missing"}));
                return;
            }
            Err(error) => panic!("cannot snapshot synthetic output: {error}"),
        };
        let mut row = json!({"relative_path":raw_path(at.strip_prefix(root).unwrap()), "mode":metadata.mode()});
        if metadata.is_dir() {
            row["kind"] = json!("directory");
            rows.push(row);
            let mut children = fs::read_dir(at)
                .unwrap()
                .take(129)
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            assert!(
                children.len() <= 128 - rows.len(),
                "synthetic output entry limit"
            );
            children.sort();
            for child in children {
                visit(root, &child, rows, total);
            }
        } else if metadata.is_file() {
            let file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(
                    (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
                )
                .open(at)
                .unwrap();
            let opened = file.metadata().unwrap();
            assert!(opened.is_file());
            assert_eq!(
                (opened.dev(), opened.ino()),
                (metadata.dev(), metadata.ino())
            );
            assert!(opened.len() <= 1024 * 1024, "synthetic output file limit");
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes).unwrap();
            assert!(
                bytes.len() <= 1024 * 1024,
                "synthetic output file grew beyond limit"
            );
            *total += bytes.len();
            assert!(*total <= 2 * 1024 * 1024, "synthetic output total limit");
            row["kind"] = json!("file");
            row["bytes_hex"] = json!(hex(&bytes));
            row["sha256"] = json!(digest(&bytes));
            rows.push(row);
        } else if metadata.file_type().is_symlink() {
            row["kind"] = json!("symlink");
            row["target"] = raw_path(&fs::read_link(at).unwrap());
            rows.push(row);
        } else {
            panic!("nonregular synthetic output");
        }
    }
    let mut rows = Vec::new();
    let mut total = 0;
    visit(root, root, &mut rows, &mut total);
    json!({"entries":rows, "file_bytes":total})
}
fn snapshot(root: &Path) -> BTreeMap<Vec<u8>, (bool, Vec<u8>)> {
    fn visit(root: &Path, at: &Path, out: &mut BTreeMap<Vec<u8>, (bool, Vec<u8>)>) {
        for e in fs::read_dir(at).unwrap() {
            let p = e.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            assert!(!m.file_type().is_symlink());
            let name = p
                .strip_prefix(root)
                .unwrap()
                .as_os_str()
                .as_bytes()
                .to_vec();
            let dir = m.is_dir();
            out.insert(
                name,
                (dir, if dir { vec![] } else { fs::read(&p).unwrap() }),
            );
            if dir {
                visit(root, &p, out);
            }
        }
    }
    let mut out = BTreeMap::new();
    if root.is_dir() {
        visit(root, root, &mut out);
    }
    out
}
#[test]
fn synthetic_original_native_committed_bytes_types_filter_and_worktree_exclusion() {
    let r = Repo::new();
    fs::write(
        r.root.join("openpilot/common/filter_simple.py"),
        b"dirty worktree",
    )
    .unwrap();
    fs::write(
        r.root.join("tools/carrot_route_vault/server.py"),
        b"dirty server",
    )
    .unwrap();
    fs::write(r.root.join("openpilot/cereal/untracked.py"), b"untracked").unwrap();
    git(&r.root, &["add", "openpilot/common/filter_simple.py"]);
    let old = r._temp.path().join("old");
    let new = r._temp.path().join("new");
    let o = r.original(&old, &r.commit);
    let (success, stdout) = r.native(&new, &r.commit);
    assert_eq!(o.code, 0, "{}", String::from_utf8_lossy(&o.stderr));
    assert!(success);
    assert_eq!(snapshot(&old), snapshot(&new));
    assert_eq!(
        stdout,
        format!("Built {} from {}\n", new.display(), r.commit).as_bytes()
    );
    assert_eq!(
        fs::read(new.join("openpilot/common/filter_simple.py")).unwrap(),
        b"committed\r\n\0\xff"
    );
    let link = new.join("openpilot/cereal/link.py");
    assert!(fs::symlink_metadata(&link).unwrap().is_file());
    assert_eq!(fs::read(link).unwrap(), b"nonexistent-target");
    assert!(!new.join("openpilot/cereal/unicode-한.py").exists());
    assert!(!new.join("openpilot/cereal/untracked.py").exists());
}
#[test]
fn synthetic_original_native_invalid_ref_existing_destination_and_partial_failure() {
    for case in ["bad-ref", "existing", "quotePath-false"] {
        let mut r = Repo::new();
        let old = r._temp.path().join("old");
        let new = r._temp.path().join("new");
        let reference = if case == "bad-ref" {
            "no-such-ref"
        } else {
            "HEAD"
        };
        if case == "existing" {
            fs::create_dir(&old).unwrap();
            fs::create_dir(&new).unwrap();
        }

        if case == "quotePath-false" {
            git(&r.root, &["config", "core.quotePath", "false"]);
        }
        r.commit = String::from_utf8(git(&r.root, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .into();
        let o = r.original(&old, reference);
        let (success, out) = r.native(&new, reference);
        assert_eq!(success, o.code == 0, "{case}");
        assert_eq!(snapshot(&old), snapshot(&new), "{case}");
        if !success {
            assert!(out.is_empty());
            assert!(!new.join("SOURCE_COMMIT").exists());
        }
    }
}

#[test]
fn text_decode_error_precedes_nonzero_child_status_like_check_output() {
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("bundle");
    let mut p = Peer::default();
    p.replies.push_back(Captured {
        code: 128,
        stdout: vec![255],
        stderr: vec![],
    });
    let e = build(t.path(), &dest, "HEAD".as_ref(), &mut p, &mut vec![]).unwrap_err();
    assert!(e.downcast_ref::<std::str::Utf8Error>().is_some());
    assert!(dest.is_dir());
}
#[test]
fn existing_file_or_symlink_directory_is_exclusive_before_git() {
    for kind in ["file", "directory-link", "dangling-link"] {
        let t = tempfile::tempdir().unwrap();
        let dest = t.path().join("bundle");
        match kind {
            "file" => fs::write(&dest, b"keep").unwrap(),
            "directory-link" => symlink(t.path(), &dest).unwrap(),
            _ => symlink("missing", &dest).unwrap(),
        }
        let mut p = Peer::default();
        assert!(build(t.path(), &dest, "bad".as_ref(), &mut p, &mut vec![]).is_err());
        assert!(p.seen.is_empty());
        assert!(fs::symlink_metadata(&dest).is_ok());
    }
}
#[test]
fn failing_output_open_occurs_only_after_blob_and_marker_failure_has_no_success() {
    for marker in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let dest = t.path().join("bundle");
        let observed = dest.clone();
        let mut p = peer(if marker { b"" } else { b"selected.py\n" });
        if marker {
            for name in FIXED {
                p.replies.push_back(ok(name));
            }
        } else {
            p.replies.push_back(ok(b"blob"));
        }
        p.before = Some(Box::new(move |n, _| {
            if marker && n == 8 {
                fs::create_dir(observed.join("SOURCE_COMMIT")).unwrap();
            }
            if !marker && n == 2 {
                fs::create_dir(observed.join("selected.py")).unwrap();
            }
        }));
        let mut out = vec![];
        let e = build(t.path(), &dest, "HEAD".as_ref(), &mut p, &mut out).unwrap_err();
        assert_eq!(
            e.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::IsADirectory
        );
        assert!(out.is_empty());
        if marker {
            assert!(dest.join("deploy_probe.py").is_file());
        }
    }
}
#[test]
fn cli_help_unknowns_and_compiled_source_root_do_not_follow_invocation_cwd() {
    let r = Repo::new();
    let dest = r._temp.path().join("cli-output");
    let mut unique = OsString::from("fixture-only-");
    unique.push(r._temp.path().file_name().unwrap());
    let branch = unique.to_str().unwrap();
    git(&r.root, &["branch", branch]);
    let mut a = vec![OsString::from(env!(
        "CARGO_BIN_EXE_build-route-vault-bundle"
    ))];
    a.extend([dest.as_os_str().into(), "--ref".into(), unique]);
    let output = command(&r.root, a);
    assert_eq!(output.code, 1);
    assert!(dest.is_dir());
    assert!(output.stdout.is_empty());
    assert!(fs::read_dir(&dest).unwrap().next().is_none());
    for (args, code) in [
        (vec!["--help"], 0),
        (vec!["out", "--unknown"], 2),
        (vec!["out", "--ref"], 2),
    ] {
        let mut a = vec![OsString::from(env!(
            "CARGO_BIN_EXE_build-route-vault-bundle"
        ))];
        a.extend(argv(&args));
        assert_eq!(command(&r.root, a).code, code);
        assert!(!r.root.join("out").exists());
    }
}

#[test]
fn lexical_raw_destination_stdout_is_not_lossy_or_resolved() {
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join(OsString::from_vec(b"raw-\xff".to_vec()));
    let mut raw = dest.as_os_str().as_bytes().to_vec();
    raw.extend_from_slice(b"//./");
    let raw = PathBuf::from(OsString::from_vec(raw));
    let mut p = peer(b"");
    for name in FIXED {
        p.replies.push_back(ok(name));
    }
    let mut out = vec![];
    build(t.path(), &raw, "HEAD".as_ref(), &mut p, &mut out).unwrap();
    let mut expected = b"Built ".to_vec();
    expected.extend_from_slice(dest.as_os_str().as_bytes());
    expected.extend_from_slice(format!(" from {COMMIT}\n").as_bytes());
    assert_eq!(out, expected);
    assert!(dest.join("SOURCE_COMMIT").is_file());
}
#[test]
fn synthetic_original_native_all_seven_fixed_failures_match_partial_effects() {
    for missing in FIXED {
        let r = Repo::new();
        git(
            &r.root,
            &["rm", &format!("tools/carrot_route_vault/{missing}")],
        );
        git(&r.root, &["commit", "-qm", "missing fixed"]);
        let old = r._temp.path().join("old");
        let new = r._temp.path().join("new");
        let o = r.original(&old, "HEAD");
        let (success, stdout) = r.native(&new, "HEAD");
        assert_eq!(o.code, 1);
        assert!(!success);
        assert_eq!(snapshot(&old), snapshot(&new), "{missing}");
        assert!(stdout.is_empty() && o.stdout.is_empty());
        assert!(!new.join(missing).exists());
        assert!(!new.join("SOURCE_COMMIT").exists());
    }
}

#[test]
fn actual_original_and_native_argparse_exit_and_creation_effects_match() {
    let r = Repo::new();
    let bad = "refs/heads/nonexistent-route-vault-bundle-parser-fixture";
    let cases = vec![
        vec![],
        vec!["--help"],
        vec!["--unknown", "--help"],
        vec!["dest", "--unknown"],
        vec!["dest", "--ref"],
        vec!["dest", "extra"],
        vec!["--help=x"],
        vec!["dest", "--ref", bad],
        vec!["--ref", "HEAD", "dest", "--ref", bad],
        vec!["dest", "--re", bad],
        vec!["--ref", bad, "--", "--ref"],
        vec!["--ref", bad, "-1"],
    ];
    for (i, args) in cases.iter().enumerate() {
        let old_cwd = r._temp.path().join(format!("parser-{i}-old"));
        let new_cwd = r._temp.path().join(format!("parser-{i}-new"));
        fs::create_dir(&old_cwd).unwrap();
        fs::create_dir(&new_cwd).unwrap();
        let o = r.original_args(&old_cwd, argv(args), &old_cwd, None);
        let mut new = vec![OsString::from(env!(
            "CARGO_BIN_EXE_build-route-vault-bundle"
        ))];
        new.extend(argv(args));
        let n = command(&new_cwd, new);
        assert_eq!(n.code, o.code, "{args:?}");
        assert_eq!(snapshot(&old_cwd), snapshot(&new_cwd), "{args:?}");
        assert!(!n.stdout.starts_with(b"Built ") && !o.stdout.starts_with(b"Built "));
    }
}

#[path = "route_vault_bundle/frozen.rs"]
mod bundle_frozen_replay;
