//! Replay original uploader observations without computing expectations from the daemon.
//! Python helpers retain live sibling consumers.
mod fixture;
pub(crate) mod http;
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    time::Duration,
};

const MAX_CORPUS: u64 = 64 * 1024 * 1024;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    version: u32,
    source_pins: BTreeMap<String, String>,
    cases: Vec<Case>,
    backoff: Backoff,
    http_cases: Vec<http::Observation>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    files: Vec<(String, u64, bool)>,
    config: Value,
    expected: Value,
    post_xattrs_hex: BTreeMap<String, Option<String>>,
    http: Option<Http>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Http {
    key_name: String,
    fake: bool,
    timeout_ms: u64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Backoff {
    inputs: Vec<Value>,
    expected: Vec<Value>,
}
#[derive(Debug)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub fixture: PathBuf,
    pub fixture_sha256: String,
    pub output: PathBuf,
}
fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 <= limit,
        "uploader observation exceeds size bound",
    )?;
    Ok(bytes)
}
fn relative(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    require(
        !value.is_empty() && path.components().all(|p| matches!(p, Component::Normal(_))),
        "unsafe uploader fixture path",
    )?;
    Ok(path)
}
pub fn load(path: &Path, pin: &str) -> Result<Corpus> {
    let bytes = bounded(path, MAX_CORPUS)?;
    require(
        pin.len() == 64 && hash(&bytes) == pin,
        "uploader original observation SHA mismatch",
    )?;
    let corpus: Corpus = serde_json::from_slice(&bytes)?;
    validate(&corpus)?;
    Ok(corpus)
}
fn validate(corpus: &Corpus) -> Result<()> {
    require(
        corpus.version == 1
            && corpus.cases.len() == 244
            && corpus.http_cases.len() == 32
            && corpus.backoff.inputs.len() == 10_000
            && corpus.backoff.expected.len() == 10_000,
        "uploader original observation roster changed",
    )?;
    let ids = corpus
        .cases
        .iter()
        .map(|c| c.id.clone())
        .collect::<BTreeSet<_>>();
    let expected = (0..212)
        .map(|i| format!("policy-{i}"))
        .chain((0..32).map(|i| format!("http-{i}")))
        .collect::<BTreeSet<_>>();
    require(ids == expected, "uploader case IDs missing or duplicated")?;
    require(
        corpus.source_pins.len() == 9
            && [
                "rust/tools/check_uploader.py",
                "rust/tools/check_uploader_logging.py",
                "rust/tools/logmessaged_native.py",
                "rust/tools/logmessaged_reference.py",
                "openpilot/system/loggerd/uploader.py",
                "openpilot/system/loggerd/xattr_cache.py",
                "openpilot/common/api.py",
                "openpilot/common/utils.py",
                "openpilot/common/logging_extra.py",
            ]
            .iter()
            .all(|name| corpus.source_pins.contains_key(*name)),
        "uploader source dependency roster differs",
    )?;
    let http_ids = corpus
        .http_cases
        .iter()
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    require(
        http_ids == (0..32).map(|i| format!("http-{i}")).collect(),
        "uploader HTTP observations missing or duplicated",
    )?;
    for case in &corpus.cases {
        require(
            !case.files.is_empty()
                && case.files.len() <= 49
                && case.config.is_object()
                && case.expected.is_object(),
            "invalid uploader case shape",
        )?;
        let names = case
            .files
            .iter()
            .map(|(name, _, _)| name)
            .collect::<BTreeSet<_>>();
        require(
            names.len() == case.files.len() && names.len() == case.post_xattrs_hex.len(),
            "uploader xattr fixture roster differs",
        )?;
        for (name, size, _) in &case.files {
            relative(name)?;
            require(
                *size <= 25_000_001 && case.post_xattrs_hex.contains_key(name),
                "uploader file observation outside source bounds",
            )?;
        }
        require(
            case.http.is_some() == case.id.starts_with("http-"),
            "uploader HTTP fixture kind differs",
        )?;
    }
    Ok(())
}
fn make_files(root: &Path, case: &Case, files: &[(String, u64, bool)]) -> Result<()> {
    for (name, size, uploaded) in files {
        let path = root.join(relative(name)?);
        fs::create_dir_all(path.parent().ok_or("fixture parent missing")?)?;
        let mut file = File::create(&path)?;
        if *size > 100_000 {
            file.set_len(*size)?;
        } else {
            file.write_all(
                &(0..*size)
                    .map(|i| ((i * 23 + 17) % 256) as u8)
                    .collect::<Vec<_>>(),
            )?;
        }
        if *uploaded {
            rustix::fs::setxattr(&path, "user.upload", b"1", rustix::fs::XattrFlags::empty())?;
        }
        if case
            .config
            .get("readonly")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
        }
    }
    Ok(())
}
fn check_xattrs(root: &Path, case: &Case) -> Result<()> {
    for (name, expected) in &case.post_xattrs_hex {
        let mut bytes = [0_u8; 1024];
        let actual =
            rustix::fs::getxattr(root.join(relative(name)?), "user.upload", &mut bytes[..])
                .ok()
                .map(|n| {
                    bytes[..n]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                });
        require(
            &actual == expected,
            &format!("{} post-upload xattr differs: {name}", case.id),
        )?;
    }
    Ok(())
}
fn trace(
    binary: &Path,
    input: &Value,
    output: &Path,
    id: &str,
    runner: &mut dyn Runner,
) -> Result<Value> {
    let mut command =
        CommandSpec::new_os(vec![binary.as_os_str().to_owned()], Duration::from_secs(30));
    let mut bytes = serde_json::to_vec(input)?;
    bytes.push(b'\n');
    command.input = Some(bytes);
    command.stdout_limit = 8 << 20;
    command.stderr_limit = 2 << 20;
    let captured = runner.run(&command)?;
    fs::write(output.join(format!("{id}.stdout")), &captured.stdout)?;
    fs::write(output.join(format!("{id}.stderr")), &captured.stderr)?;
    require(
        captured.code == 0,
        &format!("uploader trace failed for {id}"),
    )?;
    Ok(serde_json::from_slice(&captured.stdout)?)
}
pub fn run(options: &Options) -> Result<Value> {
    let corpus = load(&options.fixture, &options.fixture_sha256)?;
    for (name, expected) in &corpus.source_pins {
        relative(name)?;
        crate::uploader_source_admission::validate(&options.root, name, expected)?;
    }
    fs::create_dir_all(&options.output)?;
    let before = hash(&bounded(&options.binary, 256 << 20)?);
    let keys = http::Keys::new()?;
    let mut runner = NativeRunner;
    let mut rows = Vec::new();
    let mut unsorted_priority_fixtures = 0;
    for case in &corpus.cases {
        let (temp, layout, unsorted) = fixture::make(case)?;
        unsorted_priority_fixtures += usize::from(unsorted);
        fs::write(
            options.output.join(format!("{}.fixture.json", case.id)),
            serde_json::to_vec_pretty(&layout)?,
        )?;
        let mut input = case.config.clone();
        input["root"] = json!(temp.path());
        let server = if let Some(config) = &case.http {
            let observation = corpus
                .http_cases
                .iter()
                .find(|r| r.id == case.id)
                .ok_or("HTTP original row missing")?;
            require(
                observation.key_name == config.key_name,
                "HTTP signer fixture mismatch",
            )?;
            let server = http::Server::start(&observation.settings)?;
            let persist = tempfile::tempdir()?;
            keys.write(persist.path(), &config.key_name)?;
            input["http"] = json!({"api_host":server.url,"persist":persist.path(),"version":options.root.join("openpilot/common/version.h"),"fake":config.fake,"timeout_ms":config.timeout_ms});
            Some((server, observation, persist))
        } else {
            None
        };
        let actual = trace(
            &options.binary,
            &input,
            &options.output,
            &case.id,
            &mut runner,
        )?;
        if let Some((server, observation, _persist)) = server {
            let requests = server.finish()?;
            fs::write(
                options.output.join(format!("{}.http.json", case.id)),
                serde_json::to_vec_pretty(&requests)?,
            )?;
            http::verify_observation(observation, &requests, &keys)?;
        }
        require(
            actual == case.expected,
            &format!(
                "{} result differs from original captured decisions",
                case.id
            ),
        )?;
        check_xattrs(temp.path(), case)?;
        rows.push(json!({"case":case.id,"original_result_equal":true,"post_xattrs_equal":true}));
    }
    let temp = tempfile::tempdir()?;
    let backoff = trace(
        &options.binary,
        &json!({"root":temp.path(),"backoff":corpus.backoff.inputs}),
        &options.output,
        "backoff",
        &mut runner,
    )?;
    require(
        backoff == json!(corpus.backoff.expected),
        "backoff differs from 10000 original decisions",
    )?;
    require(
        hash(&bounded(&options.binary, 256 << 20)?) == before,
        "uploader binary changed during replay",
    )?;
    require(
        unsorted_priority_fixtures > 0,
        "fixture layout made priority sorting redundant",
    )?;
    let result = json!({"passed":true,"filesystem_scenarios":212,"backoff_decisions":10000,"loopback_http_scenarios":32,"jwt_algorithms":["RS256","ES256"],"compressed_payloads":"byte-exact original captured/native","binary_sha256":before,"fixture_sha256":options.fixture_sha256,"scenarios":rows,"expected_values_origin":"sealed actual original observations; no Rust uploader decisions used as expectations","fixture_enumeration":"controlled tmpfs; captured same-priority ties, original input slots across priorities; verified before daemon execution","unsorted_priority_fixtures":unsorted_priority_fixtures,"source_files_retired":false});
    fs::write(
        options.output.join("report.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}

/// Fixture admission is explicit while the source capture awaits review. The
/// parent may pin the accepted fixture in-tree when registering this controller.
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan, OptionSpec};
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = scan(
        &args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--root", false),
            OptionSpec::value("--fixture", true),
            OptionSpec::value("--fixture-sha256", true),
        ],
    )?;
    if parsed.help {
        println!(
            "Usage: check-uploader-reference --binary PATH --output DIR --fixture JSON --fixture-sha256 SHA256 [--root ROOT]"
        );
        return Ok(());
    }
    let get = |name: &str| -> Result<String> {
        parsed
            .values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("{name} required").into())
    };
    let root = parsed
        .values
        .get("--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .expect("workspace layout")
                .to_path_buf()
        });
    let result = run(&Options {
        root,
        binary: PathBuf::from(get("--binary")?),
        output: PathBuf::from(get("--output")?),
        fixture: PathBuf::from(get("--fixture")?),
        fixture_sha256: get("--fixture-sha256")?,
    })?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
