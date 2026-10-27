//! Actions engineering entry: build the actual complete catalog, seal it with
//! source-built tools and verify its retained generation. Never activate it.
use openpilot_runtime_build::{catalog, generation, prepare, runtime_layout, Result};
use std::{collections::BTreeMap, fs, path::PathBuf};

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
            let mut rebuilt_groups = Vec::new();
            for (name, report) in &prepared.reports {
                let rebuilt = report.artifacts.iter().any(|a| !a.fresh);
                let allowed = kind == "small-logger"
                    && prepared.plan.groups[name]
                        .binaries
                        .iter()
                        .any(|b| b == "openpilot-loggerd");
                if rebuilt && !allowed {
                    return Err(format!("unexpected rebuild in {kind}: group {name}").into());
                }
                if rebuilt {
                    rebuilt_groups.push(name.clone());
                }
                if !allowed {
                    for binary in &prepared.plan.groups[name].binaries {
                        if prepared.binaries.get(binary) != previous.binaries.get(binary) {
                            return Err(
                                format!("unaffected binary changed in {kind}: {binary}").into()
                            );
                        }
                    }
                }
            }
            if kind == "small-logger" && rebuilt_groups.is_empty() {
                return Err("small logger source change did not rebuild its group".into());
            }
            if kind == "small-logger" {
                if prepared.binaries.get("openpilot-loggerd")
                    == previous.binaries.get("openpilot-loggerd")
                {
                    return Err("changed logger help did not change its actual binary".into());
                }
                let binary = prepared
                    .reports
                    .values()
                    .flat_map(|r| &r.artifacts)
                    .find(|a| {
                        a.executable
                            .file_name()
                            .is_some_and(|n| n == "openpilot-loggerd")
                    })
                    .ok_or("actual logger executable is absent")?;
                let help = std::process::Command::new(&binary.executable)
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
            if kind != "small-logger" && prepared.stored_output_bytes != 0 {
                return Err("unchanged native build stored new binary data".into());
            }
            let result = serde_json::json!({"schema":1,"source":prepared.source.source,"reuse_kind":kind,
                "compiler_groups":13,"binaries":99,"rebuilt_groups":rebuilt_groups,
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
