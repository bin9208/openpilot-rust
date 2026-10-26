//! Stage the pinned external raylib SDK; the GNU toolchain and libraries stay external.
use crate::{
    disk_space,
    encoder_stage::zip_directory_count,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Options {
    pub arch: String,
    pub output: PathBuf,
    pub shared: bool,
    pub cxx: String,
    pub wheel_file: Option<PathBuf>,
    pub library_directories: Vec<PathBuf>,
}
struct Pin {
    arch: String,
    url: String,
    sha256: String,
    size: u64,
    archive: String,
    machine: u16,
}
const CONTRACT_SOURCE:&str = "extern \"C\" const char *openpilot_startup_ui_raylib_contract() { return \"comma-deps-raylib-6.0.0.1.post103\"; }\n";
const GROWTH: u64 = 1024 * 1024 * 1024;
const MAX_FILE: u64 = 16 * 1024 * 1024;
fn check(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn pin_for(arch: &str) -> Result<Pin> {
    let (url,sha256,size,archive,machine)=match arch {
        "x86_64" => ("https://files.pythonhosted.org/packages/27/ce/e8b941bcf0b213ca9cb4d8cb69b18398b4dad77923925d80bc3281837461/comma_deps_raylib-6.0.0.1.post103-py3-none-manylinux_2_28_x86_64.whl","942ec942ec01005f05584eede1a6add483043e8e8f764e68cfebb7c21c443b89",20722617,"libraylib_desktop.a",62),
        "aarch64" => ("https://files.pythonhosted.org/packages/3d/4d/cceaf969d771af8870094d06bb84dca805f9268584a9a3c134dfee2307c7/comma_deps_raylib-6.0.0.1.post103-py3-none-manylinux_2_28_aarch64.whl","d9b9bebb39167442bacf627958d57ae95e452550ced1b68337cbf2a73375856a",21707647,"libraylib_comma.a",183),
        _ => return Err("unsupported raylib architecture".into()),
    };
    Ok(Pin {
        arch: arch.into(),
        url: url.into(),
        sha256: sha256.into(),
        size,
        archive: archive.into(),
        machine,
    })
}
fn text(path: &Path) -> Result<String> {
    Ok(path.to_str().ok_or("raylib path is not UTF-8")?.to_owned())
}
fn libraries(pin: &Pin) -> Vec<&'static str> {
    if pin.arch == "aarch64" {
        vec!["EGL", "GLESv2", "drm", "gbm"]
    } else {
        vec!["GL", "X11"]
    }
}
fn compiler_command(options: &Options, pin: &Pin, root: &Path) -> Result<CommandSpec> {
    let mut argv = vec![
        options.cxx.clone(),
        "-shared".into(),
        "-fPIC".into(),
        "-Wl,-Bsymbolic".into(),
        "-Wl,-z,defs".into(),
        text(&root.join("plugin_contract.cc"))?,
        "-Wl,--whole-archive".into(),
        text(&root.join("lib").join(&pin.archive))?,
        "-Wl,--no-whole-archive".into(),
    ];
    for path in &options.library_directories {
        let path = text(path)?;
        argv.extend([format!("-L{path}"), format!("-Wl,-rpath-link,{path}")]);
    }
    argv.extend(libraries(pin).into_iter().map(|name| format!("-l{name}")));
    argv.extend(["-lm", "-ldl", "-lpthread", "-lrt", "-o"].map(str::to_owned));
    argv.push(text(&root.join("lib/libopenpilot-raylib.so"))?);
    let mut command = CommandSpec::new(argv, Duration::from_secs(120));
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 8 * 1024 * 1024;
    Ok(command)
}
fn successful(
    runner: &mut impl Runner,
    command: &CommandSpec,
) -> Result<crate::qa_process::Captured> {
    let result = runner.run(command)?;
    check(
        result.code == 0,
        &format!(
            "raylib command failed ({}): {}",
            result.code,
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(result)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn freeze_wheel(
    options: &Options,
    pin: &Pin,
    root: &Path,
    runner: &mut impl Runner,
) -> Result<PathBuf> {
    check(pin.size <= 24 * 1024 * 1024, "oversized raylib wheel pin")?;
    let output = root.join("verified.whl");
    if let Some(path) = &options.wheel_file {
        let flags = i32::try_from(rustix::fs::OFlags::NONBLOCK.bits())?;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(flags)
            .open(path)?;
        let metadata = file.metadata()?;
        check(
            metadata.is_file() && metadata.len() == pin.size,
            "raylib wheel size/type differs from pin",
        )?;
        let mut input = file.take(pin.size + 1);
        let mut destination = fs::File::create(&output)?;
        let mut digest = Sha256::new();
        let mut count = 0_u64;
        let mut buffer = [0; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(u64::try_from(n)?)
                .ok_or("wheel size overflow")?;
            check(count <= pin.size, "raylib wheel grew while reading")?;
            digest.update(&buffer[..n]);
            destination.write_all(&buffer[..n])?;
        }
        check(
            count == pin.size && format!("{:x}", digest.finalize()) == pin.sha256,
            "raylib wheel SHA256 differs from uv.lock",
        )?;
    } else {
        let argv = vec![
            "curl".into(),
            "--fail".into(),
            "--location".into(),
            "--proto".into(),
            "=https".into(),
            "--proto-redir".into(),
            "=https".into(),
            "--connect-timeout".into(),
            "30".into(),
            "--max-time".into(),
            "120".into(),
            "--max-filesize".into(),
            pin.size.to_string(),
            "--output".into(),
            "-".into(),
            pin.url.clone(),
        ];
        let mut command = CommandSpec::new(argv, Duration::from_secs(150));
        command.stdout_limit = usize::try_from(pin.size + 1)?;
        command.stderr_limit = 1024 * 1024;
        let captured = successful(runner, &command)?;
        check(
            u64::try_from(captured.stdout.len())? == pin.size
                && hash(&captured.stdout) == pin.sha256,
            "downloaded raylib wheel SHA256/size differs from uv.lock",
        )?;
        fs::write(&output, captured.stdout)?;
    }
    Ok(output)
}
// Inspect ordinary EOCD and raw records before ZipArchive allocates its index.
// Both reviewed wheels use ordinary single-disk ZIP, not ZIP64 or SFX layouts.
fn zip_preflight(path: &Path) -> Result<usize> {
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    let length = size.min(65557);
    file.seek(SeekFrom::Start(size - length))?;
    let mut tail = vec![0; usize::try_from(length)?];
    file.read_exact(&mut tail)?;
    let position = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..].starts_with(b"PK\x05\x06")
                && i + 22 + usize::from(u16::from_le_bytes([tail[i + 20], tail[i + 21]]))
                    == tail.len()
        })
        .ok_or("missing raylib ZIP directory")?;
    let header = &tail[position..];
    let word = |i| u16::from_le_bytes([header[i], header[i + 1]]);
    let dword = |i| u32::from_le_bytes(header[i..i + 4].try_into().unwrap_or([0; 4]));
    let count = usize::from(word(10));
    check(
        word(4) == 0 && word(6) == 0 && usize::from(word(8)) == count && count > 0 && count <= 1024,
        "unsupported raylib ZIP directory count/layout",
    )?;
    let start = u64::from(dword(16));
    let end = start
        .checked_add(u64::from(dword(12)))
        .ok_or("ZIP directory overflow")?;
    check(
        end == size - length + u64::try_from(position)?,
        "raylib ZIP directory outside archive",
    )?;
    check(
        zip_directory_count(path, start)? == count,
        "duplicate or malformed raylib ZIP directory",
    )?;
    Ok(count)
}
fn payload(path: &Path, pin: &Pin) -> Result<BTreeMap<String, Vec<u8>>> {
    let count = zip_preflight(path)?;
    let mut wheel = zip::ZipArchive::new(fs::File::open(path)?)?;
    check(wheel.len() == count, "duplicate raylib ZIP member")?;
    let expected = [
        format!("lib/{}", pin.archive),
        "include/raylib.h".into(),
        "include/rlgl.h".into(),
        "include/raymath.h".into(),
    ];
    let mut selected = BTreeMap::new();
    for index in 0..wheel.len() {
        let entry = wheel.by_index(index)?;
        let name = entry.name()?;
        if let Some(relative) = expected
            .iter()
            .find(|relative| name.ends_with(&format!("/raylib/install/{relative}")))
        {
            check(
                !name.starts_with('/')
                    && !name.contains(['\\', '\0'])
                    && name.len() <= 4096
                    && name
                        .split('/')
                        .all(|part| !part.is_empty() && part != "." && part != ".."),
                "unsafe raylib ZIP member path",
            )?;
            let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
            check(
                !entry.is_dir() && (kind == 0 || kind == 0o100000),
                "unsafe raylib ZIP member type",
            )?;
            let limit = if relative.starts_with("include/") {
                1024 * 1024
            } else {
                MAX_FILE
            };
            check(entry.size() <= limit, "oversized raylib ZIP member")?;
            check(
                selected
                    .insert(relative.clone(), (index, entry.size()))
                    .is_none(),
                "expected exactly one native raylib member",
            )?;
        }
    }
    check(selected.len() == 4, "missing native raylib member")?;
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    for (name, (index, size)) in selected {
        total = total
            .checked_add(size)
            .ok_or("raylib payload size overflow")?;
        check(total <= 19 * 1024 * 1024, "oversized raylib payload")?;
        let mut raw = Vec::with_capacity(usize::try_from(size)?);
        wheel
            .by_index(index)?
            .take(size + 1)
            .read_to_end(&mut raw)?;
        check(
            u64::try_from(raw.len())? == size,
            "raylib ZIP member size differs",
        )?;
        files.insert(name, raw);
    }
    files.insert(
        "SOURCE_SHA256".into(),
        format!("{}\n", pin.sha256).into_bytes(),
    );
    Ok(files)
}
#[derive(PartialEq, Eq)]
struct Entry {
    bytes: Vec<u8>,
    mode: u32,
    inode: u64,
    modified: (i64, i64),
}
#[derive(PartialEq, Eq)]
struct Snapshot {
    files: BTreeMap<String, Entry>,
    inode: u64,
    mode: u32,
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = path.symlink_metadata()?;
    check(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= limit,
        "unsafe or oversized raylib artifact",
    )?;
    let mut raw = Vec::with_capacity(usize::try_from(metadata.len())?);
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut raw)?;
    check(
        u64::try_from(raw.len())? == metadata.len(),
        "raylib artifact changed while reading",
    )?;
    Ok(raw)
}
fn children(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        check(
            paths.len() < 16,
            "raylib directory entry limit exceeded; use fresh output",
        )?;
        paths.push(entry?.path());
    }
    Ok(paths)
}
fn snapshot(root: &Path, pin: &Pin) -> Result<Snapshot> {
    let metadata = root.symlink_metadata()?;
    check(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "raylib output is not an owned directory; use fresh output",
    )?;
    let mut files = BTreeMap::new();
    let mut entries = children(root)?;
    while let Some(path) = entries.pop() {
        let relative = path.strip_prefix(root)?;
        let name = text(relative)?;
        let metadata = path.symlink_metadata()?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            check(
                name == "include" || name == "lib",
                "unmanaged raylib directory; use fresh output",
            )?;
            entries.extend(children(&path)?);
            continue;
        }
        check(
            matches!(
                name.as_str(),
                "SOURCE_SHA256"
                    | "include/raylib.h"
                    | "include/rlgl.h"
                    | "include/raymath.h"
                    | "plugin_contract.cc"
                    | "plugin-build.json"
                    | "lib/libopenpilot-raylib.so"
            ) || name == format!("lib/{}", pin.archive),
            "unmanaged raylib file; use fresh output",
        )?;
        let limit = match name.as_str() {
            "SOURCE_SHA256" => 65,
            "plugin_contract.cc" => 1024,
            "plugin-build.json" => 65536,
            _ => MAX_FILE,
        };
        let bytes = bounded(&path, limit)?;
        files.insert(
            name,
            Entry {
                bytes,
                mode: metadata.mode(),
                inode: metadata.ino(),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
            },
        );
    }
    Ok(Snapshot {
        files,
        inode: metadata.ino(),
        mode: metadata.mode(),
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    wheel: String,
    sha256: String,
    arch: String,
    command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plugin_sha256: Option<String>,
}
fn elf(raw: &[u8], machine: u16) -> bool {
    raw.len() >= 20
        && &raw[..6] == b"\x7fELF\x02\x01"
        && raw[16..18] == 3_u16.to_le_bytes()
        && raw[18..20] == machine.to_le_bytes()
}
fn verify_command(options: &Options, pin: &Pin, command: &[String]) -> Result<()> {
    let suffix = libraries(pin).len() + 6;
    check(
        command.len() >= 9 + suffix,
        "invalid raylib compiler provenance",
    )?;
    let root = command[5]
        .strip_suffix("/plugin_contract.cc")
        .ok_or("invalid raylib compiler root")?;
    let middle = &command[9..command.len() - suffix];
    check(
        middle.len().is_multiple_of(2),
        "invalid raylib library provenance",
    )?;
    let mut directories = Vec::new();
    for pair in middle.chunks_exact(2) {
        let path = pair[0]
            .strip_prefix("-L")
            .ok_or("invalid raylib library directory")?;
        check(
            pair[1] == format!("-Wl,-rpath-link,{path}"),
            "invalid raylib rpath-link provenance",
        )?;
        directories.push(PathBuf::from(path));
    }
    let expected_options = Options {
        arch: pin.arch.clone(),
        output: PathBuf::from(root),
        shared: true,
        cxx: command[0].clone(),
        wheel_file: None,
        library_directories: directories,
    };
    check(
        compiler_command(&expected_options, pin, Path::new(root))?.argv == command,
        "raylib compiler flags/link order differ",
    )?;
    if options.shared {
        check(
            expected_options.cxx == options.cxx
                && expected_options.library_directories == options.library_directories,
            "raylib compiler provenance differs; use fresh output",
        )?;
    }
    Ok(())
}
// None is nonshared; Some(false) native verified shared; Some(true) legacy shared.
fn verify_existing(
    prior: &Snapshot,
    assets: &BTreeMap<String, Vec<u8>>,
    options: &Options,
    pin: &Pin,
) -> Result<Option<bool>> {
    for (name, raw) in assets {
        check(
            prior
                .files
                .get(name)
                .is_some_and(|entry| entry.bytes == *raw),
            "raylib pinned asset differs/missing; use fresh output",
        )?;
    }
    let names = [
        "plugin_contract.cc",
        "plugin-build.json",
        "lib/libopenpilot-raylib.so",
    ];
    let count = names
        .iter()
        .filter(|name| prior.files.contains_key(**name))
        .count();
    if count == 0 {
        return Ok(None);
    }
    check(count == 3, "incomplete raylib plugin; use fresh output")?;
    check(
        prior.files["plugin_contract.cc"].bytes == CONTRACT_SOURCE.as_bytes(),
        "raylib plugin contract differs",
    )?;
    let proof: Proof = serde_json::from_slice(&prior.files["plugin-build.json"].bytes)?;
    check(
        proof.wheel == pin.url && proof.sha256 == pin.sha256 && proof.arch == pin.arch,
        "raylib plugin provenance differs",
    )?;
    verify_command(options, pin, &proof.command)?;
    let plugin = &prior.files["lib/libopenpilot-raylib.so"].bytes;
    check(elf(plugin, pin.machine), "raylib plugin ELF target differs")?;
    if let Some(expected) = proof.plugin_sha256 {
        check(
            hash(plugin) == expected,
            "raylib plugin hash differs; use fresh output",
        )?;
        Ok(Some(false))
    } else {
        check(
            options.shared,
            "legacy raylib plugin needs --shared verification or fresh output",
        )?;
        Ok(Some(true))
    }
}
pub fn stage(options: &Options) -> Result<PathBuf> {
    stage_with(options, &pin_for(&options.arch)?, &mut NativeRunner)
}
fn stage_with(options: &Options, pin: &Pin, runner: &mut impl Runner) -> Result<PathBuf> {
    stage_with_space(options, pin, runner, &mut disk_space::inspect)
}
fn stage_with_space(
    options: &Options,
    pin: &Pin,
    runner: &mut impl Runner,
    inspect: &mut impl FnMut(&Path, u64) -> Result<disk_space::Space>,
) -> Result<PathBuf> {
    check(options.arch == pin.arch, "raylib architecture/pin differs")?;
    let output = std::env::current_dir()?.join(&options.output);
    let parent = output.parent().ok_or("raylib output parent missing")?;
    inspect(parent, GROWTH)?.require()?;
    fs::create_dir_all(parent)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .custom_flags(i32::try_from(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits(),
        )?)
        .open(parent.join(".startup-ui-raylib.lock"))?;
    check(
        lock.metadata()?.is_file(),
        "raylib lock is not a regular file",
    )?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)?;
    let temporary = tempfile::Builder::new()
        .prefix(".raylib-")
        .tempdir_in(parent)?;
    let wheel = freeze_wheel(options, pin, temporary.path(), runner)?;
    let assets = payload(&wheel, pin)?;
    let prior = match output.symlink_metadata() {
        Ok(_) => Some(snapshot(&output, pin)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let prior_shared = prior
        .as_ref()
        .map(|prior| verify_existing(prior, &assets, options, pin))
        .transpose()?;
    if prior.is_some() && (!options.shared || prior_shared == Some(Some(false))) {
        return Ok(output.canonicalize()?);
    }
    let generation = temporary.path().join("generation");
    fs::create_dir(&generation)?;
    for (name, raw) in &assets {
        let path = generation.join(name);
        fs::create_dir_all(path.parent().ok_or("asset parent missing")?)?;
        fs::write(path, raw)?;
    }
    if options.shared {
        inspect(parent, GROWTH)?.require()?;
        fs::write(generation.join("plugin_contract.cc"), CONTRACT_SOURCE)?;
        let command = compiler_command(options, pin, &generation)?;
        successful(runner, &command)?;
        let plugin = bounded(&generation.join("lib/libopenpilot-raylib.so"), MAX_FILE)?;
        check(
            elf(&plugin, pin.machine),
            "linked raylib plugin ELF target differs",
        )?;
        if prior_shared == Some(Some(true)) {
            check(
                prior.as_ref().ok_or("missing legacy generation")?.files
                    ["lib/libopenpilot-raylib.so"]
                    .bytes
                    == plugin,
                "legacy raylib plugin differs from verified relink; use fresh output",
            )?;
        }
        let proof = Proof {
            wheel: pin.url.clone(),
            sha256: pin.sha256.clone(),
            arch: pin.arch.clone(),
            command: command.argv,
            plugin_sha256: Some(hash(&plugin)),
        };
        let mut raw = serde_json::to_vec_pretty(&proof)?;
        raw.push(b'\n');
        fs::write(generation.join("plugin-build.json"), raw)?;
    }
    if let Some(prior) = prior {
        check(
            snapshot(&output, pin)? == prior,
            "raylib output identity changed during staging",
        )?;
        fs::set_permissions(&generation, fs::Permissions::from_mode(prior.mode & 0o7777))?;
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &generation,
            rustix::fs::CWD,
            &output,
            rustix::fs::RenameFlags::EXCHANGE,
        )?;
    } else {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &generation,
            rustix::fs::CWD,
            &output,
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
    }
    Ok(output.canonicalize()?)
}
#[cfg(test)]
#[path = "raylib_stage_tests.rs"]
mod tests;
