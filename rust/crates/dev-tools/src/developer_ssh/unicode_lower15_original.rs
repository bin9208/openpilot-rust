//! 독립 CPython3.12 원본 lower와 scalar/문맥 경계를 Actions에서 대조한다.
use super::lower15;
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const ORIGINAL: &str = r#"
import hashlib, json, struct, sys, unicodedata
assert sys.version_info[:3] == (3,12,15), sys.version
assert unicodedata.unidata_version == '15.0.0'
contexts=json.loads(sys.argv[1])
digest=hashlib.sha256()
count=0
for point in range(0x110000):
    if 0xD800 <= point <= 0xDFFF: continue
    raw=chr(point).lower().encode('utf-8')
    digest.update(struct.pack('<I',len(raw)))
    digest.update(raw)
    count+=1
print(json.dumps({'python':sys.version,'unicode':unicodedata.unidata_version,
                  'scalars':count,'lower_length_prefixed_utf8_sha256':digest.hexdigest(),
                  'contexts':[s.lower() for s in contexts]}))
"#;

type SealMap = BTreeMap<&'static str, (String, Vec<i128>)>;
struct SourceFiles {
    held: Vec<(&'static str, File)>,
    before: SealMap,
}
fn identity(m: &fs::Metadata) -> Vec<i128> {
    vec![
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
}
fn seal(file: &mut File, path: &Path, limit: u64) -> (String, Vec<i128>) {
    let before = file.metadata().unwrap();
    assert!(before.is_file() && before.len() <= limit);
    assert_eq!(
        identity(&before),
        identity(&fs::symlink_metadata(path).unwrap())
    );
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes.len() as u64, before.len());
    assert_eq!(identity(&before), identity(&file.metadata().unwrap()));
    assert_eq!(
        identity(&before),
        identity(&fs::symlink_metadata(path).unwrap())
    );
    (format!("{:x}", Sha256::digest(bytes)), identity(&before))
}
impl SourceFiles {
    fn open(root: &Path) -> Self {
        let mut held = Vec::new();
        let mut before = BTreeMap::new();
        for name in [
            "rust/crates/dev-tools/src/developer_ssh.rs",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15.rs",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15.provenance.json",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15.LICENSE-UNICODE-V3.txt",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15_original.rs",
        ] {
            let path = root.join(name);
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(
                    (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
                )
                .open(&path)
                .unwrap();
            before.insert(name, seal(&mut file, &path, 1024 * 1024));
            held.push((name, file));
        }
        Self { held, before }
    }
    fn verify_after(&mut self, root: &Path) {
        for (name, file) in &mut self.held {
            assert_eq!(
                self.before[*name],
                seal(file, &root.join(*name), 1024 * 1024),
                "held native mapping source changed: {name}"
            );
        }
    }
}

#[test]
fn all_scalar_mappings_and_contexts_match_cpython_312() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let mut sources = SourceFiles::open(&root);
    let executable_path = std::env::current_exe().unwrap();
    assert!(executable_path.is_absolute());
    let mut executable = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(&executable_path)
        .unwrap();
    let binary_before = seal(&mut executable, &executable_path, 128 * 1024 * 1024);
    let prefixes = [
        "",
        "A",
        "a",
        "\u{295}",
        "\u{345}",
        "A\u{345}",
        "A\u{1171e}",
        "\u{a7cb}",
        "\u{1c89}",
        " \t",
        "A ",
        "A\u{200d}",
        "\u{130}",
        "1",
    ];
    let suffixes = [
        "",
        "A",
        "a",
        "\u{295}",
        "\u{345}",
        "\u{1171e}A",
        "\u{a7cb}",
        "\u{345}A",
        "\u{200d}A",
        "1",
        " ",
    ];
    let contexts = prefixes
        .iter()
        .flat_map(|prefix| {
            suffixes
                .iter()
                .map(move |suffix| format!("{prefix}\u{3a3}{suffix}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(contexts.len(), 154);
    let python = PathBuf::from(
        std::env::var_os("DEVELOPER_CLI_ORACLE_PYTHON")
            .or_else(|| std::env::var_os("AUTH_CLI_ORACLE_PYTHON"))
            .expect("prepared absolute CPython3.12.15 required"),
    );
    assert!(python.is_absolute());
    let library = std::env::var_os("OPENPILOT_ORACLE_PYTHON_LIBRARY_PATH")
        .expect("explicit prepared Python runtime library directory required");
    let temp = tempfile::tempdir().unwrap();
    let mut command = CommandSpec::new_os(
        vec![
            python.into_os_string(),
            "-I".into(),
            "-B".into(),
            "-S".into(),
            "-X".into(),
            format!("pycache_prefix={}/unused-pycache", temp.path().display()).into(),
            "-c".into(),
            ORIGINAL.into(),
            serde_json::to_string(&contexts).unwrap().into(),
        ],
        Duration::from_secs(20),
    );
    command.cwd = Some(temp.path().to_owned());
    command.set_os_env(BTreeMap::from([
        ("LD_LIBRARY_PATH".into(), library),
        ("LC_ALL".into(), "C.UTF-8".into()),
    ]));
    command.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| key != "LD_LIBRARY_PATH" && key != "LC_ALL")
        .collect();
    let output = NativeRunner.run(&command).unwrap();
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let original: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(original["unicode"], "15.0.0");
    let mut digest = Sha256::new();
    let mut count = 0_u32;
    for point in 0..=0x10ffff {
        let Some(value) = char::from_u32(point) else {
            continue;
        };
        let lowered = lower15(&value.to_string());
        digest.update(u32::try_from(lowered.len()).unwrap().to_le_bytes());
        digest.update(lowered.as_bytes());
        count += 1;
    }
    assert_eq!(count, 1_112_064);
    assert_eq!(json!(count), original["scalars"]);
    assert_eq!(
        json!(format!("{:x}", digest.finalize())),
        original["lower_length_prefixed_utf8_sha256"]
    );
    let actual = contexts
        .iter()
        .map(|text| lower15(text))
        .collect::<Vec<_>>();
    assert_eq!(json!(actual), original["contexts"]);
    sources.verify_after(&root);
    assert_eq!(
        binary_before,
        seal(&mut executable, &executable_path, 128 * 1024 * 1024)
    );
    println!(
        "SSH_LOWER15_ORIGINAL_JSON {}",
        json!({"schema":"openpilot.ssh-lower15.original.v1", "expected_original":original,
               "contexts":contexts,"native_source_seals":sources.before,"native_elf":{"path":executable_path,"seal":binary_before},"source_unchanged":true})
    );
}
