//! Native updater comparison against sealed, independently captured original results.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    updated_fixture,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

const ROLLBACK: &[u8] = b"complete previous source/package pair\n";
const LOCK: &[u8] = b"owned shared transaction lock\n";
const SOURCE_SHA: &str = "bb52c5227c08d70cace263c84778d9a8d87cf92ea6e30f6770d57788da1ebc80";
const FIXTURE_SHA: &str = "76607e02520f383227264257de0ed8505b9dc12e462227b176f2c919dfec2d60";
const CHECKER_SHA: &str = "bfa1686e464e5457b9a9bdc66b954e81f5994db24860d7f7b2869999f4e29b7d";
const MAX_HISTORICAL_SOURCE_BYTES: u64 = 2 * 1024 * 1024;
const ORIGINAL_INPUTS: [(&str, &str); 5] = [
    (
        "rust/tools/original_params_binding.py",
        "b5fa10da96aecca63bafef0c03f6ddf9fe5fde89683f10e70742b4e919c38457",
    ),
    (
        "openpilot/system/updated/updated.py",
        "59254235f9d5ee6dcfd5ca2ef451d7a851a6d531664e014504e067d029cc8038",
    ),
    (
        "openpilot/system/updated/process.py",
        "c6f7e468e448686a53309fdbbce48a9111ed156ecfbd8f1e212cf4d304e9c63f",
    ),
    (
        "openpilot/common/markdown.py",
        "2ab0dbbf917fd84ebffab2b672aa12f9b98cca45c5a491e0db389f008c3c3592",
    ),
    (
        "openpilot/selfdrive/selfdrived/alerts_offroad.json",
        "51fe3cc2071ebbcfc897aa1891a3aaeba4135432c1959792759506dfccc86501",
    ),
];

fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn bound(path: &Path) -> Result<(PathBuf, String)> {
    let path = fs::canonicalize(path)?;
    require(
        fs::metadata(&path)?.is_file(),
        "updated checker input is not a regular file",
    )?;
    let digest = hash(&path)?;
    Ok((path, digest))
}
fn unchanged(path: &Path, digest: &str) -> Result<()> {
    require(
        fs::symlink_metadata(path)?.is_file() && hash(path)? == digest,
        "updated checker source/input changed during run",
    )
}

fn historical_sources(root: &Path) -> Result<Value> {
    let mut admitted = serde_json::Map::new();
    for (name, digest, required) in [
        ("rust/tools/check_updated.py", CHECKER_SHA, false),
        ("rust/tools/updated_source.py", SOURCE_SHA, false),
        ("rust/tools/updated_fixtures.py", FIXTURE_SHA, true),
    ] {
        let path = root.join(name);
        require(
            fs::symlink_metadata(path.parent().ok_or("missing updater source parent")?)?.is_dir(),
            "updater source parent is not a directory",
        )?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                require(
                    metadata.is_file(),
                    "updater historical source is not regular",
                )?;
                let mut held = File::from(rustix::fs::open(
                    &path,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::NONBLOCK
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )?);
                let actual = held.metadata()?;
                require(
                    actual.is_file()
                        && (metadata.dev(), metadata.ino()) == (actual.dev(), actual.ino()),
                    "updater historical source identity changed",
                )?;
                require(
                    actual.len() <= MAX_HISTORICAL_SOURCE_BYTES,
                    "updater historical source exceeds bounded admission",
                )?;
                let mut bytes = Vec::new();
                (&mut held)
                    .take(MAX_HISTORICAL_SOURCE_BYTES + 1)
                    .read_to_end(&mut bytes)?;
                require(
                    bytes.len() as u64 <= MAX_HISTORICAL_SOURCE_BYTES,
                    "updater historical source grew beyond bounded admission",
                )?;
                require(
                    format!("{:x}", Sha256::digest(bytes)) == digest,
                    "updater historical source changed",
                )?;
                admitted.insert(name.into(), json!(digest));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !required => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(Value::Object(admitted))
}

fn sealed_original(name: &str) -> Result<Value> {
    let (bytes, digest): (&[u8], &str) = match name {
        "check-fetch-metered" => (include_bytes!("../tests/fixtures/updated-original-df0128aa/check-fetch-metered.json"), "051f172d2934a3f245736f48df1139f8cb108197e4b430cdf8cbc13ce251437d"),
        "missing-branch-failure" => (include_bytes!("../tests/fixtures/updated-original-df0128aa/missing-branch-failure.json"), "2ac87b6b182010fc835b22f1a00a4114fd512ee9c05ca3cd2aa147d350b2ee42"),
        "tizi-branch-map-agnos-same-version" => (include_bytes!("../tests/fixtures/updated-original-df0128aa/tizi-branch-map-agnos-same-version.json"), "59532593affbe4e2ffae3449164c78b36a51e4ab2273bead306f7262d0a1e85f"),
        "agnos-model-manifest-boundary" => (include_bytes!("../tests/fixtures/updated-original-df0128aa/agnos-model-manifest-boundary.json"), "216cbde9a519bafab097714e7a4b8efd63f0a73a4a28ef3525508e832bfb71ce"),
        "connectivity-thresholds" => (include_bytes!("../tests/fixtures/updated-original-df0128aa/connectivity-thresholds.json"), "ced55dbb2f37b9f381925466fcd0b1b169806002e81c7d43bca30a3ed5a82c40"),
        _ => return Err("unknown sealed updater scenario".into()),
    };
    require(
        format!("{:x}", Sha256::digest(bytes)) == digest,
        "sealed original updater observation changed",
    )?;
    Ok(serde_json::from_slice(bytes)?)
}

pub fn cases() -> Vec<(&'static str, Value, Value)> {
    vec![
        (
            "check-fetch-metered",
            json!({}),
            json!([{}, {"request":"check"}, {"request":"fetch"}, {"request":"none","params":{"NetworkMetered":"1"}}, {"request":"none","now":1791072000.001}]),
        ),
        (
            "missing-branch-failure",
            json!({}),
            json!([{}, {"request":"fetch","params":{"UpdaterTargetBranch":"missing"}}, {"params":{"UpdaterTargetBranch":"dev","NetworkMetered":"1","UpdaterLastFetchTime":"2026-10-01T00:00:00+00:00"}}]),
        ),
        (
            "tizi-branch-map-agnos-same-version",
            json!({"device":"tizi","agnos":true}),
            json!([{}, {"request":"fetch","params":{"UpdaterTargetBranch":"release3"}}]),
        ),
        (
            "agnos-model-manifest-boundary",
            json!({"device":"tici","agnos":true,"os_version":"older-os"}),
            json!([{}, {"request":"fetch"}]),
        ),
        (
            "connectivity-thresholds",
            json!({}),
            json!([{}, {"report_failure":1,"params":{"UptimeOnroad":(2300*3600).to_string(),"RouteCount":"8001"}}, {"report_failure":1,"params":{"UptimeOnroad":(2301*3600).to_string(),"RouteCount":"8001"}}, {"report_failure":1,"params":{"UptimeOnroad":(2699*3600).to_string(),"RouteCount":"8001"}}, {"report_failure":1,"params":{"UptimeOnroad":(2701*3600).to_string(),"RouteCount":"8401"}}, {"report_failure":1,"params":{"RouteCount":"9000.5"}}, {"report_failure":16,"has_internet":true}]),
        ),
    ]
}

pub fn assert_equivalent(source: &Value, native: &Value) -> Result<usize> {
    let commands = source["commands"]
        .as_array()
        .ok_or("source commands missing")?;
    let obsolete = json!({"argv":["sudo","rm","-rf","<ROOT>/data/stage"],"cwd":null});
    let count = commands.iter().filter(|item| **item == obsolete).count();
    require(
        count > 0,
        "source fixture did not exercise staging initialization",
    )?;
    let mut expected = source.clone();
    expected["commands"] = Value::Array(
        commands
            .iter()
            .filter(|item| *item != &obsolete)
            .cloned()
            .collect(),
    );
    // Python compares 5400.0 and 5400 as equal. The native snapshot writes
    // integral wait seconds as JSON integers; accept only that exact numeric
    // equivalence at the wait field, keeping every other field strict.
    if let (Some(source_rows), Some(native_rows)) = (
        source["snapshots"].as_array(),
        native["snapshots"].as_array(),
    ) {
        for (index, (source_row, native_row)) in source_rows.iter().zip(native_rows).enumerate() {
            let source_wait = &source_row["wait"];
            let native_wait = &native_row["wait"];
            if let (Some(source_number), Some(source_float), Some(native_int)) = (
                source_wait.as_number(),
                source_wait.as_f64(),
                native_wait.as_u64(),
            ) {
                if source_number.is_f64()
                    && native_int <= (1_u64 << 53)
                    && source_float == native_int as f64
                {
                    expected["snapshots"][index]["wait"] = native_wait.clone();
                }
            }
        }
    }
    require(
        native == &expected,
        "source/native mismatch outside preserved staging-root lifecycle",
    )?;
    Ok(count)
}

fn output_value(value: &Value, path: &Path) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn git_head(base: &Path) -> Result<String> {
    let mut spec = CommandSpec::new(
        vec!["/usr/bin/git".into(), "rev-parse".into(), "HEAD".into()],
        Duration::from_secs(60),
    );
    spec.cwd = Some(base.into());
    spec.env.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
    spec.env
        .insert("GIT_CONFIG_GLOBAL".into(), "/dev/null".into());
    let result = NativeRunner.run(&spec)?;
    require(result.code == 0, "fixture Git rev-parse failed")?;
    Ok(String::from_utf8(result.stdout)?.trim().into())
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| format!("missing updater {key}").into())
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    field(value, key)?
        .as_array()
        .ok_or_else(|| format!("invalid updater {key}").into())
}
fn snapshots(value: &Value) -> Result<&Vec<Value>> {
    array(value, "snapshots")
}
fn verify_case(name: &str, native: &Value) -> Result<()> {
    let s = snapshots(native)?;
    let last = s.last().ok_or("empty updater snapshots")?;
    let params = |i: usize| -> Result<&Value> {
        field(s.get(i).ok_or("missing updater snapshot")?, "params")
    };
    match name {
        "check-fetch-metered" => {
            require(
                s.get(2).and_then(|v| v["consistent"].as_bool()) == Some(true)
                    && s.get(4).and_then(|v| v["consistent"].as_bool()) == Some(true),
                "updater consistency lost",
            )?;
            require(
                params(2)?["UpdateAvailable"] == "31"
                    && params(3)?["UpdaterFetchAvailable"] == "31",
                "updater availability mismatch",
            )?;
        }
        "missing-branch-failure" => require(
            last["wait"] == 300 && last["consistent"] == false,
            "missing branch policy mismatch",
        )?,
        "tizi-branch-map-agnos-same-version" => require(
            last["consistent"] == true && array(native, "agnos")?.is_empty(),
            "same-version AGNOS mismatch",
        )?,
        "agnos-model-manifest-boundary" => {
            require(last["consistent"] == true, "AGNOS snapshot inconsistent")?;
            let agnos = array(native, "agnos")?
                .last()
                .ok_or("missing AGNOS action")?;
            require(
                agnos["manifest"]
                    .as_str()
                    .is_some_and(|m| m.ends_with("/agnos-tici.json"))
                    && agnos["slot"] == 1,
                "AGNOS manifest boundary mismatch",
            )?;
        }
        "connectivity-thresholds" => {
            for (i, key, present) in [
                (1, "Offroad_ConnectivityNeededPrompt", false),
                (2, "Offroad_ConnectivityNeededPrompt", true),
                (4, "Offroad_ConnectivityNeeded", true),
                (5, "Offroad_ConnectivityNeeded", false),
                (6, "Offroad_UpdateFailed", true),
            ] {
                require(
                    params(i)?.get(key).is_some() == present,
                    "connectivity threshold mismatch",
                )?;
            }
        }
        _ => return Err("unknown updater case".into()),
    }
    Ok(())
}

struct Execution<'a> {
    root: &'a Path,
    output: &'a Path,
    name: &'a str,
    seed: &'a updated_fixture::Seed,
    launcher: &'a Path,
    binary: &'a Path,
    shim: &'a Path,
    options: &'a Value,
    steps: &'a Value,
}
fn one(execution: &Execution<'_>) -> Result<Value> {
    let Execution {
        root,
        output,
        name,
        seed,
        launcher,
        binary,
        shim,
        options,
        steps,
    } = execution;
    let setup = updated_fixture::setup(root, seed, launcher, shim)?;
    let mut config = setup.config;
    let staging = PathBuf::from(
        config["paths"]["staging"]
            .as_str()
            .ok_or("missing staging path")?,
    );
    let retained = staging.join("old_openpilot.fixture/owned-rollback");
    fs::create_dir_all(retained.parent().ok_or("missing rollback parent")?)?;
    fs::write(&retained, ROLLBACK)?;
    let lock = staging.join(".runtime-update.lock");
    fs::write(&lock, LOCK)?;
    let held = File::open(&lock)?;
    let inode = held.metadata()?.ino();
    for (key, value) in options.as_object().ok_or("case options missing")? {
        config[key] = value.clone();
    }
    config["steps"] = (*steps).clone();
    let mut environment = setup.environment;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root missing")?;
    environment.insert(
        OsString::from("PYTHONPATH"),
        repository.as_os_str().to_owned(),
    );
    let mut command =
        CommandSpec::new_os(vec![binary.as_os_str().to_owned()], Duration::from_secs(60));
    command.input = Some(serde_json::to_vec(&config)?);
    command.set_os_env(environment);
    command.env_remove.push(OsString::from("OPENPILOT_PREFIX"));
    let result = NativeRunner.run(&command)?;
    let stem = output.join(format!("{name}-native"));
    fs::write(
        stem.with_extension("stdout"),
        if result.stdout.is_empty() {
            b"<empty>\n".as_slice()
        } else {
            result.stdout.as_slice()
        },
    )?;
    fs::write(
        stem.with_extension("stderr"),
        if result.stderr.is_empty() {
            b"<empty>\n".as_slice()
        } else {
            result.stderr.as_slice()
        },
    )?;
    require(
        result.code == 0,
        "updater child failed; inspect retained stdout/stderr",
    )?;
    require(
        git_head(&root.join("base"))? == seed.previous,
        "updater changed active Git HEAD",
    )?;
    require(
        fs::read(&retained)? == ROLLBACK && fs::read(&lock)? == LOCK,
        "native lost rollback or lock bytes",
    )?;
    require(
        fs::symlink_metadata(&lock)?.ino() == inode && held.metadata()?.ino() == inode,
        "native replaced held lock inode",
    )?;
    let last = result
        .stdout
        .split(|byte| *byte == b'\n')
        .rfind(|line| !line.is_empty())
        .ok_or("updater result JSON missing")?;
    let value = updated_fixture::normalize(&serde_json::from_slice(last)?, root)?;
    output_value(&value, &stem.with_extension("json"))?;
    Ok(value)
}

pub fn run(
    binary: &Path,
    launcher: &Path,
    output: &Path,
    shim: &Path,
    admission_root: &Path,
) -> Result<Value> {
    fs::create_dir_all(output)?;
    match fs::symlink_metadata(output.join("result.json")) {
        Ok(_) => return Err("updated checker output already contains result.json".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root missing")?;
    let historical = historical_sources(admission_root)?;
    let mut original_inputs = Vec::new();
    let mut original_pins = serde_json::Map::new();
    for (relative, expected) in ORIGINAL_INPUTS {
        let (path, digest) = bound(&repo.join(relative))?;
        require(
            digest == expected,
            "updated original dependency pin mismatch",
        )?;
        original_pins.insert(relative.to_owned(), Value::String(digest.clone()));
        original_inputs.push((path, digest));
    }
    let (binary, binary_sha) = bound(binary)?;
    let (launcher, launcher_sha) = bound(launcher)?;
    let (shim, shim_sha) = bound(shim)?;
    let temporary = tempfile::Builder::new()
        .prefix("updated-source-")
        .tempdir()?;
    let seed = updated_fixture::seed(temporary.path())?;
    let mut rows = Vec::new();
    for (name, options, steps) in cases() {
        let native_root = temporary.path().join(format!("{name}-native"));
        let execution = Execution {
            root: &native_root,
            output,
            name,
            seed: &seed,
            launcher: &launcher,
            binary: &binary,
            shim: &shim,
            options: &options,
            steps: &steps,
        };
        let original = sealed_original(name)?;
        let native = one(&execution)?;
        let removed = assert_equivalent(&original, &native)?;
        verify_case(name, &native)?;
        let row = json!({"scenario":name,"snapshots":snapshots(&original)?.len(),"commands":array(&native,"commands")?.len(),"obsolete_staging_removals":removed,"rollback_and_lock_preserved":true,"passed":true});
        println!(
            "PASS {name}: {} snapshots, {} ordered native commands",
            row["snapshots"], row["commands"]
        );
        rows.push(row);
        for (path, digest) in [
            (&binary, &binary_sha),
            (&launcher, &launcher_sha),
            (&shim, &shim_sha),
        ] {
            unchanged(path, digest)?;
        }
        require(
            historical_sources(admission_root)? == historical,
            "updater historical source presence changed",
        )?;
        for (path, digest) in &original_inputs {
            unchanged(path, digest)?;
        }
    }
    let report = json!({"passed":true,"scenarios":rows,"device_access":false,"original_observation":"sealed_df0128aa_capture","pins":{"historical_source_sha256":historical,"original_dependencies":original_pins,"native_binary":binary_sha,"launcher":launcher_sha,"overlay_shim":shim_sha}});
    require(
        historical_sources(admission_root)? == historical,
        "updater historical source presence changed before receipt",
    )?;
    output_value(&report, &output.join("result.json"))?;
    Ok(report)
}

#[derive(Debug)]
pub struct CliOptions {
    pub binary: PathBuf,
    pub launcher: PathBuf,
    pub output: PathBuf,
    pub root: PathBuf,
}

pub fn parse(args: &[OsString]) -> Result<Option<CliOptions>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--launcher", true),
            OptionSpec::value("--binding", false),
            OptionSpec::value("--output", true),
            OptionSpec::value("--root", false),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    let get = |key: &str| -> Result<PathBuf> {
        Ok(PathBuf::from(
            parsed
                .values
                .get(key)
                .ok_or_else(|| format!("missing {key}"))?
                .clone(),
        ))
    };
    Ok(Some(CliOptions {
        binary: get("--binary")?,
        launcher: get("--launcher")?,
        output: get("--output")?,
        root: parsed
            .values
            .get("--root")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(3)
                    .unwrap()
                    .to_path_buf()
            }),
    }))
}

pub fn cli() -> Result<()> {
    let Some(options) = parse(&env::args_os().skip(1).collect::<Vec<_>>())? else {
        println!("usage: check-updated --binary BINARY --launcher LAUNCHER --output OUTPUT [--root ROOT]");
        return Ok(());
    };
    let shim = env::current_exe()?.with_file_name("updated-fixture");
    run(
        &options.binary,
        &options.launcher,
        &options.output,
        &shim,
        &options.root,
    )?;
    Ok(())
}
