//! Static ELF architecture, per-root dependency, version and strong-symbol audit.
//! Native readelf is an external inspection tool; no ELF object is executed.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Debug, Serialize)]
pub struct ElfInfo {
    pub dynamic: String,
    pub versions: String,
    pub symbols: String,
    pub needed: Vec<String>,
    pub definitions: Vec<String>,
    pub needs: BTreeMap<String, Vec<String>>,
    pub exports: BTreeSet<String>,
    pub imports: BTreeSet<String>,
}
fn token_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.split_once(marker)?.1.split_whitespace().next()
}
/// Parse GNU readelf --wide output, retaining the original inspection text.
pub fn parse_readelf(dynamic: String, versions: String, symbols: String) -> ElfInfo {
    let mut needed = Vec::new();
    for line in dynamic.lines() {
        if let Some((_, suffix)) = line.split_once("(NEEDED)") {
            if let Some((_, name)) = suffix.split_once('[') {
                if let Some((name, _)) = name.split_once(']') {
                    needed.push(name.to_owned());
                }
            }
        }
    }
    let definition_section = versions
        .rsplit("Version definition section")
        .next()
        .unwrap_or("")
        .split("Version needs section")
        .next()
        .unwrap_or("");
    let definitions = definition_section
        .lines()
        .filter_map(|line| token_after(line, "Name: ").map(String::from))
        .collect();
    let mut needs = BTreeMap::<String, Vec<String>>::new();
    let mut provider = None;
    for line in versions
        .rsplit("Version needs section")
        .next()
        .unwrap_or("")
        .lines()
    {
        if let Some(name) = token_after(line, "File: ") {
            provider = Some(name.to_owned());
            needs.insert(name.to_owned(), Vec::new());
        }
        if let (Some(provider), Some(version)) = (&provider, token_after(line, "Name: ")) {
            needs
                .entry(provider.clone())
                .or_default()
                .push(version.to_owned());
        }
    }
    let mut exports = BTreeSet::new();
    let mut imports = BTreeSet::new();
    for line in symbols.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 8 {
            continue;
        }
        let Some(number) = fields[0].strip_suffix(':') else {
            continue;
        };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let name = fields[7].replace("@@", "@");
        if fields[6] != "UND" {
            exports.insert(name.clone());
            if fields[7].contains("@@") {
                exports.insert(name.split('@').next().unwrap_or("").to_owned());
            }
        } else if fields[4] != "WEAK" {
            imports.insert(name);
        }
    }
    ElfInfo {
        dynamic,
        versions,
        symbols,
        needed,
        definitions,
        needs,
        exports,
        imports,
    }
}
/// Inspect one object using bounded native readelf argv, with no shell or Python.
/// PATH selects the configured native SDK tool; LC_ALL fixes GNU parser text.
pub fn elf(path: &Path) -> Result<ElfInfo> {
    elf_with_runner(path, &mut NativeRunner)
}
fn elf_with_runner(path: &Path, runner: &mut impl Runner) -> Result<ElfInfo> {
    let mut read = |option: &str| -> Result<String> {
        let mut command = CommandSpec::new(
            vec![
                "readelf".into(),
                "--wide".into(),
                option.into(),
                path.to_str().ok_or("ELF path is not UTF-8")?.into(),
            ],
            Duration::from_secs(30),
        );
        command.env.insert("LC_ALL".into(), "C".into());
        command.stderr_limit = 1024 * 1024;
        let result = runner.run(&command)?;
        if result.code != 0 {
            return Err(format!(
                "readelf {option} failed for {}: {}",
                path.display(),
                String::from_utf8_lossy(&result.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(result.stdout)?)
    };
    Ok(parse_readelf(
        read("-d")?,
        read("--version-info")?,
        read("--dyn-syms")?,
    ))
}
fn architecture(path: &Path, machine: u16) -> Result<()> {
    let mut header = [0; 64];
    let mut file = File::open(path)?;
    if file.read_exact(&mut header).is_err()
        || &header[..6] != b"\x7fELF\x02\x01"
        || u16::from_le_bytes([header[18], header[19]]) != machine
    {
        return Err(format!("unexpected ELF architecture: {}", path.display()).into());
    }
    Ok(())
}
fn provider(object: &Path, needed: &str, libraries: &[PathBuf]) -> Result<PathBuf> {
    let missing = || format!("missing provider {needed} required by {}", object.display());
    let mut expanded = String::new();
    let mut rest = needed;
    while let Some(index) = rest.find('$') {
        expanded.push_str(&rest[..index]);
        let token = &rest[index..];
        rest = token
            .strip_prefix("${ORIGIN}")
            .or_else(|| {
                token.strip_prefix("$ORIGIN").filter(|suffix| {
                    !suffix.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                })
            })
            .ok_or_else(|| format!("unsupported dynamic token in provider {needed}"))?;
        expanded.push_str(
            object
                .parent()
                .and_then(Path::to_str)
                .ok_or("ELF origin directory is not UTF-8")?,
        );
    }
    expanded.push_str(rest);
    if expanded.contains('/') {
        // DT_NEEDED pathnames are direct, not searches relative to each library.
        // Only ORIGIN supplies an object-relative base; a relative pathname would
        // need the runtime working directory, which this static audit cannot infer.
        let path = Path::new(&expanded);
        if !path.is_absolute() {
            return Err(format!("relative provider pathname is unsupported: {needed}").into());
        }
        let found = path
            .canonicalize()
            .map_err(|error| format!("{} (expanded {}): {error}", missing(), path.display()))?;
        if !found.is_file()
            || !libraries
                .iter()
                .filter_map(|dir| dir.canonicalize().ok())
                .any(|dir| found.parent() == Some(dir.as_path()))
        {
            return Err(
                format!("provider {needed} is outside explicit library directories").into(),
            );
        }
        return Ok(found);
    }
    libraries
        .iter()
        .map(|folder| folder.join(needed))
        .find(|p| p.is_file())
        .ok_or_else(missing)?
        .canonicalize()
        .map_err(Into::into)
}
/// Resolve explicit ordered provider directories and evaluate each root separately.
/// ORIGIN pathnames use their declaring object, within those explicit directories.
/// This does not model RPATH/other loader search policy or execute any provider.
pub fn audit(roots: &[PathBuf], libraries: &[PathBuf], machine: u16) -> Result<Value> {
    audit_in_context(roots, libraries, machine, None)
}
/// Check the owner independently first, then expose only its startup closure to
/// each plugin root. Other roots never supply exports to one another.
pub fn audit_with_load_owner(
    roots: &[PathBuf],
    libraries: &[PathBuf],
    machine: u16,
    load_owner: &Path,
) -> Result<Value> {
    audit_in_context(roots, libraries, machine, Some(load_owner))
}
fn audit_in_context(
    roots: &[PathBuf],
    libraries: &[PathBuf],
    machine: u16,
    load_owner: Option<&Path>,
) -> Result<Value> {
    if roots.is_empty() {
        return Err("at least one ELF root required".into());
    }
    let load_owner = load_owner.map(fs::canonicalize).transpose()?;
    let mut scoped_roots = roots.to_vec();
    if let Some(owner) = &load_owner {
        scoped_roots = scoped_roots
            .iter()
            .map(fs::canonicalize)
            .collect::<std::io::Result<Vec<_>>>()?;
        scoped_roots.retain(|root| root != owner);
        scoped_roots.insert(0, owner.clone());
    }
    let roots = &scoped_roots;
    let mut objects = BTreeMap::<PathBuf, ElfInfo>::new();
    let mut providers = BTreeMap::<(PathBuf, String), PathBuf>::new();
    let mut pending = roots
        .iter()
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    if load_owner.is_some() {
        pending.reverse();
    }
    while let Some(path) = pending.pop() {
        if objects.contains_key(&path) {
            continue;
        }
        architecture(&path, machine)?;
        let info = elf(&path)?;
        for needed in &info.needed {
            let found = provider(&path, needed, libraries)?;
            providers.insert((path.clone(), needed.clone()), found.clone());
            pending.push(found);
        }
        objects.insert(path, info);
    }
    let mut results = Vec::new();
    let mut owner_closure = BTreeSet::new();
    for (index, root) in roots.iter().enumerate() {
        let root = root.canonicalize()?;
        let mut closure = BTreeSet::new();
        let mut pending = vec![root.clone()];
        while let Some(path) = pending.pop() {
            if !closure.insert(path.clone()) {
                continue;
            }
            for needed in &objects[&path].needed {
                pending.push(
                    providers
                        .get(&(path.clone(), needed.clone()))
                        .ok_or_else(|| format!("unresolved provider {needed}"))?
                        .clone(),
                );
            }
        }
        if load_owner.is_some() {
            if index == 0 {
                // This owner's result cannot be rescued by any plugin exports.
                owner_closure = closure.clone();
            } else {
                closure.extend(owner_closure.iter().cloned());
            }
        }
        let exports: BTreeSet<_> = closure
            .iter()
            .flat_map(|path| objects[path].exports.iter().cloned())
            .collect();
        let mut failures = Vec::new();
        for path in &closure {
            let info = &objects[path];
            let mut missing_versions = BTreeMap::<String, Vec<String>>::new();
            for (provider, versions) in &info.needs {
                let provider_path = providers
                    .get(&(path.clone(), provider.clone()))
                    .ok_or_else(|| {
                        format!(
                            "missing version provider {provider} required by {}",
                            path.display()
                        )
                    })?;
                let definitions: BTreeSet<_> = objects[provider_path].definitions.iter().collect();
                let missing: BTreeSet<_> = versions
                    .iter()
                    .filter(|version| !definitions.contains(version))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    missing_versions.insert(provider.clone(), missing.into_iter().collect());
                }
            }
            let missing_symbols: Vec<_> = info.imports.difference(&exports).cloned().collect();
            if !missing_versions.is_empty() || !missing_symbols.is_empty() {
                failures.push(json!({"path":path,"missing_versions":missing_versions,"missing_strong_symbols":missing_symbols}));
            }
        }
        results.push(json!({"root":root,"closure":closure,"compatible":failures.is_empty(),"failures":failures}));
    }
    let mut object_rows = Vec::new();
    for (path, info) in objects {
        object_rows.push(json!({"path":path,"needed":info.needed,"sha256":digest(&path)?}));
    }
    let mut report = json!({"compatible":results.iter().all(|v|v["compatible"]==true),"roots":results,"objects":object_rows,"scope":"static ELF64 little-endian architecture, per-root DT_NEEDED closure, version and strong-symbol resolution; no hardware execution"});
    if let Some(owner) = load_owner {
        report["load_owner"] = json!(owner);
        report["scope"] = json!("static ELF64 little-endian architecture, independent load-owner startup closure plus each plugin's DT_NEEDED closure, version and strong-symbol resolution; no hardware execution");
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa_process::{CaptureError, Captured, CommandSpec};
    use std::time::Duration;

    struct ScriptTool(&'static str);
    impl Runner for ScriptTool {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            assert_eq!(spec.argv[0], "readelf");
            assert_eq!(spec.argv[1], "--wide");
            assert_eq!(spec.env.get("LC_ALL").map(String::as_str), Some("C"));
            assert_eq!(spec.timeout, Duration::from_secs(30));
            assert_eq!(spec.stdout_limit, 16 * 1024 * 1024);
            assert_eq!(spec.stderr_limit, 1024 * 1024);
            let mut owned = spec.clone();
            owned.argv = vec!["/bin/sh".into(), "-c".into(), self.0.into()];
            owned.timeout = if self.0 == "sleep 30" {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(2)
            };
            owned.stdout_limit = 1024;
            owned.stderr_limit = 1024;
            NativeRunner.run(&owned)
        }
    }
    #[test]
    fn native_readelf_timeout_and_output_limits_fail_closed() {
        for (script, expected) in [
            ("sleep 30", "timeout"),
            ("while :; do printf '0123456789abcdef'; done", "stdout"),
            ("while :; do printf '0123456789abcdef' >&2; done", "stderr"),
        ] {
            let error =
                elf_with_runner(Path::new("fixture.elf"), &mut ScriptTool(script)).unwrap_err();
            match (expected, error.downcast_ref::<CaptureError>()) {
                ("timeout", Some(CaptureError::Timeout { .. })) => {}
                (stream, Some(CaptureError::Limit { stream: actual, .. })) => {
                    assert_eq!(stream, *actual)
                }
                _ => panic!("unexpected inspection failure: {error}"),
            }
        }
    }
    #[test]
    fn origin_paths_require_exact_tokens_and_explicit_provider_directories() {
        let temp = tempfile::tempdir().unwrap();
        let lib = temp.path().join("lib");
        fs::create_dir(&lib).unwrap();
        let object = lib.join("consumer.so");
        let target = lib.join("libprovider.so");
        fs::write(&target, b"provider").unwrap();
        let libraries = [lib];
        for needed in [
            "$ORIGIN/../lib/libprovider.so",
            "${ORIGIN}/../lib/libprovider.so",
        ] {
            assert_eq!(
                provider(&object, needed, &libraries).unwrap(),
                target.canonicalize().unwrap()
            );
            assert!(provider(&object, needed, &[]).is_err());
        }
        for (needed, expected) in [
            ("$ORIGIN_SUFFIX/libprovider.so", "unsupported dynamic token"),
            ("${ORIGIN/libprovider.so", "unsupported dynamic token"),
            ("$LIB/libprovider.so", "unsupported dynamic token"),
            ("../lib/libprovider.so", "relative provider pathname"),
        ] {
            let error = provider(&object, needed, &libraries).unwrap_err();
            assert!(error.to_string().contains(expected), "{needed}: {error}");
        }
        let outside = temp.path().join("outside.so");
        fs::write(&outside, b"outside").unwrap();
        assert!(provider(&object, "$ORIGIN/../outside.so", &libraries).is_err());
        assert_eq!(
            provider(&object, target.to_str().unwrap(), &libraries).unwrap(),
            target.canonicalize().unwrap()
        );
        assert!(provider(&object, outside.to_str().unwrap(), &libraries).is_err());
    }
    #[test]
    fn malformed_elf_and_invalid_readelf_text_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("malformed");
        fs::write(&file, b"not ELF").unwrap();
        assert!(elf(&file).unwrap_err().to_string().contains("readelf"));
        let error = elf_with_runner(&file, &mut ScriptTool("printf '\\377'")).unwrap_err();
        assert!(
            error.downcast_ref::<std::string::FromUtf8Error>().is_some(),
            "{error}"
        );
    }
    #[test]
    fn parser_preserves_default_version_aliases_and_excludes_weak_imports() {
        let info=parse_readelf("0x1 (NEEDED) Shared library: [libfixture.so]\n".into(),"Version definition section '.gnu.version_d' contains 2 entries:\n  Name: FIXTURE_1\nVersion needs section '.gnu.version_r' contains 1 entry:\n  File: libfixture.so Cnt: 1\n  Name: PROVIDER_1 Flags: none\n".into(),"Symbol table:\n 1: 000 0 FUNC GLOBAL DEFAULT 11 owned@@FIXTURE_1\n 2: 000 0 FUNC GLOBAL DEFAULT 11 old@FIXTURE_0\n 3: 000 0 FUNC GLOBAL DEFAULT UND needed@PROVIDER_1 (2)\n 4: 000 0 FUNC WEAK DEFAULT UND optional\n 5: 000 0 FUNC GLOBAL DEFAULT UND unversioned\n".into());
        assert_eq!(info.needed, ["libfixture.so"]);
        assert_eq!(info.definitions, ["FIXTURE_1"]);
        assert_eq!(info.needs["libfixture.so"], ["PROVIDER_1"]);
        assert!(info.exports.contains("owned@FIXTURE_1"));
        assert!(info.exports.contains("owned"));
        assert!(info.exports.contains("old@FIXTURE_0"));
        assert!(!info.exports.contains("old"));
        assert_eq!(
            info.imports,
            BTreeSet::from(["needed@PROVIDER_1".into(), "unversioned".into()])
        );
    }
}
