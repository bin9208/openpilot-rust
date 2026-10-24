//! 단일 normal40 역사 원문 수용. 원본 실행/provider의 새 증거를 만들지 않는다.
use super::{bind, hex, numeric, relative, require, Bound, OriginalValues, PINS};
use openpilot_dev_tools::{controlsd_fixture_generate as fixture, seal::Result};
use rustix::fs::{open, Mode, OFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub const REFERENCE_ID: &str = "legacy-normal40-12456242812-v1";
// 부모 importer가 이 index를 복사할 수 있다. raw 기대값/capture를 생성하지 않는다.
pub const REFERENCE_INDEX: &str = r#"{
  "schema": 1,
  "reference_kind": "historical-original-data",
  "reference_id": "legacy-normal40-12456242812-v1",
  "repository": "bin9208/openpilot-rust",
  "artifact_id": 12456242812,
  "run_id": 39844308544,
  "job_id": 120055270738,
  "artifact_name": "rust-controls-evidence",
  "artifact_bytes": 9895310,
  "artifact_sha256": "956326c29facb48c10f3dfeba18507e29327c48919f2cf35840edbd26bfe570f",
  "workflow_head_sha": "adff0392e61608c9a576550be78dd7eefb8b9065",
  "historical_checkout_sha": "6f25f161aae5eb3f887df6d0147172e4f85d935c",
  "historical_job_result": "success",
  "historical_runner": "GitHub Actions 1000027460",
  "historical_runner_image": "ubuntu-24.04",
  "members": [
    {"name":"loop/input.json","bytes":101555623,"sha256":"9441af7c071e49daaebfcbd8153136ef48bf6f248b3a9c995a8224073c3a0a5b"},
    {"name":"loop/source.json","bytes":63477957,"sha256":"448d2519bd0e0b2580d3d6921e3fab13df0ca4a7f7a66e39e72325b2e2f582d3"},
    {"name":"loop/results.json","bytes":114,"sha256":"84fd54e4bba2d6cfd542fcd3d9db47a0cbc035e22661df4408f9f1b0f6827cb9"},
    {"name":"loop/process.log","bytes":1757,"sha256":"fd72ae04d3bd1c0b5a81c57bd6d8bf6388120fe3aa9af3b5b4165706e7f72e61"}
  ],
  "historical_result": {"pass":true,"cases":40,"frames":8655,"numeric_comparisons":705948,"max_absolute_error":0.0},
  "historical_job_log": {"name":"historical-job.log","bytes":157617,"sha256":"6bbf9799511ee423f32bc41ac1fbd9e18579e96d8b4a27c241ef63ef1382e5fe"},
  "historical_original_provider_closure": "not-recorded",
  "historical_original_provider_closure_complete": false,
  "historical_provider_paths_dereferenced": false,
  "original_capture_executed": false
}"#;

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Pin {
    name: String,
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct HistoricalResult {
    pass: bool,
    cases: u64,
    frames: u64,
    numeric_comparisons: u64,
    max_absolute_error: f64,
}
#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Index {
    schema: u32,
    reference_kind: String,
    reference_id: String,
    repository: String,
    artifact_id: u64,
    run_id: u64,
    job_id: u64,
    artifact_name: String,
    artifact_bytes: u64,
    artifact_sha256: String,
    workflow_head_sha: String,
    historical_checkout_sha: String,
    historical_job_result: String,
    historical_runner: String,
    historical_runner_image: String,
    members: Vec<Pin>,
    historical_result: HistoricalResult,
    historical_job_log: Pin,
    historical_original_provider_closure: String,
    historical_original_provider_closure_complete: bool,
    historical_provider_paths_dereferenced: bool,
    original_capture_executed: bool,
}

fn identity(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
// 역사 파일은 hash를 읽기 전 길이도 제한한다. index에는 64 KiB만 허용한다.
struct Held {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    hash: String,
}
impl Held {
    fn open(path: &Path, limit: u64, expected: Option<&str>) -> Result<Self> {
        let mut file = File::from(open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK | OFlags::NOFOLLOW,
            Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        require(
            metadata.is_file() && metadata.len() <= limit,
            "frozen file type/size differs",
        )?;
        let mut hash = Sha256::new();
        let mut read = 0u64;
        let mut buffer = [0; 65536];
        loop {
            let remaining = usize::try_from((limit + 1 - read).min(buffer.len() as u64))?;
            let count = file.read(&mut buffer[..remaining])?;
            if count == 0 {
                break;
            }
            read += count as u64;
            require(read <= limit, "frozen file grew past byte limit")?;
            hash.update(&buffer[..count]);
        }
        let hash = format!("{:x}", hash.finalize());
        require(
            read == metadata.len()
                && identity(&metadata) == identity(&file.metadata()?)
                && identity(&metadata) == identity(&fs::symlink_metadata(path)?),
            "frozen file changed while reading",
        )?;
        require(
            expected.is_none_or(|value| value == hash),
            "frozen file SHA256 differs",
        )?;
        Ok(Self {
            path: path.into(),
            file,
            metadata,
            hash,
        })
    }
    fn verify(&self) -> Result<()> {
        let other = Self::open(&self.path, self.metadata.len(), Some(&self.hash))?;
        require(
            identity(&self.metadata) == identity(&other.metadata)
                && identity(&self.metadata) == identity(&self.file.metadata()?),
            "frozen held file identity changed",
        )
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(self.metadata.len() + 1).read_to_end(&mut bytes)?;
        require(
            bytes.len() as u64 == self.metadata.len(),
            "frozen file length changed",
        )?;
        self.verify()?;
        Ok(bytes)
    }
}

type Entries = BTreeMap<OsString, (bool, u64, u64)>;
struct Roster {
    path: PathBuf,
    file: File,
    inode: (u64, u64),
    entries: Entries,
}
impl Roster {
    fn entries(path: &Path) -> Result<Entries> {
        let mut entries = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let m = fs::symlink_metadata(entry.path())?;
            require(
                m.is_file() || m.is_dir(),
                "frozen/numerics symlink or special entry",
            )?;
            require(
                seen.insert((m.dev(), m.ino())),
                "frozen/numerics inode alias",
            )?;
            entries.insert(entry.file_name(), (m.is_dir(), m.dev(), m.ino()));
        }
        Ok(entries)
    }
    fn open(path: &Path, names: &[(&str, bool)]) -> Result<Self> {
        require(
            path.is_absolute() && path.canonicalize()? == path,
            "canonical frozen/numerics directory required",
        )?;
        let file = File::from(open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
            Mode::empty(),
        )?);
        let m = file.metadata()?;
        let entries = Self::entries(path)?;
        let actual: BTreeMap<_, _> = entries
            .iter()
            .map(|(name, (dir, _, _))| (name.clone(), *dir))
            .collect();
        let expected: BTreeMap<_, _> = names
            .iter()
            .map(|(name, dir)| (OsString::from(*name), *dir))
            .collect();
        require(
            actual == expected,
            "frozen/numerics directory roster differs",
        )?;
        let roster = Self {
            path: path.into(),
            file,
            inode: (m.dev(), m.ino()),
            entries,
        };
        roster.verify()?;
        Ok(roster)
    }
    fn verify(&self) -> Result<()> {
        let m = fs::symlink_metadata(&self.path)?;
        let held = self.file.metadata()?;
        require(
            m.is_dir()
                && self.inode == (m.dev(), m.ino())
                && self.inode == (held.dev(), held.ino())
                && self.entries == Self::entries(&self.path)?,
            "frozen/numerics directory identity/roster changed",
        )
    }
}

pub(super) struct Admission {
    index: Index,
    files: Vec<Held>,
    rosters: Vec<Roster>,
}
impl Admission {
    pub fn verify(&self) -> Result<()> {
        for roster in &self.rosters {
            roster.verify()?;
        }
        for file in &self.files {
            file.verify()?;
        }
        Ok(())
    }
    pub fn check_frames(&self, cases: &[Value]) -> Result<()> {
        let frames = cases.iter().try_fold(0u64, |count, case| -> Result<u64> {
            Ok(count
                + case["rows"]
                    .as_array()
                    .ok_or("historical rows missing")?
                    .len() as u64)
        })?;
        require(
            cases.len() as u64 == self.index.historical_result.cases
                && frames == self.index.historical_result.frames,
            "frozen normal40 case/frame count differs",
        )
    }
    pub fn check_result(&self, result: &Value) -> Result<()> {
        require(
            result["pass"] == true
                && result["cases"] == 40
                && result["frames"] == 8655
                && result["numeric_comparisons"] == 705948
                && result["max_absolute_error"]
                    .as_f64()
                    .is_some_and(|x| x.is_finite() && x >= 0.0),
            "fresh frozen-reference comparison counts differ",
        )
    }
    pub fn receipt(&self) -> Value {
        json!({
            "reference_kind":self.index.reference_kind,"reference_id":self.index.reference_id,
            "historical_reference":self.index,"original_source_commit":self.index.historical_checkout_sha,
            "historical_original_provider_closure":"not-recorded",
            "historical_original_provider_closure_complete":false,
            "historical_provider_paths_dereferenced":false,"original_capture_executed":false,
            "full_runtime_acceptance":false,"inventory_credit":false,
            "current_full_source_sdk_exec_provider_seals":"required-parent-gate",
            "historical_bindings":self.files.iter().map(|file|json!({"path":file.path,"bytes":file.metadata.len(),"sha256":file.hash})).collect::<Vec<_>>()
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Numerics {
    format: u32,
    numpy: String,
    abi: String,
    library: String,
    files: Vec<NumericalFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericalFile {
    name: String,
    sha256: String,
}

pub(super) fn admit(
    root: &Path,
    numerics: &Path,
    reference: &Path,
    bounds: &mut Vec<Bound>,
) -> Result<(Admission, OriginalValues)> {
    let root_roster = Roster::open(
        reference,
        &[
            ("artifact.zip", false),
            ("historical-job.log", false),
            ("frozen-reference-admission.json", false),
            ("loop", true),
        ],
    )?;
    let loop_roster = Roster::open(
        &reference.join("loop"),
        &[
            ("input.json", false),
            ("source.json", false),
            ("results.json", false),
            ("process.log", false),
        ],
    )?;
    let index_file = Held::open(
        &reference.join("frozen-reference-admission.json"),
        64 * 1024,
        None,
    )?;
    let index: Index = serde_json::from_slice(&index_file.bytes()?)?;
    let recipe: Index = serde_json::from_str(REFERENCE_INDEX)?;
    require(index == recipe, "fixed frozen reference recipe differs")?;
    let mut files = vec![index_file];
    files.push(Held::open(
        &reference.join("artifact.zip"),
        index.artifact_bytes,
        Some(&index.artifact_sha256),
    )?);
    require(
        files[1].metadata.len() == index.artifact_bytes,
        "historical archive byte count differs",
    )?;
    let log = &index.historical_job_log;
    files.push(Held::open(
        &reference.join(&log.name),
        log.bytes,
        Some(&log.sha256),
    )?);
    require(
        files[2].metadata.len() == log.bytes,
        "historical job log byte count differs",
    )?;
    let mut values = BTreeMap::new();
    for pin in &index.members {
        let held = Held::open(&reference.join(&pin.name), pin.bytes, Some(&pin.sha256))?;
        require(
            held.metadata.len() == pin.bytes,
            "historical member byte count differs",
        )?;
        match pin.name.as_str() {
            "loop/input.json" | "loop/source.json" => {
                let raw = held.bytes()?;
                let key = if pin.name == "loop/input.json" {
                    "input.json"
                } else {
                    "source.json"
                };
                values.insert(key, (numeric::parse_python(&raw)?, raw));
            }
            "loop/results.json" => require(
                serde_json::from_slice::<HistoricalResult>(&held.bytes()?)?
                    == index.historical_result,
                "historical result differs",
            )?,
            "loop/process.log" => {}
            _ => return Err("unknown frozen member".into()),
        }
        files.push(held);
    }
    // 현재 source의 보수적 closure + 실제 controller 의존. 전체 source/SDK 봉인을 대신하지 않는다.
    let mut sources = super::source_paths(root)?;
    for name in [
        "Cargo.toml",
        "src/lib.rs",
        "src/qa_process.rs",
        "src/seal.rs",
        "src/argparse_scan.rs",
        "src/camera_random.rs",
        "src/controlsd_check.rs",
        "src/bin/check_controlsd_native.rs",
        "src/controlsd_check/frozen.rs",
        "src/controlsd_check/reference.rs",
        "src/paramsd_estimator_check/identity.rs",
    ] {
        sources.insert(Path::new("rust/crates/dev-tools").join(name));
    }
    require(
        sources.len() < 16384,
        "current controller source binding limit exceeded",
    )?;
    for path in sources {
        bounds.push(Bound::open(&root.join(path), None)?);
    }
    for (name, pin) in PINS.into_iter().chain(fixture::SOURCES) {
        require(
            bounds
                .iter()
                .any(|bound| bound.path == root.join(name) && bound.hash == pin),
            "original caller/fixture attribution pin differs",
        )?;
    }
    let manifest = Bound::open(&numerics.join("manifest.json"), None)?;
    let staged: Numerics = serde_json::from_slice(&manifest.bytes(1024 * 1024)?)?;
    require(
        staged.format == 1 && staged.numpy == "2.5.3" && staged.abi == "scipy_dgesdd_64_",
        "exact CONTROLS_NUMERICS contract required",
    )?;
    let mut names = BTreeSet::from(["manifest.json".to_owned()]);
    for file in &staged.files {
        require(
            relative(Path::new(&file.name))
                && Path::new(&file.name).components().count() == 1
                && hex(&file.sha256, 64)
                && names.insert(file.name.clone()),
            "numerics duplicate/invalid name or digest",
        )?;
        bind(bounds, &numerics.join(&file.name), &file.sha256)?;
    }
    require(
        staged.files.iter().any(|file| file.name == staged.library),
        "numerical library not bound",
    )?;
    let names: Vec<_> = names.iter().map(|name| (name.as_str(), false)).collect();
    let numerics_roster = Roster::open(numerics, &names)?;
    bounds.push(manifest);
    let admission = Admission {
        index,
        files,
        rosters: vec![root_roster, loop_roster, numerics_roster],
    };
    admission.verify()?;
    Ok((admission, values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_held_files_and_rosters_reject_mutation_aliases_and_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let path = root.join("raw");
        fs::write(&path, b"held").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"held"));
        let held = Held::open(&path, 4, Some(&hash)).unwrap();
        let roster = Roster::open(&root, &[("raw", false)]).unwrap();
        fs::write(&path, b"edit").unwrap();
        assert!(held.verify().is_err());
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"held").unwrap();
        assert!(held.verify().is_err());
        assert!(roster.verify().is_err());
        assert!(Held::open(&path, 3, Some(&hash)).is_err());
        fs::hard_link(&path, root.join("alias")).unwrap();
        assert!(Roster::open(&root, &[("raw", false), ("alias", false)]).is_err());
        fs::remove_file(root.join("alias")).unwrap();
        let roster = Roster::open(&root, &[("raw", false)]).unwrap();
        fs::write(root.join("extra"), b"unexpected").unwrap();
        assert!(roster.verify().is_err());
        fs::remove_file(root.join("extra")).unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("missing", &path).unwrap();
        assert!(held.verify().is_err());
        assert!(Roster::open(&root, &[("raw", false)]).is_err());
    }
}
