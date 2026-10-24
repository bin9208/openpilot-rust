//! Independent original manager constructors and getter/callback policy.
//! The runtime manager-catalog, Params and integer implementations are not used.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use ops::PythonError;
use std::{
    ffi::OsString,
    fmt, fs,
    path::{Path, PathBuf},
};
mod catalog;
pub(crate) mod ops;
mod params;
mod policy;
pub(crate) use policy::PREDICATES;
pub(crate) fn string(value: &JsonValue) -> Result<String> {
    ops::utf8(value)
}
pub(crate) fn catalog_names() -> Vec<&'static str> {
    catalog::entries().into_iter().map(|v| v.name).collect()
}
#[derive(Clone, Default)]
pub struct Context {
    pub tici: bool,
    pub darwin: bool,
    pub webcam: Option<OsString>,
    pub carrot_web_external: Option<OsString>,
}
impl Context {
    pub fn current() -> Self {
        Self {
            tici: Path::new("/TICI").is_file(),
            darwin: cfg!(target_os = "macos"),
            webcam: std::env::var_os("USE_WEBCAM"),
            carrot_web_external: std::env::var_os("CARROT_WEB_EXTERNAL"),
        }
    }
}
#[derive(Debug)]
struct ReferenceError {
    kind: &'static str,
    message: String,
}
impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.kind == "fatal_integer" {
            f.write_str(self.kind)
        } else {
            write!(f, "{}: {}", self.kind, self.message)
        }
    }
}
impl std::error::Error for ReferenceError {}
fn normalize(error: Box<dyn std::error::Error>) -> Box<dyn std::error::Error> {
    if let Some(e) = error.downcast_ref::<PythonError>() {
        Box::new(ReferenceError {
            kind: e.kind,
            message: e.message.clone(),
        })
    } else {
        error
    }
}
fn inner(raw: &[u8], output: &Path, context: &Context) -> Result<JsonValue> {
    let text =
        std::str::from_utf8(raw).map_err(|e| ops::error("UnicodeDecodeError", e.to_string()))?;
    let request = JsonValue::parse(text).map_err(|e| {
        ops::error(
            if matches!(e, openpilot_log_format::JsonError::IntegerLimit) {
                "ValueError"
            } else {
                "json.decoder.JSONDecodeError"
            },
            e.to_string(),
        )
    })?;
    fs::create_dir_all(output).map_err(ops::classify_io)?;
    let config = catalog::config(&request, context)?;
    let entries = catalog::entries();
    let descriptors = ops::array(
        &entries
            .iter()
            .map(|v| v.snapshot(&config))
            .collect::<Result<Vec<_>>>()?,
    )?;
    let cases = ops::sequence(&ops::default(&request, "cases", ops::array(&[])?)?)?;
    let mut results = Vec::new();
    let mut unserializable = false;
    for (index, case) in cases.iter().enumerate() {
        let mut params = params::Params::new(output, index, case)?;
        let name = ops::get(case, "predicate")?;
        if !matches!(name.view(), JsonView::Text(_)) {
            return Err(ops::error(
                "AttributeError",
                "predicate has no attribute startswith",
            ));
        }
        // getattr accepts Python codepoints, including lone surrogates. Unknown
        // names fail here before callback argument lookup, without UTF-8 encoding.
        let name = match name.view() {
            JsonView::Text(points) => {
                let process = points.starts_with(&[112, 114, 111, 99, 101, 115, 115, 58]);
                points
                    .iter()
                    .map(|&p| char::from_u32(p))
                    .collect::<Option<String>>()
                    .ok_or_else(|| {
                        ops::error(
                            if process {
                                "KeyError"
                            } else {
                                "AttributeError"
                            },
                            "unknown predicate",
                        )
                    })?
            }
            _ => unreachable!(),
        };
        let callback = if let Some(name) = name.strip_prefix("process:") {
            entries
                .iter()
                .rev()
                .find(|v| v.name == name)
                .map(|v| v.callback)
                .ok_or_else(|| ops::error("KeyError", format!("unknown process {name}")))?
        } else {
            if !policy::PREDICATES.contains(&name.as_str())
                && ![
                    "ublox_available",
                    "cluster_hud_active",
                    "or_",
                    "and_",
                    "DaemonProcess",
                    "PythonProcess",
                    "NativeProcess",
                    "Params",
                    "PC",
                    "TICI",
                    "WEBCAM",
                    "CARROT_WEB_EXTERNAL",
                    "BODYTELEOP_AVAILABLE",
                    "procs",
                    "managed_processes",
                    "os",
                    "platform",
                    "importlib",
                    "car",
                    "__name__",
                    "__doc__",
                    "__package__",
                    "__loader__",
                    "__spec__",
                    "__file__",
                    "__cached__",
                    "__builtins__",
                ]
                .contains(&name.as_str())
            {
                return Err(ops::error(
                    "AttributeError",
                    format!("unknown predicate {name}"),
                ));
            }
            name.as_str()
        };
        // Source callback argument lookup is inside the broad Python try block,
        // unlike callback-name resolution and initial Params filesystem setup.
        let outcome = match (|| {
            let started = ops::get(case, "started")?;
            let not_car = ops::get(case, "not_car")?;
            if matches!(callback, "or_" | "and_" | "DaemonProcess" | "PythonProcess") {
                // Constructors/combinators return an object, then json.dumps
                // fails only AFTER all subsequent cases and their access files.
                unserializable = true;
                Ok(ops::null())
            } else if !callback.starts_with("all:")
                && !callback.starts_with("any:")
                && !policy::PREDICATES.contains(&callback)
            {
                Err(ops::error(
                    "TypeError",
                    "module global cannot be called with three arguments",
                ))
            } else {
                policy::callback(callback, &started, &not_car, &mut params)
            }
        })() {
            Ok(value) => ops::object(&[("value", value)])?,
            Err(e)
                if e.downcast_ref::<PythonError>()
                    .is_some_and(|v| v.kind == "fatal_integer") =>
            {
                return Err(e)
            }
            Err(_) => ops::object(&[("error", ops::text("parameter"))])?,
        };
        results.push(ops::object(&[
            ("outcome", outcome),
            (
                "trace",
                JsonValue::parse(&serde_json::to_string(&params.trace)?)?,
            ),
            ("ublox", ops::boolean(params.persisted())),
        ])?);
    }
    if unserializable {
        return Err(ops::error(
            "TypeError",
            "callback object is not JSON serializable",
        ));
    }
    let result = ops::object(&[("catalog", descriptors), ("results", ops::array(&results)?)])?;
    fs::write(
        output.join("result.json"),
        format!(
            "{}\n",
            crate::control_tools_policy_check::json::pretty(&result, 0)?
        ),
    )
    .map_err(ops::classify_io)?;
    Ok(result)
}
pub fn evaluate(raw: &[u8], output: &Path, context: &Context) -> Result<JsonValue> {
    inner(raw, output, context).map_err(normalize)
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].to_str(), Some("-h" | "--help")) {
        println!("usage: manager-catalog-reference REQUEST OUTPUT");
        return Ok(());
    }
    if args.len() != 2 {
        return Err(crate::argparse_scan::ArgumentError(
            "usage: manager-catalog-reference REQUEST OUTPUT".into(),
        )
        .into());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    crate::manager_catalog_check::validate_source(&root)?;
    let request = fs::read(PathBuf::from(&args[0])).map_err(ops::classify_io)?;
    evaluate(&request, &PathBuf::from(&args[1]), &Context::current())?;
    Ok(())
}
