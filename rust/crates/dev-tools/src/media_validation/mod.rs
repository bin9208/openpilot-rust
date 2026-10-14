//! Native target-provider proofs. Static ELF checks and QEMU execution stay distinct.
mod dynamic;
mod ffmpeg;
mod graphics;
mod sdl;
use crate::{
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
pub use dynamic::{dynamic, DynamicOptions};
pub use ffmpeg::{ffmpeg, FfmpegOptions};
pub use graphics::{graphics, GraphicsOptions};
pub use sdl::{sdl, SdlOptions, SDL_TTF_SHA256, SDL_WHEEL_SHA256};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn text(path: &Path) -> Result<String> {
    Ok(path.to_str().ok_or("non-UTF8 native QA path")?.into())
}
fn write_json(path: &Path, v: &Value) -> Result<()> {
    let mut file = File::create(path)?;
    serde_json::to_writer_pretty(&mut file, v)?;
    file.write_all(b"\n")?;
    Ok(())
}
fn report(output: &Path, v: &Value) -> Result<()> {
    write_json(&output.join("report.json"), v)
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/fixtures")
        .join(name)
}
fn provider_prefix(qemu: &Path, loader: &Path, libraries: &[PathBuf]) -> Result<Vec<String>> {
    Ok(vec![
        text(qemu)?,
        text(loader)?,
        "--library-path".into(),
        libraries
            .iter()
            .map(|p| text(p))
            .collect::<Result<Vec<_>>>()?
            .join(":"),
    ])
}
fn symbols(source: &str, pattern: &str) -> Result<BTreeSet<String>> {
    let re = regex::Regex::new(pattern)?;
    Ok(re.captures_iter(source).map(|c| c[1].to_owned()).collect())
}
fn stdout_text(output: &Captured) -> Result<String> {
    Ok(String::from_utf8(output.stdout.clone())?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn stderr_text(output: &Captured) -> Result<String> {
    Ok(String::from_utf8(output.stderr.clone())?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn checked(output: &Captured, name: &str) -> Result<()> {
    require(
        output.code == 0,
        format!("native {name} failed with exit code {}", output.code),
    )
}
fn build_probe(
    cc: &Path,
    source: &Path,
    output: &Path,
    includes: &[PathBuf],
    runner: &mut impl Runner,
) -> Result<Vec<String>> {
    crate::package::disk_guard(output, 1024 * 1024 * 1024)?;
    let mut args = vec![
        text(cc)?,
        "-std=c11".into(),
        "-Wall".into(),
        "-Wextra".into(),
        "-Werror".into(),
    ];
    args.extend(
        includes
            .iter()
            .map(|p| text(p).map(|s| format!("-I{s}")))
            .collect::<Result<Vec<_>>>()?,
    );
    args.extend([text(source)?, "-ldl".into(), "-o".into(), text(output)?]);
    let result = runner.run(&CommandSpec::new(args.clone(), Duration::from_secs(120)))?;
    let parent = output.parent().ok_or("missing build output parent")?;
    let log = stdout_text(&result)? + &stderr_text(&result)?;
    fs::write(parent.join("build.log"), log)?;
    checked(&result, "probe compile")?;
    Ok(args)
}
pub fn cli(family: &str) -> Result<()> {
    let allowed: &[&str] = match family {
        "dynamic" => &["root", "sdk", "agnos", "qemu", "probe", "output"],
        "graphics" => &[
            "source-root",
            "cc",
            "qemu",
            "loader",
            "plugin",
            "sdk",
            "agnos-libraries",
            "output",
        ],
        "ffmpeg" => &["qemu", "loader", "prefix", "library", "output"],
        "sdl" => &[
            "wheel",
            "ttf-source",
            "cc",
            "sdk",
            "qemu",
            "loader",
            "agnos-libraries",
            "output",
            "source-oracle",
        ],
        _ => return Err("unknown native media family".into()),
    };
    let mut values = BTreeMap::<String, Vec<PathBuf>>::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let key = arg.to_str().ok_or("invalid argument")?;
        if ["--help", "-h"].contains(&key) {
            println!(
                "Native {family} provider validation. Options: {}",
                allowed
                    .iter()
                    .map(|s| format!("--{s}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            return Ok(());
        }
        let key = key
            .strip_prefix("--")
            .filter(|k| allowed.contains(k))
            .ok_or("unknown native media option")?;
        let value = args.next().ok_or("missing option value")?;
        let entry = values.entry(key.into()).or_default();
        require(
            key == "library" || entry.is_empty(),
            "duplicate native media option",
        )?;
        entry.push(value.into());
    }
    let get = |name: &str| -> Result<PathBuf> {
        values
            .get(name)
            .and_then(|v| v.first())
            .cloned()
            .ok_or_else(|| format!("--{name} required").into())
    };
    let mut runner = NativeRunner;
    let value = match family {
        "dynamic" => dynamic(
            &DynamicOptions {
                root: get("root")?,
                sdk: get("sdk")?,
                agnos: get("agnos")?,
                qemu: get("qemu")?,
                probe: get("probe")?,
                output: get("output")?,
            },
            &mut runner,
        )?,
        "graphics" => graphics(
            &GraphicsOptions {
                source_root: get("source-root")?,
                cc: get("cc")?,
                qemu: get("qemu")?,
                loader: get("loader")?,
                plugin: get("plugin")?,
                sdk: get("sdk")?,
                agnos_libraries: get("agnos-libraries")?,
                output: get("output")?,
            },
            &mut runner,
        )?,
        "ffmpeg" => ffmpeg(
            &FfmpegOptions {
                qemu: get("qemu")?,
                loader: get("loader")?,
                prefix: get("prefix")?,
                libraries: values
                    .get("library")
                    .cloned()
                    .filter(|v| !v.is_empty())
                    .ok_or("--library required")?,
                output: get("output")?,
            },
            &mut runner,
        )?,
        "sdl" => sdl(
            &SdlOptions {
                wheel: get("wheel")?,
                ttf_source: get("ttf-source")?,
                cc: get("cc")?,
                sdk: get("sdk")?,
                qemu: get("qemu")?,
                loader: get("loader")?,
                agnos_libraries: get("agnos-libraries")?,
                output: get("output")?,
                source_oracle: values.get("source-oracle").and_then(|v| v.first()).cloned(),
            },
            &mut runner,
        )?,
        _ => unreachable!(),
    };
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests;
