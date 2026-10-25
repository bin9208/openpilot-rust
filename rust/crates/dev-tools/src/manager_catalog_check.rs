//! Full independent manager catalog/callback checker. The candidate is only an
//! external executable; the expected policy lives in manager_catalog_reference.
use crate::{
    argparse_scan::ArgumentError,
    control_tools_policy_check::json as pyjson,
    manager_catalog_reference::ops as v,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};
mod requests;
pub struct Scenario {
    pub name: String,
    pub request: Vec<u8>,
    pub environment: BTreeMap<String, Option<OsString>>,
    pub fatal: bool,
}
pub fn suite_requests() -> Result<Vec<Scenario>> {
    requests::generate()
}
const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/manager-catalog/manifest.json");
const RETIRED: [&str; 2] = [
    "rust/tools/check_manager_catalog.py",
    "rust/tools/manager_catalog_reference.py",
];
fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn require(ok: bool, detail: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(detail.into().into())
    }
}
/// Permit absence of only these two retired tools; changed/reintroduced tools
/// and every retained original constructor/Params source remain strict pins.
pub fn validate_source(root: &Path) -> Result<Value> {
    require(
        hash(MANIFEST) == "6baafae469725142680ed1aeda918e114b1df3453d552adcd239c144f2c157a4",
        "manager original manifest changed",
    )?;
    let manifest: Value = serde_json::from_slice(MANIFEST)?;
    let pins = manifest["source_sha256"]
        .as_object()
        .ok_or("manager original source map missing")?;
    require(pins.len() == 24, "manager source keyset changed")?;
    for (name, expected) in pins {
        match fs::read(root.join(name)) {
            Ok(bytes) => require(
                Some(hash(&bytes).as_str()) == expected.as_str(),
                format!("manager original source changed: {name}"),
            )?,
            Err(e) if e.kind() == io::ErrorKind::NotFound && RETIRED.contains(&name.as_str()) => {}
            Err(e) => return Err(format!("manager original source absent: {name}: {e}").into()),
        }
    }
    Ok(manifest["source_sha256"].clone())
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub reference: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
}
fn object(fields: &[(&str, JsonValue)]) -> Result<JsonValue> {
    v::object_dynamic(
        &fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<Vec<_>>(),
    )
}
fn from_serde(value: &Value) -> Result<JsonValue> {
    Ok(JsonValue::parse(&value.to_string())?)
}
fn identity(path: &Path) -> Result<JsonValue> {
    // Identity is evidence, not executable admission: source runs before a
    // missing candidate's natural spawn failure in the original checker.
    let field = match fs::read(path) {
        Ok(bytes) => ("sha256", JsonValue::text(&hash(&bytes))),
        Err(error) => ("unavailable", JsonValue::text(&error.to_string())),
    };
    object(&[
        ("path", crate::python_fs::json_value(path.as_os_str())),
        field,
    ])
}

fn invocation(argv: &[OsString], code: i32) -> Result<JsonValue> {
    object(&[
        (
            "argv",
            v::array(
                &argv
                    .iter()
                    .map(|s| crate::python_fs::json_value(s))
                    .collect::<Vec<_>>(),
            )?,
        ),
        ("exit_code", v::number(code)),
    ])
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("candidate result missing {key}").into())
}
fn check_availability(root: &Path, result: &JsonValue) -> Result<()> {
    let inventory: Value = serde_json::from_slice(&fs::read(root.join("rust/port-status.json"))?)?;
    let mut statuses = BTreeMap::new();
    for entry in inventory["processes"]
        .as_array()
        .ok_or("process inventory missing")?
    {
        statuses.insert(
            entry["name"].as_str().ok_or("inventory name missing")?,
            entry["status"].as_str().ok_or("inventory status missing")?,
        );
    }
    let mut packages = Vec::new();
    for entry in fs::read_dir(root.join("rust/crates"))? {
        let path = entry?.path().join("Cargo.toml");
        if path.is_file() {
            packages.push(toml::from_str::<toml::Value>(&fs::read_to_string(path)?)?);
        }
    }
    for entry in v::sequence(&field(result, "availability")?)? {
        let name = crate::manager_catalog_reference::string(&field(&entry, "name")?)?;
        let status = crate::manager_catalog_reference::string(&field(&entry, "status")?)?;
        require(
            (status == "not_ported")
                == (*statuses
                    .get(name.as_str())
                    .ok_or("inventory process absent")?
                    == "not_ported"),
            format!("availability status differs: {name}"),
        )?;
        if status == "candidate" {
            let package = crate::manager_catalog_reference::string(&field(&entry, "package")?)?;
            let binary = crate::manager_catalog_reference::string(&field(&entry, "binary")?)?;
            let manifest = packages
                .iter()
                .find(|m| {
                    m.get("package")
                        .and_then(|v| v.get("name"))
                        .and_then(toml::Value::as_str)
                        == Some(package.as_str())
                })
                .ok_or("candidate package absent")?;
            let bins = manifest
                .get("bin")
                .and_then(toml::Value::as_array)
                .is_some_and(|bins| {
                    bins.iter().any(|b| {
                        b.get("name").and_then(toml::Value::as_str) == Some(binary.as_str())
                    })
                });
            require(
                bins || binary == package,
                format!("candidate binary absent: {name}"),
            )?;
            require(
                v::truth(&field(&entry, "limitation")?),
                format!("candidate limitation absent: {name}"),
            )?;
        }
    }
    Ok(())
}
/// Execute every child sequentially. The generic process owner enforces each
/// original 600-second deadline, disables core dumps and reaps its owned group.
pub fn run(options: &Options, runner: &mut impl Runner) -> Result<Value> {
    let sources = validate_source(&options.root)?;
    fs::create_dir_all(&options.output)?;
    let checker = std::env::current_exe()?;
    let initial = object(&[
        ("checker", identity(&checker)?),
        ("reference", identity(&options.reference)?),
        ("candidate", identity(&options.binary)?),
    ])?;
    fs::write(
        options.output.join("inputs.json"),
        format!("{}\n", pyjson::pretty(&initial, 0)?),
    )?;
    let mut rows = Vec::new();
    let mut last_catalog = None;
    let mut predicate_cases = 0;
    for scenario in suite_requests()? {
        let folder = options.output.join(&scenario.name);
        fs::create_dir(&folder)?;
        let request = folder.join("request.json");
        fs::write(&request, &scenario.request)?;
        let mut outputs = Vec::new();
        let mut invocations = Vec::new();
        for source in [true, false] {
            let kind = if source { "source" } else { "native" };
            let output = folder.join(kind);
            let mut argv = if source {
                vec![options.reference.as_os_str().to_owned()]
            } else {
                let mut argv = options.runner.clone();
                argv.push(options.binary.as_os_str().to_owned());
                argv
            };
            argv.extend([
                request.as_os_str().to_owned(),
                output.as_os_str().to_owned(),
            ]);
            let mut command = CommandSpec::new_os(argv.clone(), Duration::from_secs(600));
            command.cwd = Some(options.root.clone());
            command.disable_core_dump = true;
            command.inherit_stdin = true;
            command.env_remove.push("OPENPILOT_PREFIX".into());
            let mut env = BTreeMap::new();
            for (key, value) in &scenario.environment {
                if let Some(value) = value {
                    env.insert(OsString::from(key), value.clone());
                } else {
                    command.env_remove.push(key.into());
                }
            }
            command.set_os_env(env);
            let stdout = folder.join(format!("{kind}.stdout"));
            let stderr = folder.join(format!("{kind}.stderr"));
            let captured = runner.run_to_files(&command, &stdout, &stderr)?;
            invocations.push((kind.to_string(), invocation(&argv, captured.code)?));
            if source && scenario.fatal {
                require(
                    captured.code == 1 && fs::read(&stderr)?.ends_with(b"fatal_integer\n"),
                    format!(
                        "{}: independent reference must preserve fatal integer failure",
                        scenario.name
                    ),
                )?;
                outputs.push(JsonValue::parse(&fs::read_to_string(
                    output.join("trace-0.json"),
                )?)?);
            } else {
                require(
                    captured.code == 0,
                    format!(
                        "{} {kind} exit {}: {}",
                        scenario.name,
                        captured.code,
                        String::from_utf8_lossy(&fs::read(stderr)?)
                    ),
                )?;
                outputs.push(JsonValue::parse(&fs::read_to_string(
                    output.join("result.json"),
                )?)?);
            }
        }
        let observable = if scenario.fatal {
            let results = v::sequence(&field(&outputs[1], "results")?)?;
            let native = results.first().ok_or("native fatal result empty")?;
            require(
                pyjson::equal(
                    &field(native, "outcome")?,
                    &JsonValue::parse(r#"{"error":"fatal_integer"}"#)?,
                ),
                format!("{} native must retain typed fatal_integer", scenario.name),
            )?;
            require(
                pyjson::equal(&outputs[0], &field(native, "trace")?),
                format!("{} fatal access prefix differs", scenario.name),
            )?;
            "original Cython SIGABRT captured independently; native reference fatal_integer and candidate typed fatal_integer preserve identical access prefix".to_string()
        } else {
            for key in ["catalog", "results"] {
                require(
                    pyjson::equal(&field(&outputs[0], key)?, &field(&outputs[1], key)?),
                    format!(
                        "{} {key} differs from independent source policy",
                        scenario.name
                    ),
                )?;
            }
            let descriptors = v::sequence(&field(&outputs[0], "catalog")?)?;
            require(descriptors.len() == 63, "expected 63 original descriptors")?;
            let names = descriptors
                .iter()
                .map(|entry| crate::manager_catalog_reference::string(&field(entry, "name")?))
                .collect::<Result<BTreeSet<_>>>()?;
            require(
                names.len() == 63 && !names.contains("uploader"),
                "catalog uniqueness/uploader policy differs",
            )?;
            if scenario.name.starts_with("catalog-") {
                last_catalog = Some(outputs[1].clone());
            }
            let count = v::sequence(&field(&outputs[0], "results")?)?.len();
            predicate_cases += count;
            format!("63 ordered descriptors and {count} callback outcomes/access traces/persisted Ublox values identical")
        };
        let artifacts = [
            request,
            folder.join("native/result.json"),
            folder.join(if scenario.fatal {
                "source/trace-0.json"
            } else {
                "source/result.json"
            }),
        ];
        let row = object(&[
            ("scenario", JsonValue::text(&scenario.name)),
            ("invocations", v::object_dynamic(&invocations)?),
            ("observable", JsonValue::text(&observable)),
            (
                "artifacts",
                v::array(
                    &artifacts
                        .iter()
                        .map(|p| crate::python_fs::json_value(p.as_os_str()))
                        .collect::<Vec<_>>(),
                )?,
            ),
        ])?;
        fs::write(
            folder.join("comparison.json"),
            format!("{}\n", pyjson::pretty(&row, 0)?),
        )?;
        rows.push(row);
    }
    check_availability(
        &options.root,
        last_catalog.as_ref().ok_or("catalog scenarios absent")?,
    )?;
    require(predicate_cases == 10573, "callback suite count differs")?;
    let final_identity = object(&[
        ("checker", identity(&checker)?),
        ("reference", identity(&options.reference)?),
        ("candidate", identity(&options.binary)?),
    ])?;
    require(
        pyjson::equal(&initial, &final_identity),
        "executing checker/reference/candidate identity changed during run",
    )?;
    validate_source(&options.root)?;
    let summary = json!({"result":"PASS","descriptors":63,"catalog_configurations":64,"environment_snapshots":25,"predicate_cases":predicate_cases,"fatal_cases":24});
    let mut report_fields = Vec::new();
    for key in [
        "result",
        "descriptors",
        "catalog_configurations",
        "environment_snapshots",
        "predicate_cases",
        "fatal_cases",
    ] {
        report_fields.push((key, from_serde(&summary[key])?));
    }
    let original: Value = serde_json::from_slice(MANIFEST)?;
    report_fields.extend([
        ("comparisons",v::array(&rows)?),
        ("source_sha256",from_serde(&sources)?),
        ("binary_sha256",field(&field(&initial,"candidate")?,"sha256")?),
        ("executables",initial),
        ("original_capture",object(&[("binding_sha256",from_serde(&original["binding_sha256"])?),("probe_sha256",from_serde(&original["probe_sha256"])?),("scope",JsonValue::text("Immutable original capture provenance only; no interpreter/binding is loaded or claimed verified at runtime"))])?),
        ("limits",JsonValue::text("Policy API only; original Cython abort is captured provenance, native typed fatal errors are not SIGABRT parity. No manager launch, AGNOS or vehicle validation.")),
    ]);
    let report = object(&report_fields)?;
    fs::write(
        options.output.join("manifest.json"),
        format!("{}\n", pyjson::pretty(&report, 0)?),
    )?;
    Ok(summary)
}
fn root() -> PathBuf {
    std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut positional = Vec::new();
    let mut runner = Vec::new();
    let mut index = 0;
    let mut options = true;
    while index < args.len() {
        let arg = &args[index];
        if options && matches!(arg.to_str(), Some("-h" | "--help")) {
            println!("usage: check-manager-catalog BINARY OUTPUT [--runner ...]");
            return Ok(());
        }
        if options && arg == "--" {
            options = false;
            index += 1;
            continue;
        }
        if options && arg == "--runner" {
            runner.extend_from_slice(&args[index + 1..]);
            break;
        }
        if options && arg.as_encoded_bytes().starts_with(b"-") {
            return Err(
                ArgumentError(format!("unrecognized argument: {}", arg.to_string_lossy())).into(),
            );
        }
        positional.push(arg.clone());
        index += 1;
    }
    if positional.len() != 2 {
        return Err(ArgumentError(
            "usage: check-manager-catalog BINARY OUTPUT [--runner ...]".into(),
        )
        .into());
    }
    let resolve = |path: &OsString| crate::path_resolve::resolve_non_strict(Path::new(path));
    let options = Options {
        root: root(),
        binary: resolve(&positional[0])?,
        output: resolve(&positional[1])?,
        reference: std::env::current_exe()?.with_file_name("manager-catalog-reference"),
        runner,
    };
    let report = run(&options, &mut NativeRunner)?;
    let fields = [
        "result",
        "descriptors",
        "catalog_configurations",
        "environment_snapshots",
        "predicate_cases",
        "fatal_cases",
    ]
    .iter()
    .map(|&key| Ok((key, from_serde(&report[key])?)))
    .collect::<Result<Vec<_>>>()?;
    println!("{}", object(&fields)?.to_json()?);
    Ok(())
}
