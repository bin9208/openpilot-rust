//! Independent original service definitions and full Event schema admission.
//! The generated messaging catalog is always the candidate, never the oracle.
use crate::{
    argparse_scan::{scan_os, ArgumentError, OptionSpec},
    seal::Result,
};
use capnp::introspect::{Introspect, TypeVariant};
use openpilot_cereal::log_capnp::event;
use openpilot_log_format::JsonValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fmt::Write,
    fs,
    io::{self, Write as IoWrite},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const RETIRED: &str = "rust/tools/check_services.py";
const SOURCES: [&str; 8] = [
    RETIRED,
    "openpilot/cereal/services.py",
    "openpilot/cereal/__init__.py",
    "openpilot/cereal/log.capnp",
    "openpilot/cereal/custom.capnp",
    "openpilot/cereal/deprecated.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
    "openpilot/cereal/include/c++.capnp",
];
const CORPORA: [&str; 6] = [
    "catalog.json",
    "generated.rs",
    "schema.json",
    "process-original.json",
    "schema-original.json",
    "format-original.json",
];
const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/services-catalog/manifest.json");
const ORIGINAL: &[u8] = include_bytes!("../tests/fixtures/services-catalog/catalog.json");
const REPR_MANIFEST: &[u8] =
    include_bytes!("../tests/fixtures/services-catalog/cli-repr-manifest.json");
const MISSING: [&str; 3] = [
    "navModel",
    "customReservedRawData1",
    "customReservedRawData2",
];
const CATALOG_ERROR: &str = "Rust service catalog differs from source; regenerate with --emit-rust";
const SCHEMA_ERROR: &str =
    "source schema availability changed; update the explicit Rust default-payload parity cases";
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Service {
    pub name: String,
    pub should_log: bool,
    pub frequency: f64,
    pub decimation: Option<i64>,
    pub queue_size: i64,
    pub frequency_range: Option<[f64; 2]>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Require the entire captured closure; deleting a critical pin is not drift approval.
pub fn validate_manifest(manifest: &Value) -> Result<()> {
    for (name, required) in [
        ("sources", SOURCES.as_slice()),
        ("corpora", CORPORA.as_slice()),
    ] {
        let values = manifest[name]
            .as_object()
            .ok_or("missing original service provenance map")?;
        let actual = values.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if actual != required.iter().copied().collect::<BTreeSet<_>>() {
            return Err(format!("original service {name} pin keyset differs").into());
        }
        for value in values.values() {
            let valid = value.as_str().is_some_and(|v| {
                v.len() == 64
                    && v.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            });
            if !valid {
                return Err("invalid original service provenance hash".into());
            }
        }
    }
    let dependencies = manifest["dependency"]
        .as_array()
        .ok_or("missing original pycapnp dependency")?;
    if dependencies.len() != 1
        || dependencies[0]["name"] != "pycapnp"
        || dependencies[0]["version"] != "2.1.0"
    {
        return Err("original pycapnp dependency identity differs".into());
    }
    Ok(())
}
fn manifest() -> Result<Value> {
    if hash(MANIFEST) != "cc0626dc951421809a30c2923deb1fe30721efc0278583d267d91b3a87d1fd97" {
        return Err("original service manifest integrity mismatch".into());
    }
    let value: Value = serde_json::from_slice(MANIFEST)?;
    validate_manifest(&value)?;
    Ok(value)
}
/// All 88 independent original tuples, not the candidate's generated include.
pub fn catalog() -> Result<Vec<Service>> {
    let manifest = manifest()?;
    if Some(hash(ORIGINAL).as_str()) != manifest["corpora"]["catalog.json"].as_str() {
        return Err("original service catalog integrity mismatch".into());
    }
    Ok(serde_json::from_slice(ORIGINAL)?)
}
fn float(value: f64) -> Result<String> {
    if value.is_nan() {
        return Ok("nan".into());
    }
    if value == f64::INFINITY {
        return Ok("inf".into());
    }
    if value == f64::NEG_INFINITY {
        return Ok("-inf".into());
    }
    // Existing native Python JSON formatter uses shortest-even dtoa and Python's
    // fixed/scientific boundary. JSON and repr share finite float spellings.
    Ok(JsonValue::parse(&serde_json::to_string(&value)?)?.to_json()?)
}
/// Preserve the source generator's field/order/whitespace and numeric spellings.
pub fn render(catalog: &[Service]) -> Result<String> {
    let mut out = String::from("pub const SERVICES: &[Service] = &[\n");
    for row in catalog {
        let decimation = row
            .decimation
            .map_or_else(|| "None".into(), |v| format!("Some({v})"));
        let range = match row.frequency_range {
            None => "None".into(),
            Some([lo, hi]) => format!("Some(({}, {}))", float(lo)?, float(hi)?),
        };
        writeln!(out,"    Service {{\n        name: \"{}\",\n        should_log: {},\n        frequency: {},\n        decimation: {},\n        queue_size: {},\n        frequency_range: {},\n    }},",row.name,row.should_log,float(row.frequency)?,decimation,row.queue_size,range)?;
    }
    out.push_str("];\n");
    Ok(out)
}
/// The complete native-generated Event struct fields, including deprecated fields.
pub fn schema_fields() -> Result<Vec<String>> {
    let TypeVariant::Struct(schema) = event::Owned::introspect().which() else {
        return Err("Event schema must be a struct".into());
    };
    let schema: capnp::schema::StructSchema = schema.into();
    schema
        .get_fields()?
        .iter()
        .map(|field| Ok(field.get_proto().get_name()?.to_str()?.to_owned()))
        .collect()
}
/// Keep original generation -> candidate read/text equality -> schema check order.
/// Emit mode bypasses candidate IO and the schema assertion, as the source does.
pub fn check_with(
    root: &Path,
    emit: bool,
    catalog: &[Service],
    fields: &[String],
) -> Result<String> {
    let generated = render(catalog)?;
    if emit {
        return Ok(generated);
    }
    let text = fs::read_to_string(root.join("rust/crates/messaging/src/services_generated.rs"))?;
    // pathlib.read_text opens in universal-newline mode. This is text equality,
    // while the emitted generated source itself retains its exact LF bytes.
    if text.replace("\r\n", "\n").replace('\r', "\n") != generated {
        return Err(CATALOG_ERROR.into());
    }
    let present = fields.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let missing = catalog
        .iter()
        .filter(|v| !present.contains(v.name.as_str()))
        .map(|v| v.name.as_str())
        .collect::<Vec<_>>();
    if missing != MISSING {
        return Err(SCHEMA_ERROR.into());
    }
    Ok(format!(
        "{} service definitions match original source\n",
        catalog.len()
    ))
}
/// Original sources stay mandatory. Only the retired checker may be absent; a
/// changed or reintroduced checker must still match its immutable original hash.
pub fn validate_source(root: &Path) -> Result<()> {
    let manifest = manifest()?;
    for path in SOURCES {
        match fs::read(root.join(path)) {
            Ok(bytes) if Some(hash(&bytes).as_str()) == manifest["sources"][path].as_str() => (),
            Err(error) if path == RETIRED && error.kind() == io::ErrorKind::NotFound => (),
            _ => {
                return Err(format!(
                    "{path} source changed or absent; independently recapture service catalog"
                )
                .into())
            }
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let dependencies = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing original dependency lock")?
        .iter()
        .filter(|v| v.get("name").and_then(toml::Value::as_str) == Some("pycapnp"))
        .collect::<Vec<_>>();
    if serde_json::to_value(dependencies)? != manifest["dependency"] {
        return Err("original service pycapnp dependency changed; independently recapture".into());
    }
    // Check the schema inputs compiled with this binary too: a mutable checkout
    // cannot establish that a stale binary contains the pinned Event definition.
    for (path, bytes) in [
        (
            "openpilot/cereal/log.capnp",
            include_bytes!("../../../../openpilot/cereal/log.capnp").as_slice(),
        ),
        (
            "openpilot/cereal/custom.capnp",
            include_bytes!("../../../../openpilot/cereal/custom.capnp").as_slice(),
        ),
        (
            "openpilot/cereal/deprecated.capnp",
            include_bytes!("../../../../openpilot/cereal/deprecated.capnp").as_slice(),
        ),
        (
            "opendbc_repo/opendbc/car/car.capnp",
            include_bytes!("../../../../opendbc_repo/opendbc/car/car.capnp").as_slice(),
        ),
        (
            "openpilot/cereal/include/c++.capnp",
            include_bytes!("../../../../openpilot/cereal/include/c++.capnp").as_slice(),
        ),
    ] {
        if Some(hash(bytes).as_str()) != manifest["sources"][path].as_str() {
            return Err(format!("compiled {path} differs from original schema").into());
        }
    }
    validate_repr_sources(root)?;
    Ok(())
}
fn validate_repr_sources(root: &Path) -> Result<()> {
    if hash(REPR_MANIFEST) != "3bdf854067bd2c4861735c4774b5eccbd9e7a722446d828aa5a8f021306e9ed4" {
        return Err("original service CLI repr manifest integrity mismatch".into());
    }
    let manifest: Value = serde_json::from_slice(REPR_MANIFEST)?;
    if manifest["base_manifest_sha256"] != hash(MANIFEST)
        || manifest["original_checker_sha256"] != self::manifest()?["sources"][RETIRED]
    {
        return Err("service CLI repr original provenance differs".into());
    }
    let pins = manifest["format_sources"]
        .as_object()
        .ok_or("missing service repr source pins")?;
    let required = [
        (
            "rust/crates/dev-tools/src/plot_samples_source.rs",
            include_bytes!("plot_samples_source.rs").as_slice(),
        ),
        (
            "rust/crates/dev-tools/tests/fixtures/plot-samples/repr-nonprintable.json",
            include_bytes!("../tests/fixtures/plot-samples/repr-nonprintable.json").as_slice(),
        ),
    ];
    if pins.keys().map(String::as_str).collect::<BTreeSet<_>>()
        != required.iter().map(|(p, _)| *p).collect::<BTreeSet<_>>()
    {
        return Err("service repr source pin keyset differs".into());
    }
    for (path, bytes) in required {
        if Some(hash(bytes).as_str()) != manifest["format_sources"][path].as_str()
            || Some(hash(&fs::read(root.join(path))?).as_str())
                != manifest["format_sources"][path].as_str()
        {
            return Err(
                format!("{path} service repr source changed; independently recapture").into(),
            );
        }
    }
    Ok(())
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn usage(program: &str) -> String {
    format!("usage: {program} [-h] [--emit-rust]\n")
}
fn argument_text(value: &std::ffi::OsStr) -> String {
    let mut text = String::new();
    for point in crate::python_fs::codepoints(value) {
        if let Some(ch) = char::from_u32(point) {
            text.push(ch);
        } else {
            write!(text, "\\u{point:04x}").expect("String write");
        }
    }
    text
}
fn argument_error(program: &str, args: &[OsString], error: ArgumentError) -> ArgumentError {
    let text = if error.0 == "unrecognized arguments" {
        let mut terminated = false;
        let extras = args
            .iter()
            .filter(|a| {
                let a = a.to_string_lossy();
                if terminated {
                    return true;
                }
                if a == "--" {
                    terminated = true;
                    return true;
                }
                !(a.starts_with("--") && "--emit-rust".starts_with(a.as_ref()))
            })
            .map(|v| argument_text(v))
            .collect::<Vec<_>>()
            .join(" ");
        format!("unrecognized arguments: {extras}")
    } else if let Some(option) = error.0.strip_suffix(" does not accept a value") {
        let explicit = args
            .iter()
            .find_map(|v| {
                let bytes = v.as_bytes();
                let at = bytes.iter().position(|v| *v == b'=')?;
                let key = std::str::from_utf8(&bytes[..at]).ok()?;
                (option.starts_with(key) || (option == "--help" && key == "-h"))
                    .then(|| OsStr::from_bytes(&bytes[at + 1..]))
            })
            .unwrap_or_else(|| OsStr::new(""));
        let option = if option == "--help" {
            "-h/--help"
        } else {
            option
        };
        format!(
            "argument {option}: ignored explicit argument {}",
            crate::plot_samples_source::text_repr(&crate::python_fs::codepoints(explicit))
        )
    } else {
        error.0
    };
    ArgumentError(format!("{}{program}: error: {text}", usage(program)))
}
/// The original command has one optional flag and no subprocess or interpreter.
pub fn cli() -> Result<()> {
    let root = root();
    // Original service/schema imports occur before argparse, including for help.
    validate_source(&root)?;
    let rows = catalog()?;
    let fields = schema_fields()?;
    let program = std::env::args_os()
        .next()
        .and_then(|v| {
            Path::new(&v)
                .file_name()
                .map(|v| v.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "check-services".into());
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = scan_os(&args, &[OptionSpec::flag("--emit-rust", None)])
        .map_err(|e| argument_error(&program, &args, e))?;
    let text = if parsed.help {
        format!(
            "{}\noptions:\n  -h, --help   show this help message and exit\n  --emit-rust\n",
            usage(&program)
        )
    } else {
        check_with(
            &root,
            parsed.values.contains_key("--emit-rust"),
            &rows,
            &fields,
        )?
    };
    io::stdout().lock().write_all(text.as_bytes())?;
    Ok(())
}
