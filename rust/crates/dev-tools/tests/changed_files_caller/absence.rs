// UNRUN: 실제 source-absence/held-file 경계의 Actions 회귀. 원본을 실행하지 않는다.
use rustix::fs::{openat, statat, AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

const RETIRED: [&str; 2] = [
    "tools/ci/check_changed_files.py",
    "tools/ci/tests/test_changed_files.py",
];
const DATA_TREE: &str = "rust/crates/dev-tools/src/changed_files_check";
const SOURCE_FILES: &[&str] = &[
    "rust/Cargo.toml",
    "rust/Cargo.lock",
    "rust/rust-toolchain.toml",
    "rust/crates/dev-tools/Cargo.toml",
    "rust/crates/dev-tools/src/lib.rs",
    "rust/crates/dev-tools/src/qa_process.rs",
    "rust/crates/dev-tools/src/ci_policy.rs",
    "rust/crates/dev-tools/src/bin/check_changed_files.rs",
    "rust/crates/dev-tools/src/changed_files_check.rs",
    "rust/crates/dev-tools/tests/changed_files_caller.rs",
    "rust/crates/dev-tools/tests/changed_files_caller/absence.rs",
    ".github/workflows/fast-checks.yml",
];
const CONTEXT_LIMIT: u64 = 64 * 1024;
const SOURCE_LIMIT: u64 = 8 * 1024 * 1024;
const ELF_LIMIT: u64 = 512 * 1024 * 1024;
const SOURCE_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);
const READ_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Identity {
    device: u64,
    inode: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    bytes: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
fn identity(metadata: &fs::Metadata) -> Identity {
    Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
        mode: metadata.mode(),
        uid: metadata.uid(),
        gid: metadata.gid(),
        bytes: metadata.len(),
        mtime: (metadata.mtime(), metadata.mtime_nsec()),
        ctime: (metadata.ctime(), metadata.ctime_nsec()),
    }
}
fn directory_identity(metadata: &fs::Metadata) -> (u64, u64, u32, u32, u32) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.mode(),
        metadata.uid(),
        metadata.gid(),
    )
}
fn digest_is_valid(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
// 모든 path component를 openat/NOFOLLOW로 연다. 마지막 이름만 검사하지 않는다.
fn open_path(path: &Path, final_flags: OFlags) -> Result<File, String> {
    if !path.is_absolute() {
        return Err(format!("absolute path required: {}", path.display()));
    }
    let mut file = File::from(
        rustix::fs::open("/", DIRECTORY_FLAGS, Mode::empty()).map_err(|e| e.to_string())?,
    );
    let mut parts = path.components().peekable();
    while let Some(part) = parts.next() {
        let Component::Normal(name) = part else {
            if part == Component::RootDir {
                continue;
            }
            return Err(format!("non-normal path component: {}", path.display()));
        };
        let flags = if parts.peek().is_some() {
            DIRECTORY_FLAGS
        } else {
            final_flags
        };
        file = File::from(
            openat(&file, name, flags, Mode::empty())
                .map_err(|e| format!("{}: {e}", path.display()))?,
        );
    }
    Ok(file)
}
fn require_absent(root: &Path) -> Result<(), String> {
    let root_file = open_path(root, DIRECTORY_FLAGS)?;
    for relative in RETIRED {
        let mut parent = root_file.try_clone().map_err(|e| e.to_string())?;
        let mut parts = Path::new(relative).iter().peekable();
        while let Some(name) = parts.next() {
            if parts.peek().is_none() {
                match statat(&parent, name, AtFlags::SYMLINK_NOFOLLOW) {
                    Err(rustix::io::Errno::NOENT) => {}
                    Ok(_) => {
                        return Err(format!(
                            "retired path exists (including directory/link): {relative}"
                        ))
                    }
                    Err(e) => return Err(format!("absence check {relative}: {e}")),
                }
            } else {
                match openat(&parent, name, DIRECTORY_FLAGS, Mode::empty()) {
                    Ok(fd) => parent = File::from(fd),
                    Err(rustix::io::Errno::NOENT) => break,
                    Err(e) => return Err(format!("absence parent {relative}: {e}")),
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Seal {
    identity: Identity,
    sha256: String,
}
fn seal(file: &mut File, limit: u64) -> Result<Seal, String> {
    let before = file.metadata().map_err(|e| e.to_string())?;
    if !before.is_file() || before.len() > limit {
        return Err("bounded regular file required".into());
    }
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut reader = Read::by_ref(file).take(limit + 1);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 65536];
    let mut bytes = 0;
    loop {
        let size = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        bytes += size as u64;
        hasher.update(&buffer[..size]);
    }
    let after = file.metadata().map_err(|e| e.to_string())?;
    if bytes > limit || bytes != before.len() || identity(&before) != identity(&after) {
        return Err("file changed during bounded read".into());
    }
    Ok(Seal {
        identity: identity(&before),
        sha256: format!("{:x}", hasher.finalize()),
    })
}
struct HeldFile {
    path: PathBuf,
    file: File,
    before: Seal,
    limit: u64,
}
impl HeldFile {
    fn open(path: &Path, limit: u64) -> Result<Self, String> {
        let mut file = open_path(path, READ_FLAGS)?;
        let before = seal(&mut file, limit)?;
        let named = open_path(path, READ_FLAGS)?;
        if identity(&named.metadata().map_err(|e| e.to_string())?) != before.identity {
            return Err(format!("path changed during open: {}", path.display()));
        }
        Ok(Self {
            path: path.into(),
            file,
            before,
            limit,
        })
    }
    fn verify(&mut self) -> Result<(), String> {
        let after = seal(&mut self.file, self.limit)?;
        let named = open_path(&self.path, READ_FLAGS)?;
        if after != self.before
            || identity(&named.metadata().map_err(|e| e.to_string())?) != self.before.identity
        {
            return Err(format!(
                "held bytes/identity/path changed: {}",
                self.path.display()
            ));
        }
        Ok(())
    }
}

struct Tree {
    root: PathBuf,
    files: BTreeSet<PathBuf>,
    directories: BTreeMap<PathBuf, (File, Identity)>,
}
impl Tree {
    fn capture(root: &Path) -> Result<Self, String> {
        let mut tree = Self {
            root: root.into(),
            files: BTreeSet::new(),
            directories: BTreeMap::new(),
        };
        tree.visit(root, 0)?;
        Ok(tree)
    }
    fn visit(&mut self, path: &Path, depth: usize) -> Result<(), String> {
        if depth > 16 || self.files.len() + self.directories.len() >= MAX_ENTRIES {
            return Err("source tree bounds exceeded".into());
        }
        let directory = open_path(path, DIRECTORY_FLAGS)?;
        let before = identity(&directory.metadata().map_err(|e| e.to_string())?);
        self.directories
            .insert(path.into(), (directory, before.clone()));
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            if self.files.len() + self.directories.len() >= MAX_ENTRIES {
                return Err("source roster bounds exceeded".into());
            }
            let path = entry.map_err(|e| e.to_string())?.path();
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.is_dir() {
                self.visit(&path, depth + 1)?;
            } else if metadata.is_file() {
                self.files.insert(path);
            } else {
                return Err(format!(
                    "regular data or unlinked directory required: {}",
                    path.display()
                ));
            }
        }
        let named = open_path(path, DIRECTORY_FLAGS)?;
        let (held, _) = &self.directories[path];
        if identity(&named.metadata().map_err(|e| e.to_string())?) != before
            || identity(&held.metadata().map_err(|e| e.to_string())?) != before
        {
            return Err(format!(
                "directory changed while listing: {}",
                path.display()
            ));
        }
        Ok(())
    }
    fn verify(&self) -> Result<(), String> {
        let after = Self::capture(&self.root)?;
        if self.files != after.files || self.directories.keys().ne(after.directories.keys()) {
            return Err("source data roster changed".into());
        }
        for (path, (file, before)) in &self.directories {
            if identity(&file.metadata().map_err(|e| e.to_string())?) != *before
                || after.directories[path].1 != *before
            {
                return Err(format!("held directory changed: {}", path.display()));
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramPin {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    schema: String,
    source_commit: String,
    source_root: PathBuf,
    proof_dir: PathBuf,
    source_sha256: BTreeMap<String, String>,
    cli: ProgramPin,
    test_elf: ProgramPin,
}
pub(super) struct Guard {
    ready: bool,
    test: &'static str,
    context: Context,
    context_sha256: String,
    held: Vec<HeldFile>,
    tree: Tree,
    proof: File,
    proof_identity: (u64, u64, u32, u32, u32),
}
impl Guard {
    pub(super) fn current(test: &'static str) -> Self {
        Self::load(test)
            .unwrap_or_else(|e| panic!("changed-files caller context/absence guard: {e}"))
    }
    fn load(test: &'static str) -> Result<Self, String> {
        let context_path = std::env::var_os("CHANGED_FILES_CALLER_CONTEXT")
            .ok_or("mandatory CHANGED_FILES_CALLER_CONTEXT")?;
        let context_sha256 =
            std::env::var("CHANGED_FILES_CALLER_CONTEXT_SHA256").map_err(|e| e.to_string())?;
        let expected_source = std::env::var("EXPECTED_SOURCE").map_err(|e| e.to_string())?;
        if !digest_is_valid(&context_sha256, 64) || !digest_is_valid(&expected_source, 40) {
            return Err("invalid context/source pin".into());
        }
        let mut context_file = HeldFile::open(Path::new(&context_path), CONTEXT_LIMIT)?;
        if context_file.before.sha256 != context_sha256 {
            return Err("context SHA mismatch".into());
        }
        context_file.file.rewind().map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut context_file.file)
            .take(CONTEXT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > CONTEXT_LIMIT
            || format!("{:x}", Sha256::digest(&bytes)) != context_sha256
        {
            return Err("context changed while parsing".into());
        }
        context_file.verify()?;
        let context: Context = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let compiled_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .ok_or("manifest root")?;
        if context.schema != "openpilot.changed-files-caller-context.v1"
            || context.source_commit != expected_source
            || context.source_root != compiled_root
        {
            return Err("context schema/source/compiled root mismatch".into());
        }
        if context.cli.path != Path::new(env!("CARGO_BIN_EXE_check-changed-files"))
            || context.test_elf.path != std::env::current_exe().map_err(|e| e.to_string())?
        {
            return Err("context must pin the actual Cargo CLI/current test ELF".into());
        }
        require_absent(&context.source_root)?;
        let tree = Tree::capture(&context.source_root.join(DATA_TREE))?;
        let mut roster: BTreeSet<String> = SOURCE_FILES.iter().map(|s| (*s).into()).collect();
        for path in &tree.files {
            roster.insert(
                path.strip_prefix(&context.source_root)
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("non-UTF8 source data path")?
                    .into(),
            );
        }
        if roster.len() > MAX_ENTRIES || roster.iter().ne(context.source_sha256.keys()) {
            return Err("context source roster mismatch".into());
        }
        let mut held = vec![context_file];
        let mut source_bytes = 0;
        for relative in roster {
            let file = HeldFile::open(&context.source_root.join(&relative), SOURCE_LIMIT)?;
            let pin = &context.source_sha256[&relative];
            if !digest_is_valid(pin, 64) || file.before.sha256 != *pin {
                return Err(format!("current source SHA mismatch: {relative}"));
            }
            source_bytes += file.before.identity.bytes;
            if source_bytes > SOURCE_TOTAL_LIMIT {
                return Err("source byte budget exceeded".into());
            }
            held.push(file);
        }
        for program in [&context.cli, &context.test_elf] {
            let file = HeldFile::open(&program.path, ELF_LIMIT)?;
            if !digest_is_valid(&program.sha256, 64) || file.before.sha256 != program.sha256 {
                return Err("current program SHA mismatch".into());
            }
            held.push(file);
        }
        if context.proof_dir.starts_with(&context.source_root) {
            return Err("proof must be outside source tree".into());
        }
        let proof = open_path(&context.proof_dir, DIRECTORY_FLAGS)?;
        let metadata = proof.metadata().map_err(|e| e.to_string())?;
        if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o022 != 0 {
            return Err("owned proof directory without group/other writes required".into());
        }
        let mut guard = Self {
            ready: false,
            test,
            context,
            context_sha256,
            held,
            tree,
            proof,
            proof_identity: directory_identity(&metadata),
        };
        guard.verify_after()?;
        guard.ready = true;
        Ok(guard)
    }
    fn verify_after(&mut self) -> Result<(), String> {
        require_absent(&self.context.source_root)?;
        self.tree.verify()?;
        for file in &mut self.held {
            file.verify()?;
        }
        let named = open_path(&self.context.proof_dir, DIRECTORY_FLAGS)?;
        if directory_identity(&named.metadata().map_err(|e| e.to_string())?) != self.proof_identity
            || directory_identity(&self.proof.metadata().map_err(|e| e.to_string())?)
                != self.proof_identity
        {
            return Err("owned proof directory identity changed".into());
        }
        require_absent(&self.context.source_root)
    }
    fn complete(&mut self) -> Result<(), String> {
        self.verify_after()?;
        let receipt = json!({
            "schema": "openpilot.changed-files-caller-absence.v1", "test": self.test,
            "source_commit": self.context.source_commit, "context_sha256": self.context_sha256,
            "source_root": self.context.source_root, "cli": self.context.cli.path,
            "test_elf": self.context.test_elf.path,
            "retired_paths": RETIRED, "source_absent_before_and_after": true,
            "source_data_roster_unchanged": true, "held_files_unchanged": true,
            "held_files": self.held.iter().map(|f| json!({"path": f.path, "seal": f.before})).collect::<Vec<_>>(),
            "data_directories": self.tree.directories.iter().map(|(path, (_, before))| json!({"path": path, "identity": before})).collect::<Vec<_>>(),
            "passed": true, "fresh_original_executions": 0, "historical_admission": false,
        });
        let mut bytes = serde_json::to_vec(&receipt).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if bytes.len() > 512 * 1024 {
            return Err("receipt budget exceeded".into());
        }
        let name = format!("{}.json", self.test);
        let fd = openat(
            &self.proof,
            name.as_str(),
            OFlags::WRONLY
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|e| e.to_string())?;
        let mut file = File::from(fd);
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            let _ = rustix::fs::unlinkat(&self.proof, name.as_str(), AtFlags::empty());
            return Err(error.to_string());
        }
        println!("CHANGED_FILES_CALLER_ABSENCE {receipt}");
        Ok(())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.ready {
            return;
        }
        if std::thread::panicking() {
            if let Err(error) = self.verify_after() {
                eprintln!("changed-files after guard also failed: {error}");
            }
            return; // 원본 test panic 뒤에는 PASS receipt를 쓰지 않는다.
        }
        self.complete()
            .expect("changed-files caller after guard/receipt failed");
    }
}

#[test]
fn absence_controls_reject_restored_paths_and_held_mutation() {
    let absence = Guard::current("absence_controls_reject_restored_paths_and_held_mutation");
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    std::fs::create_dir_all(root.join("tools/ci/tests")).unwrap();
    assert!(require_absent(root).is_ok());
    for relative in RETIRED {
        let path = root.join(relative);
        std::fs::write(&path, b"restored control").unwrap();
        assert!(require_absent(root).is_err(), "accepted file {relative}");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            require_absent(root).is_err(),
            "accepted directory {relative}"
        );
        std::fs::remove_dir(&path).unwrap();
        std::os::unix::fs::symlink("missing-control-target", &path).unwrap();
        assert!(
            require_absent(root).is_err(),
            "accepted broken link {relative}"
        );
        std::fs::remove_file(&path).unwrap();
        assert!(require_absent(root).is_ok());
    }
    let path = root.join("held-control");
    std::fs::write(&path, b"before-0").unwrap();
    let mut held = HeldFile::open(&path, SOURCE_LIMIT).unwrap();
    assert!(held.verify().is_ok());
    // 같은 길이 덮어쓰기와 경로 교체 모두 거부해야 한다.
    std::fs::write(&path, b"after--0").unwrap();
    assert!(held.verify().is_err(), "accepted held-file mutation");
    std::fs::write(&path, b"before-0").unwrap();
    let mut held = HeldFile::open(&path, SOURCE_LIMIT).unwrap();
    let replacement = root.join("replacement-control");
    std::fs::write(&replacement, b"before-0").unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert!(
        held.verify().is_err(),
        "accepted replaced held-file identity"
    );

    let tree_path = root.join("data-control");
    std::fs::create_dir(&tree_path).unwrap();
    let tree = Tree::capture(&tree_path).unwrap();
    assert!(tree.verify().is_ok());
    std::fs::write(tree_path.join("added"), b"new roster entry").unwrap();
    assert!(tree.verify().is_err(), "accepted changed data roster");
    std::fs::remove_file(tree_path.join("added")).unwrap();
    std::os::unix::fs::symlink(root.join("tools"), tree_path.join("linked")).unwrap();
    assert!(
        Tree::capture(&tree_path).is_err(),
        "accepted linked data directory"
    );

    // unwind 중 Drop은 검증하더라도 성공 receipt를 만들면 안 된다.
    let panic_receipt = absence.context.proof_dir.join("panic-control.json");
    assert!(
        matches!(fs::symlink_metadata(&panic_receipt), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
    let panic = std::panic::catch_unwind(|| {
        let _guard = Guard::current("panic-control");
        std::panic::panic_any("intentional absence receipt control");
    })
    .expect_err("panic control did not unwind");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"intentional absence receipt control")
    );
    assert!(
        matches!(fs::symlink_metadata(&panic_receipt), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
}
