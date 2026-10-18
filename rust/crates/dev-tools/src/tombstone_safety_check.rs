//! Real harmless-marker proof of the approved native filename correction (#77).
use crate::{
    path_resolve::resolve_non_strict,
    seal::{digest, Result},
    tombstone_reference_check::Pair,
    version_reference_check::process::Cancellation,
};
use openpilot_log_format::JsonValue as J;
use std::{collections::BTreeMap, ffi::OsString, fs, path::Path};

pub use crate::tombstone_reference_check::{Interrupted, Options};
const BODY: &str = "ExecutablePath: /data/openpilot/selfdrive/modeld/modeld\nSignal: 11\nProcMaps:\n omitted\nProcStatus:\n kept\nCoreDump: omitted\n";
const SAFETY_SOURCE: &str = "rust/tools/check_tombstoned_safety.py";
const SAFETY_SOURCE_SHA256: &str =
    "2b70f9cd79cde571fed12d5f15f06ff080786de23de54cc47dd0187d9862f0a4";

pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Options> {
    if args.len() < 3 {
        return Err(
            "tombstone safety requires BINARY PARAMS_BINDING OUTPUT [RUNNER ARGS...]".into(),
        );
    }
    Ok(Options {
        binary: resolve_non_strict(&cwd.join(&args[0]))?,
        params_binding: resolve_non_strict(&cwd.join(&args[1]))?,
        output: resolve_non_strict(&cwd.join(&args[2]))?,
        runner: args[3..].to_vec(),
        environment: environment.clone(),
    })
}
pub fn format_report(report: &J) -> Result<String> {
    crate::control_tools_policy_check::json::pretty(report, 0)
}
fn object(fields: &[(&str, J)]) -> Result<J> {
    Ok(J::parse(&format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(key, value)| Ok(format!("{}:{}", J::text(key).to_json()?, value.to_json()?)))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
fn array(values: &[J]) -> Result<J> {
    Ok(J::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(J::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
fn sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    let mut pins = crate::tombstone_source::sources(repository)?;
    for (name, expected) in [
        (SAFETY_SOURCE, SAFETY_SOURCE_SHA256),
        (
            "openpilot/common/params_pyx.pyx",
            "a8cc5e5296dd38a012c55b0867c4563f8e320d25960260d897675dd90f0ea544",
        ),
    ] {
        if name == SAFETY_SOURCE {
            // Only actual absence of this standalone retired checker is allowed.
            // A dangling symlink is still present and must fail the digest read.
            match fs::symlink_metadata(repository.join(name)) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let hash = digest(&repository.join(name))?;
        if hash != expected {
            return Err(format!("changed tombstone safety source {name}").into());
        }
        pins.insert(name.into(), hash);
    }
    Ok(pins)
}
fn remove_report(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub fn run(repository: &Path, options: &Options) -> Result<J> {
    remove_report(&options.output.join("safety.json"))?;
    remove_report(&options.output.join("source-provenance.json"))?;
    let pinned = sources(repository)?;
    let binary_hash = digest(&options.binary)?;
    let binding_hash = digest(&options.params_binding)?;
    let cancel = Cancellation::new()?;
    let result = run_inner(
        repository,
        options,
        &cancel,
        &pinned,
        &binary_hash,
        &binding_hash,
    );
    if let Some(signal) = cancel.signal() {
        remove_report(&options.output.join("safety.json"))?;
        return Err(Interrupted(signal).into());
    }
    result
}
fn run_inner(
    repository: &Path,
    options: &Options,
    cancel: &Cancellation,
    pinned: &BTreeMap<String, String>,
    binary_hash: &str,
    binding_hash: &str,
) -> Result<J> {
    let mut pair = Pair::new(repository, options, &options.output, cancel)?;
    let mut cases = Vec::new();
    for (name, marker) in [
        ("x$(touch marker_substitution).crash", "marker_substitution"),
        ("x`touch marker_backtick`.crash", "marker_backtick"),
        ("x\"$(touch marker_quote)\".crash", "marker_quote"),
    ] {
        cancel.check()?;
        let relative = format!("apport/{name}");
        pair.write(Path::new(&relative), BODY.as_bytes(), Some(0o640))?;
        pair.call(
            &object(&[
                ("op", J::text("retrace")),
                ("path", J::text(&format!("$FIXTURE/{relative}"))),
            ])?,
            false,
        )?;
        let [source_marker, native_marker] = pair.paths(Path::new(marker));
        if !source_marker.is_file() {
            return Err("original source did not reproduce the benign marker".into());
        }
        if native_marker.try_exists()? {
            return Err("native adapter executed filename shell text".into());
        }
        let [source_input, native_input] = pair.paths(Path::new("retrace.input"));
        let source_input = fs::read_to_string(source_input)?;
        let native_input = fs::read_to_string(native_input)?;
        if !native_input.contains(BODY) || source_input.contains(BODY) {
            return Err("retrace did not preserve the approved literal filename difference".into());
        }
        cases.push(object(&[
            ("filename", J::text(name)),
            ("source_marker_created", J::parse("true")?),
            ("native_marker_created", J::parse("false")?),
            ("source_retrace_input", J::text(&source_input)),
            ("native_retrace_input", J::text(&native_input)),
        ])?);
    }
    pair.write(
        Path::new("apport/ordinary space.crash"),
        BODY.as_bytes(),
        Some(0o640),
    )?;
    pair.call(
        &object(&[
            ("op", J::text("retrace")),
            ("path", J::text("$FIXTURE/apport/ordinary space.crash")),
        ])?,
        false,
    )?;
    let [source_input, native_input] = pair.paths(Path::new("retrace.input"));
    if fs::read(source_input)? != fs::read(native_input)? {
        return Err("ordinary filename spaces changed retrace input".into());
    }
    if pair.finish()? != 5 {
        return Err("incomplete original tombstone safety comparison roster".into());
    }
    if &sources(repository)? != pinned
        || digest(&options.binary)? != binary_hash
        || digest(&options.params_binding)? != binding_hash
    {
        return Err("tombstone safety source or artifact changed during comparison".into());
    }
    cancel.check()?;
    let mut command = options.runner.clone();
    command.push(options.binary.as_os_str().to_owned());
    let report = object(&[
        ("result", J::text("PASS")),
        ("approved_difference", J::text("source executes shell substitutions; native passes filename as a positional argument")),
        ("source_issue", J::parse("77")?),
        ("cases", array(&cases)?),
        ("ordinary_spaces_preserved", J::parse("true")?),
        ("binary_sha256", J::text(binary_hash)),
        ("command", array(&command.iter().map(|value| crate::python_fs::json_value(value)).collect::<Vec<_>>())?),
    ])?;
    let historical = if pinned.contains_key(SAFETY_SOURCE) {
        BTreeMap::new()
    } else {
        BTreeMap::from([(SAFETY_SOURCE, SAFETY_SOURCE_SHA256)])
    };
    let provenance = serde_json::json!({"source_sha256": pinned, "historical_source_sha256": historical, "binary_sha256": binary_hash, "params_binding_sha256": binding_hash});
    fs::write(
        options.output.join("source-provenance.json"),
        format!("{}\n", serde_json::to_string_pretty(&provenance)?),
    )?;
    fs::write(
        options.output.join("safety.json"),
        format!("{}\n", format_report(&report)?),
    )?;
    Ok(report)
}
