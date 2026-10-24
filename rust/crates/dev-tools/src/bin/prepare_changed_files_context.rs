//! 현재 Cargo artifact와 source bytes만 pin하는 CI metadata builder. 원본/후보 실행 없음.
#[cfg(target_os = "linux")]
mod linux {
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs::{self, File, OpenOptions},
        io::{Read, Seek, Write},
        os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
        path::{Component, Path, PathBuf},
        process::Command,
    };

    // Linux asm-generic/fcntl.h; bin-actions x86_64와 aarch64의 동일한 flag 값.
    const NOFOLLOW_NONBLOCK: i32 = 0o400000 | 0o4000;
    const DIRECTORY: i32 = 0o200000;
    const JSON_LIMIT: u64 = 32 * 1024 * 1024;
    const SOURCE_LIMIT: u64 = 8 * 1024 * 1024;
    const ELF_LIMIT: u64 = 512 * 1024 * 1024;
    const DATA_TREE: &str = "rust/crates/dev-tools/src/changed_files_check";
    // tests/changed_files_caller/absence.rs와 같은 exact source roster.
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
    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    type Identity = (u64, u64, u32, u32, u32, u64, i64, i64, i64, i64);
    fn identity(m: &fs::Metadata) -> Identity {
        (
            m.dev(),
            m.ino(),
            m.mode(),
            m.uid(),
            m.gid(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    }
    fn open(path: &Path, directory: bool) -> Result<File> {
        if !path.is_absolute() {
            return Err("absolute path required".into());
        }
        let options = |directory| {
            let mut options = OpenOptions::new();
            options
                .read(true)
                .custom_flags(NOFOLLOW_NONBLOCK | if directory { DIRECTORY } else { 0 });
            options
        };
        let mut file = options(true).open("/")?;
        let mut parts = path.components().peekable();
        while let Some(part) = parts.next() {
            let Component::Normal(name) = part else {
                if part == Component::RootDir {
                    continue;
                }
                return Err("non-normal path component".into());
            };
            // procfs의 현재 소유 FD만 anchor로 사용하고 사용자 component는 NOFOLLOW한다.
            let child = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd())).join(name);
            file = options(parts.peek().is_some() || directory).open(child)?;
        }
        Ok(file)
    }
    fn seal(file: &mut File, limit: u64) -> Result<(Identity, String)> {
        let before = file.metadata()?;
        if !before.is_file() || before.len() > limit {
            return Err("bounded regular file required".into());
        }
        file.rewind()?;
        let mut reader = Read::by_ref(file).take(limit + 1);
        let mut buffer = [0_u8; 65536];
        let mut hash = Sha256::new();
        let mut count = 0;
        loop {
            let length = reader.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            hash.update(&buffer[..length]);
            count += length as u64;
        }
        if count > limit
            || count != before.len()
            || identity(&file.metadata()?) != identity(&before)
        {
            return Err("file changed during bounded read".into());
        }
        Ok((identity(&before), format!("{:x}", hash.finalize())))
    }
    struct Held {
        path: PathBuf,
        file: File,
        before: (Identity, String),
        limit: u64,
    }
    impl Held {
        fn new(path: &Path, limit: u64) -> Result<Self> {
            let mut file = open(path, false)?;
            let before = seal(&mut file, limit)?;
            let held = Self {
                path: path.into(),
                file,
                before,
                limit,
            };
            if identity(&open(path, false)?.metadata()?) != held.before.0 {
                return Err("path changed during open".into());
            }
            Ok(held)
        }
        fn verify(&mut self) -> Result<()> {
            if seal(&mut self.file, self.limit)? != self.before
                || identity(&open(&self.path, false)?.metadata()?) != self.before.0
            {
                return Err(format!("held bytes/identity changed: {}", self.path.display()).into());
            }
            Ok(())
        }
        fn contents(&mut self) -> Result<Vec<u8>> {
            self.file.rewind()?;
            let mut bytes = Vec::new();
            Read::by_ref(&mut self.file)
                .take(self.limit + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > self.limit
                || format!("{:x}", Sha256::digest(&bytes)) != self.before.1
            {
                return Err("held input changed while parsing".into());
            }
            self.verify()?;
            Ok(bytes)
        }
        fn elf(&mut self) -> Result<()> {
            self.file.rewind()?;
            let mut magic = [0_u8; 4];
            self.file.read_exact(&mut magic)?;
            if magic != *b"\x7fELF" || self.before.0 .2 & 0o111 == 0 {
                return Err("actual executable ELF required".into());
            }
            self.verify()
        }
    }
    struct Tree {
        root: PathBuf,
        files: BTreeSet<PathBuf>,
        dirs: BTreeMap<PathBuf, (File, Identity)>,
    }
    impl Tree {
        fn new(root: &Path) -> Result<Self> {
            let mut tree = Self {
                root: root.into(),
                files: BTreeSet::new(),
                dirs: BTreeMap::new(),
            };
            tree.visit(root, 0)?;
            Ok(tree)
        }
        fn visit(&mut self, path: &Path, depth: usize) -> Result<()> {
            if depth > 16 || self.files.len() + self.dirs.len() >= 256 {
                return Err("data tree bound exceeded".into());
            }
            let directory = open(path, true)?;
            let before = identity(&directory.metadata()?);
            self.dirs.insert(path.into(), (directory, before));
            for entry in fs::read_dir(path)? {
                if self.files.len() + self.dirs.len() >= 256 {
                    return Err("data roster bound exceeded".into());
                }
                let path = entry?.path();
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.is_dir() {
                    self.visit(&path, depth + 1)?;
                } else if metadata.is_file() {
                    self.files.insert(path);
                } else {
                    return Err("data symlink/special file rejected".into());
                }
            }
            let (held, before) = &self.dirs[path];
            if identity(&held.metadata()?) != *before
                || identity(&open(path, true)?.metadata()?) != *before
            {
                return Err("data directory changed while listing".into());
            }
            Ok(())
        }
        fn verify(&self) -> Result<()> {
            let after = Self::new(&self.root)?;
            if self.files != after.files || self.dirs.keys().ne(after.dirs.keys()) {
                return Err("data roster changed".into());
            }
            for (path, (file, before)) in &self.dirs {
                if identity(&file.metadata()?) != *before || after.dirs[path].1 != *before {
                    return Err("held data directory changed".into());
                }
            }
            Ok(())
        }
    }
    fn head(root: &Path, expected: &str) -> Result<()> {
        let output = Command::new("git")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--verify", "HEAD"])
            .output()?;
        if !output.status.success() || output.stdout != format!("{expected}\n").as_bytes() {
            return Err("actual source HEAD mismatch".into());
        }
        Ok(())
    }
    fn select(
        rows: &[Value],
        root: &Path,
        name: &str,
        kind: &str,
        test: bool,
        source: &str,
    ) -> Result<(PathBuf, String)> {
        let mut selected = None;
        for row in rows {
            if row["reason"] != "compiler-artifact" || row["target"]["name"] != name {
                continue;
            }
            if row["manifest_path"].as_str()
                != root.join("rust/crates/dev-tools/Cargo.toml").to_str()
                || row["target"]["src_path"].as_str() != root.join(source).to_str()
            {
                return Err(format!("{name}: artifact is from another source root").into());
            }
            if row["target"]["kind"] != json!([kind])
                || row["profile"]["test"].as_bool() != Some(test)
            {
                continue;
            }
            let path = PathBuf::from(
                row["executable"]
                    .as_str()
                    .ok_or("selected artifact has no executable")?,
            );
            let package = row["package_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("missing artifact package ID")?
                .to_owned();
            if selected.replace((path, package)).is_some() {
                return Err(format!("duplicate {name} artifact").into());
            }
        }
        selected.ok_or_else(|| format!("missing exact {name} artifact").into())
    }
    pub(super) fn run() -> Result<()> {
        let mut args = std::env::args_os().skip(1);
        let mut options = BTreeMap::new();
        while let Some(key) = args.next() {
            let value = args.next().ok_or("option needs value")?;
            let key = key.into_string().map_err(|_| "non-UTF8 option")?;
            if ![
                "--root",
                "--expected-source",
                "--proof-dir",
                "--cargo-jsonl",
            ]
            .contains(&key.as_str())
                || options.insert(key, value).is_some()
            {
                return Err("unknown/duplicate option".into());
            }
        }
        if options.len() != 4 {
            return Err(
                "required: --root ROOT --expected-source SHA --proof-dir DIR --cargo-jsonl FILE"
                    .into(),
            );
        }
        let root = PathBuf::from(&options["--root"]);
        let compiled = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .ok_or("compiled root")?;
        let expected = options["--expected-source"]
            .to_str()
            .ok_or("non-UTF8 SHA")?;
        if root != compiled
            || expected.len() != 40
            || !expected
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("compiled root/expected source mismatch".into());
        }
        let source_dir = open(&root, true)?;
        let source_identity = identity(&source_dir.metadata()?);
        let proof_path = PathBuf::from(&options["--proof-dir"]);
        if proof_path.starts_with(&root) {
            return Err("proof must be outside source".into());
        }
        let proof = open(&proof_path, true)?;
        let proof_meta = proof.metadata()?;
        if proof_meta.uid() != source_identity.3 || proof_meta.mode() & 0o7777 != 0o700 {
            return Err("owned 0700 proof required".into());
        }
        let receipts = proof_path.join("receipts");
        let receipt_dir = open(&receipts, true)?;
        let receipt_meta = receipt_dir.metadata()?;
        if receipt_meta.uid() != proof_meta.uid() || receipt_meta.mode() & 0o7777 != 0o700 {
            return Err("owned 0700 receipts required".into());
        }
        head(&root, expected)?;
        let json_path = PathBuf::from(&options["--cargo-jsonl"]);
        if json_path.parent() != Some(proof_path.as_path()) {
            return Err("Cargo JSONL must be held in this proof directory".into());
        }
        let mut raw = Held::new(&json_path, JSON_LIMIT)?;
        let bytes = raw.contents()?;
        // 전체 JSONL을 eager parse한다. 뒤의 불량 record를 무시하고 ELF를 채택하지 않는다.
        let mut rows: Vec<Value> = Vec::new();
        for line in std::str::from_utf8(&bytes)?.lines() {
            if rows.len() == 16384 {
                return Err("Cargo record count bound exceeded".into());
            }
            rows.push(serde_json::from_str(line)?);
        }
        if rows
            .iter()
            .any(|row| !row.is_object() || !row["reason"].is_string())
        {
            return Err("invalid Cargo record".into());
        }
        let finished: Vec<_> = rows
            .iter()
            .filter(|row| row["reason"] == "build-finished")
            .collect();
        if finished.len() != 1 || finished[0]["success"] != true {
            return Err("exact successful build-finished record required".into());
        }
        let (cli_path, package) = select(
            &rows,
            &root,
            "check-changed-files",
            "bin",
            false,
            "rust/crates/dev-tools/src/bin/check_changed_files.rs",
        )?;
        let (test_path, test_package) = select(
            &rows,
            &root,
            "changed_files_caller",
            "test",
            true,
            "rust/crates/dev-tools/tests/changed_files_caller.rs",
        )?;
        let target = PathBuf::from(
            std::env::var_os("CARGO_TARGET_DIR")
                .ok_or("explicit current CARGO_TARGET_DIR required")?,
        );
        let target_dir = open(&target, true)?;
        let target_identity = identity(&target_dir.metadata()?);
        if target_identity.3 != source_identity.3 || target_identity.2 & 0o022 != 0 {
            return Err("owned current target directory required".into());
        }
        if package != test_package
            || !cli_path.is_absolute()
            || !test_path.is_absolute()
            || !cli_path.starts_with(&target)
            || !test_path.starts_with(&target)
        {
            return Err(
                "actual artifacts must share current source package and target root".into(),
            );
        }
        let mut cli = Held::new(&cli_path, ELF_LIMIT)?;
        let mut test_elf = Held::new(&test_path, ELF_LIMIT)?;
        cli.elf()?;
        test_elf.elf()?;
        if (cli.before.0 .0, cli.before.0 .1) == (test_elf.before.0 .0, test_elf.before.0 .1) {
            return Err("CLI/test must be distinct actual ELFs".into());
        }
        let tree = Tree::new(&root.join(DATA_TREE))?;
        let mut paths: BTreeSet<PathBuf> = SOURCE_FILES.iter().map(|p| root.join(p)).collect();
        paths.extend(tree.files.iter().cloned());
        if paths.len() > 256 {
            return Err("source roster bound exceeded".into());
        }
        let mut source_sha256 = BTreeMap::new();
        let mut held = vec![raw, cli, test_elf];
        let mut total = 0;
        for path in paths {
            let file = Held::new(&path, SOURCE_LIMIT)?;
            total += file.before.0 .5;
            if total > 64 * 1024 * 1024 {
                return Err("source read budget exceeded".into());
            }
            source_sha256.insert(
                path.strip_prefix(&root)?
                    .to_str()
                    .ok_or("non-UTF8 source path")?
                    .to_owned(),
                file.before.1.clone(),
            );
            held.push(file);
        }
        let context = json!({
            "schema": "openpilot.changed-files-caller-context.v1", "source_commit": expected,
            "source_root": root, "proof_dir": receipts, "source_sha256": source_sha256,
            "cli": {"path": cli_path, "sha256": held[1].before.1},
            "test_elf": {"path": test_path, "sha256": held[2].before.1},
        });
        let mut context_bytes = serde_json::to_vec_pretty(&context)?;
        context_bytes.push(b'\n');
        if context_bytes.len() > 64 * 1024 {
            return Err("context budget exceeded".into());
        }
        tree.verify()?;
        for file in &mut held {
            file.verify()?;
        }
        head(&root, expected)?;
        let directory_key = |m: &fs::Metadata| (m.dev(), m.ino(), m.mode(), m.uid(), m.gid());
        for (path, fd, before) in [
            (&root, &source_dir, source_identity),
            (&target, &target_dir, target_identity),
            (&proof_path, &proof, identity(&proof_meta)),
            (&receipts, &receipt_dir, identity(&receipt_meta)),
        ] {
            let initial = (before.0, before.1, before.2, before.3, before.4);
            if directory_key(&fd.metadata()?) != initial
                || directory_key(&open(path, true)?.metadata()?) != initial
            {
                return Err("source/target/proof directory identity changed".into());
            }
        }
        let output =
            PathBuf::from(format!("/proc/self/fd/{}", proof.as_raw_fd())).join("context.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(NOFOLLOW_NONBLOCK)
            .open(&output)?;
        if let Err(error) = file
            .write_all(&context_bytes)
            .and_then(|()| file.sync_all())
        {
            let _ = fs::remove_file(&output);
            return Err(error.into());
        }
        eprintln!(
            "current Cargo JSONL SHA256 {} ({} records)",
            held[0].before.1,
            rows.len()
        );
        println!("{:x}", Sha256::digest(&context_bytes));
        Ok(())
    }
}
#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = linux::run() {
        eprintln!("prepare-changed-files-context: {error}");
        std::process::exit(1);
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("prepare-changed-files-context requires Linux");
    std::process::exit(1);
}
