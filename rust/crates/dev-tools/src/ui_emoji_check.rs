//! Original seven-text emoji raster gate. Generic emoji rendering remains separate.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    image_array, image_array_math,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
const TEXTS: [&str; 7] = [
    "😀",
    "🇰🇷",
    "👨\u{200d}👩\u{200d}👧",
    "❤️",
    "😀😀",
    "⚠️",
    "👍🏽",
];
const IMAGES: [&[u8]; 7] = [
    include_bytes!("../tests/fixtures/ui-emoji/emoji-0-source.png"),
    include_bytes!("../tests/fixtures/ui-emoji/emoji-1-source.png"),
    include_bytes!("../tests/fixtures/ui-emoji/emoji-2-source.png"),
    include_bytes!("../tests/fixtures/ui-emoji/emoji-3-source.png"),
    include_bytes!("../tests/fixtures/ui-emoji/emoji-4-source.png"),
    include_bytes!("../tests/fixtures/ui-emoji/emoji-5-source.png"),
    include_bytes!("../tests/fixtures/ui-emoji/emoji-6-source.png"),
];
const CHECKER: &str = "rust/tools/check_ui_emoji.py";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn validate(root: &Path) -> Result<()> {
    let bytes = include_bytes!("../tests/fixtures/ui-emoji/original.json");
    if sha(bytes) != "24d7e26c62d9e4f06fc686d7909b211070b5444aa2faf0c69153ffb7828e7a99" {
        return Err("emoji original manifest changed".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(bytes)?;
    for (file, hash) in manifest["sources"]
        .as_object()
        .ok_or("emoji source pins missing")?
    {
        match fs::read(root.join(file)) {
            Ok(bytes) if Some(sha(&bytes).as_str()) != hash.as_str() => {
                return Err(
                    "emoji source/font changed; independently recapture original rasters".into(),
                )
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && file == CHECKER => {}
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("emoji source dependency lock missing")?;
    for (name, version) in manifest["dependencies"]
        .as_object()
        .ok_or("emoji dependency pins missing")?
    {
        let matching = packages
            .iter()
            .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some(name))
            .collect::<Vec<_>>();
        if matching.len() != 1
            || matching[0].get("version").and_then(toml::Value::as_str) != version.as_str()
        {
            return Err("emoji source dependency changed; independently recapture".into());
        }
    }
    for (index, bytes) in IMAGES.iter().enumerate() {
        if manifest["images"][format!("emoji-{index}-source.png")] != sha(bytes) {
            return Err("emoji captured image changed".into());
        }
    }
    Ok(())
}
fn lexical(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut output = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for component in bytes
        .split(|b| *b == b'/')
        .filter(|v| !v.is_empty() && *v != b".")
    {
        if !output.is_empty() && !output.ends_with(b"/") {
            output.push(b'/');
        }
        output.extend_from_slice(component);
    }
    if output.is_empty() {
        output.push(b'.');
    }
    std::ffi::OsString::from_vec(output).into()
}
#[derive(serde::Serialize)]
struct Row<'a> {
    text: &'a str,
    different_pixels: usize,
    max_channel_difference: i64,
}
/// Compare every original scene and preserve partial writes and failure ordering.
/// Broad native decoders retain the candidate's array mode; no RGBA normalization.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    let root = resolve_non_strict(root)?;
    validate(&root)?;
    let mut rows = vec![];
    for (index, text) in TEXTS.iter().enumerate() {
        let source = output.join(format!("emoji-{index}-source.png"));
        let native = output.join(format!("emoji-{index}-native.png"));
        fs::write(&source, IMAGES[index])?;
        let mut command = CommandSpec::new_os(
            vec![
                lexical(binary).into_os_string(),
                root.join("openpilot/selfdrive/assets/fonts/NotoColorEmoji.ttf")
                    .into_os_string(),
                (*text).into(),
                native.clone().into_os_string(),
            ],
            Duration::MAX,
        );
        command.inherit_output = true;
        command.inherit_stdin = true;
        let result = runner.run(&command)?;
        if result.code != 0 {
            return Err(format!("emoji candidate exited {}", result.code).into());
        }
        // The original keeps its in-memory image across the untrusted candidate call.
        let source = image_array::decode(IMAGES[index])?;
        let native = image_array::read(&native)?;
        let difference = image_array_math::compare(
            &source.shape,
            &source.values,
            &native.shape,
            &native.values,
        )?;
        rows.push(Row {
            text,
            different_pixels: difference.different_pixels,
            max_channel_difference: difference.max_channel_difference,
        });
    }
    fs::write(
        output.join("emoji-pixels.json"),
        serde_json::to_string_pretty(&rows)?,
    )?;
    if rows.iter().any(|r| r.different_pixels != 0) {
        return Err("original/native emoji rasters differ".into());
    }
    printed.write_all(b"PASS: 7 actual-source emoji rasters equal pixel-for-pixel, including flags, ZWJ, selectors, adjacent emoji and skin tone\n")?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-ui-emoji --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&args.values["--binary"]),
        Path::new(&args.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
