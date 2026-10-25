//! Dedicated native replacements for the diagnostics and log-pipeline controllers.
//! Shared lifecycle/input/diagnostic validation stays owned by loggerd_check.
use super::{owned, peer, providers, request, require, save, validation, Request, Result, WORKER};
use crate::argparse_scan::{scan_os_required_first, OptionSpec};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};
mod diagnostics;
mod pipeline;
mod source_pins;
struct HeldSource {
    path: PathBuf,
    file: File,
    device: u64,
    inode: u64,
    digest: String,
}
struct Sources {
    held: Vec<HeldSource>,
    output: PathBuf,
}
impl Sources {
    fn new(repository: &Path, output: &Path) -> Result<Self> {
        let mut held = Vec::new();
        for &(name, expected) in source_pins::PINS {
            let path = repository.join(name);
            let file = File::from(rustix::fs::open(
                &path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )?);
            let metadata = file.metadata()?;
            require(
                metadata.is_file() && metadata.len() <= 1 << 20,
                "original source is not a bounded regular file",
            )?;
            let digest = source_digest(&file)?;
            require(
                digest == expected,
                format!("reviewed original source changed: {name}"),
            )?;
            held.push(HeldSource {
                path,
                file,
                device: metadata.dev(),
                inode: metadata.ino(),
                digest,
            });
        }
        let sources = Self {
            held,
            output: output.into(),
        };
        save(&output.join("sources.before.json"), &sources.snapshot()?)?;
        Ok(sources)
    }
    fn snapshot(&self) -> Result<Value> {
        let mut rows = Vec::new();
        for source in &self.held {
            let current = fs::symlink_metadata(&source.path)?;
            require(
                current.is_file()
                    && !current.file_type().is_symlink()
                    && (current.dev(), current.ino()) == (source.device, source.inode),
                "original source path identity changed",
            )?;
            require(
                source_digest(&source.file)? == source.digest
                    && crate::seal::digest(&source.path)? == source.digest,
                "original source bytes changed",
            )?;
            rows.push(json!({"path":source.path,"device":source.device,"inode":source.inode,"sha256":source.digest}));
        }
        Ok(json!(rows))
    }
    fn verify(&self) -> Result<()> {
        save(&self.output.join("sources.after.json"), &self.snapshot()?)
    }
}
fn source_digest(file: &File) -> Result<String> {
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take((1 << 20) + 1).read_to_end(&mut bytes)?;
    require(bytes.len() <= 1 << 20, "source grew beyond bound")?;
    Ok(super::hash(&bytes))
}
pub(super) fn worker(request: &Request) -> Result<Value> {
    match request.kind.as_str() {
        "pipeline" => pipeline::drive(request),
        "diagnostic-throughput" | "diagnostic-suppression" => diagnostics::drive(request),
        _ => Err("unknown logger controller worker kind".into()),
    }
}
/// Dedicated single-threaded process entry point; never call from a test thread.
pub fn diagnostics_cli() -> Result<()> {
    cli(false)
}
/// Actual independent source collector and native collector; never Python fallback.
pub fn pipeline_cli() -> Result<()> {
    cli(true)
}
fn cli(pipeline: bool) -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if let Some(result) = self::pipeline::source_worker(&args) {
        return result;
    }
    if args.len() == 2 && args[0] == WORKER {
        let request: Request = serde_json::from_slice(&fs::read(&args[1])?)?;
        let result = worker(&request)?;
        return save(&request.output.join("worker-result.json"), &result);
    }
    let mut specs = vec![
        OptionSpec::value("--binary", true),
        OptionSpec::value("--original", true),
        OptionSpec::value("--output", true),
        OptionSpec::value("--root", false),
    ];
    if pipeline {
        specs.extend([
            OptionSpec::value("--collector", true),
            OptionSpec::value("--fixture", true),
        ]);
    } else {
        specs.push(OptionSpec::value("--fixtures", true));
    }
    let parsed = scan_os_required_first(&args, &specs)?;
    if parsed.help {
        println!(
            "{} --binary BINARY --original ORIGINAL --output OUTPUT {} [--root ROOT]",
            if pipeline {
                "check-loggerd-log-pipeline"
            } else {
                "check-loggerd-diagnostics"
            },
            if pipeline {
                "--collector COLLECTOR --fixture FIXTURE"
            } else {
                "--fixtures FIXTURES"
            }
        );
        return Ok(());
    }
    let binary = PathBuf::from(&parsed.values["--binary"]).canonicalize()?;
    let original = PathBuf::from(&parsed.values["--original"]).canonicalize()?;
    let output = std::path::absolute(Path::new(&parsed.values["--output"]))?;
    let root = parsed
        .values
        .get("--root")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        )
        .canonicalize()?;
    let fixtures = PathBuf::from(&parsed.values[if pipeline { "--fixture" } else { "--fixtures" }])
        .canonicalize()?;
    let collector = parsed
        .values
        .get("--collector")
        .map(PathBuf::from)
        .map(|v| v.canonicalize())
        .transpose()?;
    fs::create_dir_all(output.parent().ok_or("controller output parent absent")?)?;
    fs::DirBuilder::new().mode(0o700).create(&output)?;
    let sources = Sources::new(&root, &output)?;
    let collector_sources = if pipeline {
        Some(crate::managed_entry_collector_source::sources(&root)?)
    } else {
        None
    };
    save(
        &output.join("collector-sources.before.json"),
        &json!(collector_sources),
    )?;
    let mut paths = vec![binary.as_path(), original.as_path()];
    if let Some(collector) = &collector {
        paths.push(collector.as_path());
    }
    let mut providers = providers::Providers::new(&paths, &output)?;
    let mut owner = owned::Owner::new_for(
        &output.join("control"),
        if pipeline {
            "check-loggerd-log-pipeline"
        } else {
            "check-loggerd-diagnostics"
        },
    )?;
    let mut report = json!({});
    let result = (|| -> Result<()> {
        for (side, binary) in [("original", &original), ("rust", &binary)] {
            if let Some(collector) = &collector {
                let collector = collector
                    .to_str()
                    .ok_or("collector path not UTF-8")?
                    .to_owned();
                let req = request(
                    "pipeline",
                    side,
                    binary,
                    &output.join(side),
                    &fixtures,
                    &[collector],
                );
                report[side] = owner.run(&[req], Duration::from_secs(90))?.remove(0);
            } else {
                for (name, kind) in [
                    ("throughput", "diagnostic-throughput"),
                    ("suppression", "diagnostic-suppression"),
                ] {
                    let target = output.join(format!("{name}-{side}"));
                    let req = request(kind, name, binary, &target, &fixtures, &[]);
                    owner.run(&[req], Duration::from_secs(300))?;
                    let checked =
                        diagnostics::verify(&root, &target, side == "rust", name == "throughput")?;
                    if name == "throughput" {
                        report[side] = checked;
                    } else {
                        save(&target.join("validation.json"), &checked)?;
                    }
                }
            }
            save(&output.join("report.partial.json"), &report)?;
        }
        if !pipeline {
            require(
                report["original"]["logged_bytes_at_report"]
                    == report["rust"]["logged_bytes_at_report"],
                "original/native actual logged-byte count differs",
            )?;
            report["suppression"] = validation::diagnostics(
                &root,
                &peer::Evidence::load(&output.join("suppression-original"), true)?,
                &peer::Evidence::load(&output.join("suppression-rust"), true)?,
            )?;
        }
        Ok(())
    })();
    // Attempt all independent seals/cleanup even when traffic or comparison fails.
    let cleanup = owner.finish();
    let provider_result = providers.verify();
    let source_result = sources.verify();
    let collector_after = if pipeline {
        Some(crate::managed_entry_collector_source::sources(&root)?)
    } else {
        None
    };
    save(
        &output.join("collector-sources.after.json"),
        &json!(collector_after),
    )?;
    require(
        collector_sources == collector_after,
        "original collector source closure changed",
    )?;
    result?;
    cleanup?;
    provider_result?;
    source_result?;
    report["result"] = json!("pass");
    save(&output.join("report.json"), &report)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(test)]
mod original_replay;
