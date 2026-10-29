//! Actions engineering entry: build the actual complete catalog, seal it with
//! source-built tools and verify its retained generation. Never activate it.
use openpilot_runtime_build::{catalog, generation, prepare, runtime_layout, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

// Normal/target path-dependency closure: loggerd -> bootlog -> manager.
// loggerd's src build-script trigger refreshes its Git stamp; bootlog embeds it
// through Environment::init_data. Manager can relink to an identical final ELF.
const LOGGER_CONSUMERS: [(&str, &str); 3] = [
    ("07", "openpilot-loggerd"),
    ("08", "openpilot-bootlog"),
    ("09", "openpilot-manager"),
];
const REQUIRED_CHANGES: [&str; 2] = ["openpilot-loggerd", "openpilot-bootlog"];

fn verify_reuse(
    prepared: &prepare::Prepared,
    previous: &prepare::Prepared,
    kind: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    let small = kind == "small-logger";
    if !["repeat", "docs", "small-logger"].contains(&kind)
        || prepared.binaries.keys().ne(previous.binaries.keys())
        || prepared.reports.keys().ne(previous.reports.keys())
    {
        return Err("reuse kind or previous build roster differs".into());
    }
    let mut rebuilt_groups = Vec::new();
    for (name, report) in &prepared.reports {
        let expected: BTreeSet<_> = prepared.plan.groups[name]
            .binaries
            .iter()
            .map(String::as_str)
            .collect();
        let actual: BTreeSet<_> = report
            .artifacts
            .iter()
            .filter_map(|a| a.executable.file_name()?.to_str())
            .collect();
        if expected != actual || actual.len() != report.artifacts.len() {
            return Err(format!("Cargo binary roster differs: {name}").into());
        }
        for artifact in &report.artifacts {
            let binary = artifact
                .executable
                .file_name()
                .and_then(|p| p.to_str())
                .ok_or("invalid Cargo binary name")?;
            let affected = small && LOGGER_CONSUMERS.contains(&(name.as_str(), binary));
            if artifact.fresh == affected {
                return Err(format!("unexpected Cargo freshness in {kind}: {binary}").into());
            }
        }
        if report.artifacts.iter().any(|a| !a.fresh) {
            rebuilt_groups.push(name.clone());
        }
    }
    for (group, binary) in LOGGER_CONSUMERS {
        if !prepared
            .plan
            .groups
            .get(group)
            .is_some_and(|g| g.binaries.iter().any(|b| b == binary))
        {
            return Err("logger dependency consumers differ from the actual catalog plan".into());
        }
    }
    let expected_groups: Vec<_> = if small {
        LOGGER_CONSUMERS
            .iter()
            .map(|(group, _)| (*group).to_owned())
            .collect()
    } else {
        Vec::new()
    };
    if rebuilt_groups != expected_groups {
        return Err(format!("unexpected rebuilt groups in {kind}: {rebuilt_groups:?}").into());
    }
    let mut changed_binaries = Vec::new();
    for (binary, object) in &prepared.binaries {
        let changed = previous.binaries.get(binary) != Some(object);
        let affected = small
            && LOGGER_CONSUMERS
                .iter()
                .any(|(_, name)| *name == binary.as_str());
        if (changed && !affected)
            || (small && REQUIRED_CHANGES.contains(&binary.as_str()) && !changed)
        {
            return Err(format!("unexpected binary change in {kind}: {binary}").into());
        }
        if changed {
            changed_binaries.push(binary.clone());
        }
    }
    if !small && prepared.stored_output_bytes != 0 {
        return Err("unchanged native build stored new binary data".into());
    }
    Ok((rebuilt_groups, changed_binaries))
}

fn logger_help_command(prepared: &prepare::Prepared) -> Result<Command> {
    let report = prepared
        .reports
        .get("07")
        .ok_or("logger Cargo group missing")?;
    let binary = report
        .artifacts
        .iter()
        .find(|a| {
            a.executable
                .file_name()
                .is_some_and(|n| n == "openpilot-loggerd")
        })
        .ok_or("actual logger executable is absent")?;
    // Use the original compiler-provider loader contract, not a sealed layout or
    // an unverified inherited LD_LIBRARY_PATH. catalog_bindings pins these trees.
    let loader = prepared.plan.groups["07"]
        .environment
        .get("LD_LIBRARY_PATH")
        .filter(|s| !s.is_empty())
        .ok_or("logger recipe lacks its provider loader path")?;
    for path in std::env::split_paths(loader) {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
            || !report
                .verified_input_trees
                .keys()
                .any(|root| root.as_path() != Path::new("/") && path.starts_with(root))
        {
            return Err(
                "logger loader path is outside its verified compiler-provider trees".into(),
            );
        }
    }
    let mut command = Command::new(&binary.executable);
    command.env("LD_LIBRARY_PATH", loader);
    Ok(command)
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut paths = BTreeMap::new();
    while let Some(arg) = args.next() {
        let key = arg.into_string().map_err(|_| "invalid argument")?;
        if ![
            "--root",
            "--plan",
            "--layout",
            "--cache",
            "--toolchain",
            "--stores",
            "--evidence",
            "--previous",
            "--reuse-kind",
        ]
        .contains(&key.as_str())
            || paths.contains_key(&key)
        {
            return Err("unknown or duplicate full-generation argument".into());
        }
        paths.insert(key, PathBuf::from(args.next().ok_or("missing path")?));
    }
    let path = |key: &str| paths.get(key).ok_or_else(|| format!("{key} is required"));
    let root = path("--root")?;
    let base = path("--stores")?;
    let evidence = path("--evidence")?;
    fs::create_dir(evidence)?;
    let stores = generation::Stores {
        sources: base.join("sources"),
        objects: base.join("objects"),
        generations: base.join("generations"),
        params: base.join("params"),
    };
    generation::offroad(&stores.params)?;
    let plan = prepare::Plan::read(path("--plan")?)?;
    let layout: generation::Layout = serde_json::from_reader(fs::File::open(path("--layout")?)?)?;
    catalog::verify_source_plan(root, &plan)?;
    runtime_layout::verify(root, &layout.provider_root, &layout.files)?;
    let prepared = prepare::run(
        root,
        path("--cache")?,
        path("--toolchain")?,
        &stores.sources,
        &stores.objects,
        &plan,
    )?;
    if prepared.binaries.len() != 99 || prepared.reports.len() != 13 {
        return Err("full native catalog did not produce all 99 binaries in 13 groups".into());
    }
    fs::write(
        evidence.join("prepared.json"),
        serde_json::to_vec_pretty(&prepared)?,
    )?;
    match (paths.get("--previous"), paths.get("--reuse-kind")) {
        (Some(previous), Some(kind)) => {
            let kind = kind.to_str().ok_or("invalid reuse kind")?;
            if !["repeat", "docs", "small-logger"].contains(&kind) {
                return Err("invalid reuse kind".into());
            }
            let previous: prepare::Prepared = serde_json::from_reader(fs::File::open(previous)?)?;
            let (rebuilt_groups, changed_binaries) = verify_reuse(&prepared, &previous, kind)?;
            if kind == "small-logger" {
                let help = logger_help_command(&prepared)?
                    .env("LOG_ROOT", evidence.join("help-logs"))
                    .env("PARAMS_ROOT", &stores.params)
                    .env(
                        "OPENPILOT_PREFIX",
                        format!("native-complete-{}", std::process::id()),
                    )
                    .arg("--help")
                    .output()?;
                if !help.status.success()
                    || !String::from_utf8_lossy(&help.stdout)
                        .contains("Continuous native route logger (owned model cache fixture).")
                {
                    return Err("actual logger help does not match the owned source change".into());
                }
            }
            let result = serde_json::json!({"schema":1,"source":prepared.source.source,"reuse_kind":kind,
                "compiler_groups":13,"binaries":99,"rebuilt_groups":rebuilt_groups,
                "changed_binaries":changed_binaries,
                "stored_output_bytes":prepared.stored_output_bytes,"runtime_activated":false,
                "runtime_started":false,"physical_device_execution":false});
            fs::write(
                evidence.join("reuse.json"),
                serde_json::to_vec_pretty(&result)?,
            )?;
            println!("{}", result);
            return Ok(());
        }
        (None, None) => (),
        _ => return Err("reuse evidence requires both --previous and --reuse-kind".into()),
    }
    let id = generation::seal(root, &stores, prepared, layout.clone())?;
    let generation = generation::load(&stores, &id)?;
    if generation.runtime.source != generation.prepared.source.source {
        return Err("sealed runtime source identity differs".into());
    }
    // Only this job's newly composed staging tree is hidden. Original provider
    // and compiler caches are preserved; no activation or vehicle connection.
    let hidden = layout
        .provider_root
        .with_file_name("tree-hidden-for-generation-verification");
    fs::rename(&layout.provider_root, &hidden)?;
    let without_staging_cache = generation::load(&stores, &id);
    fs::rename(&hidden, &layout.provider_root)?;
    without_staging_cache?;
    let result = serde_json::json!({"schema":1,"source":generation.prepared.source.source,
        "generation_id":id,"compiler_groups":13,"binaries":99,"sealed_files":generation.files.len(),
        "source_built_sealer_and_abi_audit_passed":true,"source_generation_verified":true,
        "composed_staging_cache_absence_verified":true,"runtime_activated":false,"runtime_started":false,
        "physical_device_execution":false});
    fs::write(
        evidence.join("generation.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", result);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("full native generation: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_runtime_build::{objects, source_store, Artifact, Recipe, Report, Source};

    fn fixture() -> prepare::Prepared {
        let catalog: catalog::Catalog =
            serde_json::from_str(include_str!("../../../native-runtime-catalog.json")).unwrap();
        let source = Source {
            commit: "a".repeat(40),
            tree: "b".repeat(40),
        };
        let mut prepared = prepare::Prepared {
            schema: 1,
            plan: prepare::Plan {
                schema: 1,
                groups: BTreeMap::new(),
                expected_source_growth_bytes: 1,
                remaining_object_growth_bytes: 1,
                runtime_inputs: prepare::RuntimeInputs::default(),
            },
            source: source_store::Snapshot {
                schema: 1,
                source: source.clone(),
                root: "/owned/source".into(),
                modules: BTreeMap::new(),
            },
            reports: BTreeMap::new(),
            binaries: BTreeMap::new(),
            stored_output_bytes: 0,
            elapsed_ms: 1,
            verified_runtime_trees: BTreeMap::new(),
        };
        for (name, group) in &catalog.groups {
            let artifacts = group
                .binaries
                .iter()
                .map(|binary| {
                    prepared.binaries.insert(
                        binary.clone(),
                        objects::Object {
                            sha256: "a".repeat(64),
                            bytes: 1,
                            mode: 0o555,
                        },
                    );
                    Artifact {
                        executable: Path::new("/owned/target").join(binary),
                        fresh: true,
                        sha256: "a".repeat(64),
                        bytes: 1,
                    }
                })
                .collect();
            prepared.plan.groups.insert(
                name.clone(),
                Recipe {
                    schema: 1,
                    target: catalog.target.clone(),
                    packages: group.packages.clone(),
                    binaries: group.binaries.clone(),
                    features: group.features.clone(),
                    inputs: BTreeMap::new(),
                    input_trees: BTreeMap::new(),
                    environment: BTreeMap::new(),
                    expected_growth_bytes: 1,
                },
            );
            prepared.reports.insert(
                name.clone(),
                Report {
                    schema: 3,
                    cache_key: "owned".into(),
                    target_directory: "/owned/target".into(),
                    artifacts,
                    source: source.clone(),
                    recipe_sha256: "a".repeat(64),
                    rustc_sha256: "a".repeat(64),
                    cargo_sha256: "a".repeat(64),
                    toolchain_directory: "/owned/toolchain".into(),
                    native_tools: BTreeMap::new(),
                    verified_input_trees: BTreeMap::new(),
                    elapsed_ms: 1,
                },
            );
        }
        prepared
            .plan
            .groups
            .get_mut("07")
            .unwrap()
            .environment
            .insert(
                "LD_LIBRARY_PATH".into(),
                "/owned/ffmpeg/lib:/owned/capnp/usr/lib/aarch64-linux-gnu".into(),
            );
        prepared
            .reports
            .get_mut("07")
            .unwrap()
            .verified_input_trees
            .extend([
                (PathBuf::from("/owned/ffmpeg"), "a".repeat(64)),
                (PathBuf::from("/owned/capnp"), "a".repeat(64)),
                (PathBuf::from("/"), "a".repeat(64)),
            ]);
        prepared
    }

    fn change(prepared: &mut prepare::Prepared, binary: &str, sha: &str) {
        prepared.binaries.get_mut(binary).unwrap().sha256 = sha.into();
        for artifact in prepared.reports.values_mut().flat_map(|r| &mut r.artifacts) {
            if artifact.executable.file_name().is_some_and(|n| n == binary) {
                artifact.sha256 = sha.into();
            }
        }
    }

    fn small(previous: &prepare::Prepared) -> prepare::Prepared {
        let mut prepared = previous.clone();
        for (group, binary) in LOGGER_CONSUMERS {
            prepared
                .reports
                .get_mut(group)
                .unwrap()
                .artifacts
                .iter_mut()
                .find(|a| a.executable.file_name().is_some_and(|n| n == binary))
                .unwrap()
                .fresh = false;
        }
        for binary in REQUIRED_CHANGES {
            change(&mut prepared, binary, &"b".repeat(64));
        }
        prepared
    }

    #[test]
    fn manager_recompile_can_keep_its_elf_but_logger_and_bootlog_must_change() {
        let previous = fixture();
        assert_eq!(previous.binaries.len(), 99);
        assert_eq!(previous.reports.len(), 13);
        let mut prepared = small(&previous);
        let (groups, binaries) = verify_reuse(&prepared, &previous, "small-logger").unwrap();
        assert_eq!(groups, ["07", "08", "09"]);
        assert_eq!(binaries, ["openpilot-bootlog", "openpilot-loggerd"]);
        assert_eq!(
            prepared.binaries["openpilot-manager"],
            previous.binaries["openpilot-manager"]
        );
        change(&mut prepared, "openpilot-manager", &"c".repeat(64));
        assert_eq!(
            verify_reuse(&prepared, &previous, "small-logger")
                .unwrap()
                .1
                .len(),
            3
        );
        for binary in REQUIRED_CHANGES {
            let mut missing = small(&previous);
            change(&mut missing, binary, &previous.binaries[binary].sha256);
            assert!(verify_reuse(&missing, &previous, "small-logger").is_err());
        }
        for (group, binary) in LOGGER_CONSUMERS {
            let mut missing = small(&previous);
            missing
                .reports
                .get_mut(group)
                .unwrap()
                .artifacts
                .iter_mut()
                .find(|a| a.executable.file_name().is_some_and(|n| n == binary))
                .unwrap()
                .fresh = true;
            assert!(verify_reuse(&missing, &previous, "small-logger").is_err());
        }
    }

    #[test]
    fn unrelated_binaries_stay_fresh_and_identical_even_inside_affected_groups() {
        let previous = fixture();
        for group in ["00", "07", "08", "09"] {
            let binary = previous.plan.groups[group]
                .binaries
                .iter()
                .find(|b| !LOGGER_CONSUMERS.iter().any(|(_, name)| *name == b.as_str()))
                .unwrap();
            let mut changed = small(&previous);
            change(&mut changed, binary, &"c".repeat(64));
            assert!(verify_reuse(&changed, &previous, "small-logger").is_err());
            let mut rebuilt = small(&previous);
            rebuilt
                .reports
                .get_mut(group)
                .unwrap()
                .artifacts
                .iter_mut()
                .find(|a| {
                    a.executable
                        .file_name()
                        .is_some_and(|n| n == binary.as_str())
                })
                .unwrap()
                .fresh = false;
            assert!(verify_reuse(&rebuilt, &previous, "small-logger").is_err());
        }
        for kind in ["repeat", "docs"] {
            verify_reuse(&previous, &previous, kind).unwrap();
            assert!(verify_reuse(&small(&previous), &previous, kind).is_err());
        }
    }

    #[test]
    fn logger_loader_uses_only_its_original_verified_provider_environment() {
        let mut prepared = fixture();
        let command = logger_help_command(&prepared).unwrap();
        assert!(command
            .get_envs()
            .any(|(key, value)| key == "LD_LIBRARY_PATH"
                && value
                    == Some(std::ffi::OsStr::new(
                        "/owned/ffmpeg/lib:/owned/capnp/usr/lib/aarch64-linux-gnu"
                    ))));
        for invalid in [
            "",
            "/unverified/lib",
            "/owned/ffmpeg/../unverified",
            "relative",
        ] {
            prepared
                .plan
                .groups
                .get_mut("07")
                .unwrap()
                .environment
                .insert("LD_LIBRARY_PATH".into(), invalid.into());
            assert!(logger_help_command(&prepared).is_err());
        }
        prepared
            .plan
            .groups
            .get_mut("07")
            .unwrap()
            .environment
            .remove("LD_LIBRARY_PATH");
        assert!(logger_help_command(&prepared).is_err());
    }

    #[cfg(feature = "prepare-cargo")]
    #[test]
    fn normal_and_target_path_dependency_closure_matches_the_actual_catalog_consumers() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let manifests: BTreeMap<_, toml::Value> = fs::read_dir(crates)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.join("Cargo.toml").is_file())
            .map(|p| {
                let manifest =
                    toml::from_str(&fs::read_to_string(p.join("Cargo.toml")).unwrap()).unwrap();
                (p.canonicalize().unwrap(), manifest)
            })
            .collect();
        let mut reverse: BTreeMap<String, BTreeSet<(String, bool)>> = BTreeMap::new();
        for (path, manifest) in &manifests {
            let mut sections = vec![manifest.get("dependencies")];
            if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
                sections.extend(targets.values().map(|v| v.get("dependencies")));
            }
            for dependency in sections
                .into_iter()
                .flatten()
                .filter_map(toml::Value::as_table)
                .flat_map(|t| t.values())
            {
                if let Some(relative) = dependency.get("path").and_then(toml::Value::as_str) {
                    let origin = path.join(relative).canonicalize().unwrap();
                    if let Some(origin) = manifests.get(&origin) {
                        reverse
                            .entry(origin["package"]["name"].as_str().unwrap().into())
                            .or_default()
                            .insert((
                                manifest["package"]["name"].as_str().unwrap().into(),
                                dependency
                                    .get("optional")
                                    .and_then(toml::Value::as_bool)
                                    .unwrap_or(false),
                            ));
                    }
                }
            }
        }
        let mut closure = BTreeSet::from(["openpilot-loggerd".to_owned()]);
        loop {
            let consumers: BTreeSet<_> = closure
                .iter()
                .filter_map(|name| reverse.get(name))
                .flatten()
                .cloned()
                .collect();
            assert!(consumers.iter().all(|(_, optional)| !optional));
            let mut expanded = closure.clone();
            expanded.extend(consumers.into_iter().map(|(name, _)| name));
            if expanded == closure {
                break;
            }
            closure = expanded;
        }
        assert_eq!(
            closure,
            LOGGER_CONSUMERS
                .iter()
                .map(|(_, name)| (*name).to_owned())
                .collect::<BTreeSet<_>>()
        );
        let prepared = fixture();
        for (group, binary) in LOGGER_CONSUMERS {
            assert!(prepared.plan.groups[group]
                .binaries
                .iter()
                .any(|name| name == binary));
        }
    }
}
