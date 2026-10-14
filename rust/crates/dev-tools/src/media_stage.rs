//! Stage validated external media providers with frozen hashes and no Python.
use crate::{
    disk_space,
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};
const MAX_FILE: u64 = 1024 * 1024 * 1024;
fn check(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn read_json(path: &Path) -> Result<Value> {
    let bytes = read_bounded(path, 16 * 1024 * 1024)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    check(bytes.len() as u64 <= limit, "media evidence too large")?;
    Ok(bytes)
}
fn entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}
fn name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid media artifact name".into())
}
fn safe_parent(root: &Path, path: &Path) -> Result<()> {
    check(path.starts_with(root), "media destination escapes package")?;
    for parent in path.ancestors().skip(1).take_while(|p| *p != root) {
        check(!parent.is_symlink(), "package directory is a symlink")?;
        if parent.exists() {
            check(parent.is_dir(), "package parent is not a directory")?;
        }
    }
    Ok(())
}
struct Artifact {
    source: PathBuf,
    target: PathBuf,
    hash: String,
    size: u64,
    mode: u32,
}
fn add(
    artifacts: &mut Vec<Artifact>,
    providers: &Path,
    source: PathBuf,
    target: PathBuf,
    expected: Option<&BTreeMap<PathBuf, String>>,
) -> Result<()> {
    let canonical = source.canonicalize()?;
    check(
        canonical.starts_with(providers),
        "media source escapes provider tree",
    )?;
    let metadata = fs::metadata(&canonical)?;
    check(
        metadata.is_file() && metadata.len() <= MAX_FILE,
        "invalid or oversized media source",
    )?;
    let hash = digest(&canonical)?;
    if let Some(expected) = expected {
        check(
            expected.get(&canonical) == Some(&hash),
            "media provider missing from evidence or hash changed",
        )?;
        let mut header = [0_u8; 20];
        File::open(&canonical)?.read_exact(&mut header)?;
        check(
            &header[..4] == b"\x7fELF"
                && header[4] == 2
                && header[5] == 1
                && u16::from_le_bytes([header[18], header[19]]) == 183,
            "media provider is not aarch64 ELF",
        )?;
    }
    check(
        !artifacts.iter().any(|a| a.target == target),
        "duplicate media artifact destination",
    )?;
    artifacts.push(Artifact {
        source,
        target,
        hash,
        size: metadata.len(),
        mode: metadata.permissions().mode() & 0o777,
    });
    Ok(())
}
/// Preserve the legacy stager's layout, licenses, symlinks and provider selection.
/// Reports are independently generated proof; this command does not execute codecs.
pub fn stage(
    root: &Path,
    providers: &Path,
    ffmpeg_report: &Path,
    graphics_report: &Path,
) -> Result<Value> {
    let root = root.canonicalize()?;
    let providers = providers.canonicalize()?;
    check(
        root.is_dir() && providers.is_dir(),
        "media roots must be directories",
    )?;
    let reports = [
        read_json(ffmpeg_report)?,
        read_json(&providers.join("sdl-validated/report.json"))?,
        read_json(graphics_report)?,
    ];
    let mut expected = BTreeMap::new();
    for report in &reports {
        check(
            report["passed"] == true,
            "media provider validation has not passed",
        )?;
        for row in report["objects"]
            .as_array()
            .ok_or("media object evidence missing")?
        {
            let path = PathBuf::from(row["path"].as_str().ok_or("media evidence path missing")?);
            check(path.is_absolute(), "media evidence path must be absolute")?;
            let hash = row["sha256"]
                .as_str()
                .ok_or("media evidence hash missing")?;
            check(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "media evidence hash invalid",
            )?;
            if let Some(old) = expected.insert(path, hash.to_owned()) {
                check(old == hash, "contradictory media evidence")?;
            }
        }
    }
    let wheel = reports[1]["wheel_sha256"]
        .as_str()
        .ok_or("SDL wheel identity missing")?;
    check(
        wheel.len() == 64 && wheel.bytes().all(|b| b.is_ascii_hexdigit()),
        "SDL wheel identity invalid",
    )?;
    let binaries = root.join("rust/bin");
    let lib = binaries.join("lib");
    let mut artifacts = Vec::new();
    let mut aliases = Vec::new();
    for source in entries(&providers.join("sdl-validated/lib"))? {
        check(!source.is_symlink(), "SDL provider must be a regular file")?;
        let target = lib.join(name(&source)?);
        add(&mut artifacts, &providers, source, target, Some(&expected))?;
    }
    let ffmpeg_lib = providers.join("ffmpeg/install/lib");
    for source in entries(&ffmpeg_lib)? {
        let n = name(&source)?;
        if n.contains(".so.") && !source.is_symlink() {
            add(
                &mut artifacts,
                &providers,
                source.clone(),
                lib.join(n),
                Some(&expected),
            )?;
        }
        if n.contains(".so") && source.is_symlink() {
            let target = source.canonicalize()?;
            check(
                target.parent() == Some(ffmpeg_lib.as_path()),
                "FFmpeg alias escapes provider directory",
            )?;
            aliases.push((lib.join(n), PathBuf::from(name(&target)?)));
        }
    }
    for relative in [
        "media-sdk/usr/lib/aarch64-linux-gnu/libvpx.so.9.0.0",
        "raylib/lib/libopenpilot-raylib.so",
    ] {
        let source = providers.join(relative);
        let target = lib.join(name(&source)?);
        add(&mut artifacts, &providers, source, target, Some(&expected))?;
    }
    for (n, target) in [
        ("libvpx.so.9", "libvpx.so.9.0.0"),
        ("libSDL2-2.0.so.0", "libSDL2-2-6e5c51a6.0.so.0.2800.4"),
        (
            "libSDL2_ttf-2.0.so.0",
            "libSDL2_ttf-2-e035a276.0.so.0.2000.1",
        ),
    ] {
        aliases.push((lib.join(n), PathBuf::from(target)));
    }
    for n in ["ffmpeg", "ffprobe"] {
        add(
            &mut artifacts,
            &providers,
            providers.join("ffmpeg/install/bin").join(n),
            binaries.join(n),
            Some(&expected),
        )?;
    }
    add(
        &mut artifacts,
        &providers,
        providers.join("sdl-validated/freesansbold.ttf"),
        binaries.join("cluster-replay/freesansbold.ttf"),
        None,
    )?;
    for (source_dir, target_dir, prefix) in [
        ("sdl-licenses", "licenses/sdl", ""),
        ("raylib/licenses", "licenses/raylib", ""),
        ("ffmpeg/ffmpeg-6.1.1", "licenses/ffmpeg", "COPYING."),
    ] {
        let selected = entries(&providers.join(source_dir))?
            .into_iter()
            .filter(|p| name(p).is_ok_and(|n| n.starts_with(prefix)))
            .collect::<Vec<_>>();
        check(!selected.is_empty(), "media license files missing")?;
        for source in selected {
            let target = binaries.join(target_dir).join(name(&source)?);
            add(&mut artifacts, &providers, source, target, None)?;
        }
    }
    for (source, target) in [
        ("raylib/include/raylib.h", "licenses/raylib/raylib.h"),
        ("raylib/plugin-build.json", "licenses/raylib/build.json"),
        ("ffmpeg/build-record.json", "licenses/ffmpeg/build.json"),
        ("vpx-package-metadata.json", "licenses/vpx/package.json"),
        (
            "vpx-sdk/usr/share/doc/libvpx9/copyright",
            "licenses/vpx/copyright",
        ),
    ] {
        add(
            &mut artifacts,
            &providers,
            providers.join(source),
            binaries.join(target),
            None,
        )?;
    }
    artifacts.sort_by(|a, b| a.target.cmp(&b.target));
    aliases.sort();
    let mut destinations = BTreeSet::new();
    for artifact in &artifacts {
        safe_parent(&root, &artifact.target)?;
        check(
            destinations.insert(artifact.target.clone()),
            "duplicate media destination",
        )?;
        check(
            !artifact.target.is_symlink(),
            "package artifact is a symlink",
        )?;
        if artifact.target.exists() {
            check(
                artifact.target.is_file()
                    && digest(&artifact.target)? == artifact.hash
                    && fs::metadata(&artifact.target)?.permissions().mode() & 0o777
                        == artifact.mode,
                "conflicting package artifact",
            )?;
        }
    }
    for (target, relative) in &aliases {
        safe_parent(&root, target)?;
        check(
            destinations.insert(target.clone()),
            "duplicate media alias destination",
        )?;
        check(
            artifacts.iter().any(|a| a.target == lib.join(relative)),
            "media alias target is not a staged artifact",
        )?;
        if target.is_symlink() {
            check(
                fs::read_link(target)? == *relative,
                "conflicting provider alias",
            )?;
        } else {
            check(!target.exists(), "provider alias would replace a file")?;
        }
    }
    let provenance = json!({"architecture":"aarch64","ownership":"external native providers","files":artifacts.iter().map(|a|json!({"source":a.source,"path":a.target,"sha256":a.hash})).collect::<Vec<_>>(),"aliases":aliases.iter().map(|(path,target)|json!({"path":path,"target":target})).collect::<Vec<_>>(),"system_dependencies":"Use pinned AGNOS 19.8-carrot-bt1; system graphics, x264, OpenSSL, zlib and C runtime are not copied","sdl_wheel_sha256":wheel});
    let output = binaries.join("media-provenance.json");
    safe_parent(&root, &output)?;
    check(!output.is_symlink(), "media provenance is a symlink")?;
    if output.exists() {
        check(
            read_json(&output)? == provenance,
            "conflicting media provenance",
        )?;
    }
    let growth = artifacts.iter().try_fold(1024 * 1024_u64, |n, a| {
        n.checked_add(a.size).ok_or("media staging size overflow")
    })?;
    disk_space::inspect(&root, growth)?.require()?;
    let frozen = tempfile::tempdir_in(&root)?;
    for (index, artifact) in artifacts.iter().enumerate() {
        let mut input = File::open(&artifact.source)?.take(artifact.size + 1);
        let path = frozen.path().join(index.to_string());
        let mut file = File::create(&path)?;
        check(
            std::io::copy(&mut input, &mut file)? == artifact.size,
            "media source changed size during staging",
        )?;
        check(
            digest(&path)? == artifact.hash,
            "media source changed during staging",
        )?;
        file.set_permissions(fs::Permissions::from_mode(artifact.mode))?;
        file.sync_all()?;
    }
    let frozen_provenance = frozen.path().join("provenance");
    let mut file = File::create(&frozen_provenance)?;
    serde_json::to_writer_pretty(&mut file, &provenance)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    for (index, artifact) in artifacts.iter().enumerate() {
        safe_parent(&root, &artifact.target)?;
        fs::create_dir_all(artifact.target.parent().ok_or("artifact parent missing")?)?;
        if !artifact.target.exists() {
            fs::hard_link(frozen.path().join(index.to_string()), &artifact.target)?;
        }
        check(
            !artifact.target.is_symlink()
                && digest(&artifact.target)? == artifact.hash
                && fs::metadata(&artifact.target)?.permissions().mode() & 0o777 == artifact.mode,
            "media destination changed during staging",
        )?;
    }
    for (target, relative) in &aliases {
        if !target.is_symlink() {
            symlink(relative, target)?;
        }
    }
    if !output.exists() {
        fs::hard_link(frozen_provenance, &output)?;
    }
    Ok(json!({"files":artifacts.len(),"aliases":aliases.len(),"provenance":output}))
}
