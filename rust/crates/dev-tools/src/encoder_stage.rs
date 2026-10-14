//! Stage pinned ARM codec payloads without installing or invoking Python.
use crate::seal::{digest, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
const PAYLOAD_LIMIT: u64 = 256 * 1024 * 1024;
const SOURCE_LIMIT: u64 = 512 * 1024 * 1024;
pub const SWSCALE_VERSION: &str = "7.1";
pub const SWSCALE_SHA256: &str = "40973d44970dbc83ef302b0609f2e74982be2d85916dd2ee7472d30678a7abe6";
pub const SWSCALE_URL: &str = "https://ffmpeg.org/releases/ffmpeg-7.1.tar.xz";
#[derive(Clone, Debug, Serialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub commit: String,
    pub sha256: String,
    pub size: u64,
}
impl Package {
    pub fn filename(&self) -> String {
        format!("{}-{}-py3-none-linux_aarch64.whl", self.name, self.version)
    }
    pub fn url(&self) -> String {
        format!(
            "https://github.com/commaai/dependencies/releases/download/{}/v{}/{}",
            self.name,
            self.version,
            self.filename()
        )
    }
    pub fn prefix(&self) -> String {
        format!(
            "{}-{}.data/purelib/{}/install/",
            self.name, self.version, self.name
        )
    }
}
pub fn packages() -> [Package; 2] {
    [
        Package {
            name: "ffmpeg".into(),
            version: "7.1.0".into(),
            commit: "b9732165bcf5a3fab83b05994187802a0d115b6e".into(),
            sha256: "ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4".into(),
            size: 11743600,
        },
        Package {
            name: "libyuv".into(),
            version: "1922.0".into(),
            commit: "28c3c2a2444232aeeaf989c33fd333ce74e6fc90".into(),
            sha256: "1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475".into(),
            size: 267013,
        },
    ]
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut f = File::create(path)?;
    serde_json::to_writer_pretty(&mut f, value)?;
    f.write_all(b"\n")?;
    Ok(())
}
fn safe(name: &str) -> Result<()> {
    require(
        !name.is_empty()
            && !name.starts_with('/')
            && !name.contains(['\\', '\0'])
            && name.len() <= 4096
            && name
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."),
        format!("unsafe native archive member: {name}"),
    )
}
fn ancestor(path: &Path) -> Result<PathBuf> {
    let mut path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    while !path.exists() {
        path = path
            .parent()
            .ok_or("missing filesystem ancestor")?
            .to_owned();
    }
    Ok(path)
}
fn space(path: &Path, growth: u64) -> Result<Value> {
    let stats = rustix::fs::statvfs(ancestor(path)?)?;
    let free = stats.f_bavail.saturating_mul(stats.f_frsize);
    let required = 25 * 1024 * 1024 * 1024 + growth;
    let value = json!({"free_bytes":free,"required_bytes":required,"growth_bytes":growth});
    println!("{value}");
    require(
        free >= required,
        format!(
            "require 25 GiB plus estimated growth; recover at least 35 GiB before retry: {value}"
        ),
    )?;
    Ok(value)
}
/// Verify the unique reviewed uv.lock source and version before staging a wheel.
pub fn verify_lock(lock: &toml::Value, package: &Package) -> Result<()> {
    let rows = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("invalid uv.lock package array")?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|r| r.get("name").and_then(toml::Value::as_str) == Some(&package.name))
        .collect();
    require(
        matches.len() == 1
            && matches[0].get("version").and_then(toml::Value::as_str) == Some(&package.version)
            && matches[0]
                .get("source")
                .and_then(|s| s.get("git"))
                .and_then(toml::Value::as_str)
                .is_some_and(|s| s.ends_with(&format!("#{}", package.commit))),
        format!(
            "{}: locked source differs from the reviewed native payload",
            package.name
        ),
    )
}
/// Check both byte size and SHA256 against the reviewed wheel pin.
pub fn verify_archive(archive: &Path, package: &Package) -> Result<()> {
    require(
        fs::metadata(archive)?.len() == package.size && digest(archive)? == package.sha256,
        format!("{}: native wheel SHA256/size differs", package.name),
    )
}
#[derive(Debug, Serialize)]
pub struct FileRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
pub(crate) fn zip_directory_count(path: &Path, start: u64) -> Result<usize> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(start))?;
    let mut count = 0;
    loop {
        let mut header = [0u8; 46];
        file.read_exact(&mut header[..4])?;
        if &header[..4] != b"PK\x01\x02" {
            require(
                &header[..4] == b"PK\x05\x06" || &header[..4] == b"PK\x06\x06",
                "invalid ZIP directory terminator",
            )?;
            break;
        }
        file.read_exact(&mut header[4..])?;
        count += 1;
        require(count <= 100000, "oversized wheel member count")?;
        let length = [28, 30, 32]
            .into_iter()
            .map(|i| u64::from(u16::from_le_bytes([header[i], header[i + 1]])))
            .sum::<u64>();
        let end = file
            .stream_position()?
            .checked_add(length)
            .ok_or("ZIP directory overflow")?;
        require(end <= size, "ZIP directory outside archive")?;
        file.seek(SeekFrom::Start(end))?;
    }
    Ok(count)
}
/// Validate every selected ZIP record before creating any output directory.
pub fn extract_native(archive: &Path, package: &Package, output: &Path) -> Result<Vec<FileRecord>> {
    require(
        fs::metadata(archive)?.len() <= 128 * 1024 * 1024,
        "oversized wheel archive",
    )?;
    let mut wheel = zip::ZipArchive::new(File::open(archive)?)?;
    // ZipArchive indexes by name and replaces duplicate central-directory names.
    // Count raw records too, so that this indexing cannot conceal duplicates.
    require(
        zip_directory_count(archive, wheel.central_directory_start())? == wheel.len(),
        "unsafe duplicate native archive member",
    )?;
    require(wheel.len() <= 100000, "oversized wheel member count")?;
    let prefix = package.prefix();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..wheel.len() {
        let e = wheel.by_index(index)?;
        if !e.name()?.starts_with(&prefix) || e.is_dir() {
            continue;
        }
        let name = e
            .name()?
            .strip_prefix(&prefix)
            .ok_or("missing native prefix")?
            .to_owned();
        safe(&name)?;
        let mode = e.unix_mode().unwrap_or(0);
        require(
            mode & 0o170000 != 0o120000 && (mode & 0o170000 == 0 || mode & 0o170000 == 0o100000),
            format!("unsafe native archive member: {}", e.name()?),
        )?;
        require(
            seen.insert(name.clone()),
            format!("unsafe duplicate native archive member: {}", e.name()?),
        )?;
        total = total
            .checked_add(e.size())
            .ok_or("native payload size overflow")?;
        require(
            total <= PAYLOAD_LIMIT,
            "missing or oversized native payload",
        )?;
        selected.push((index, name, e.size()));
    }
    require(
        !selected.is_empty(),
        format!("{}: missing or oversized native payload", package.name),
    )?;
    for (_, name, _) in &selected {
        let mut parent = Path::new(name).parent();
        while let Some(p) = parent {
            if let Some(s) = p.to_str().filter(|s| !s.is_empty()) {
                require(!seen.contains(s), "native archive file/directory collision")?;
            }
            parent = p.parent();
        }
    }
    space(output, total)?;
    require(
        fs::symlink_metadata(output).is_err_and(|e| e.kind() == io::ErrorKind::NotFound),
        "native output already exists",
    )?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".encoder-wheel-")
        .tempdir_in(parent)?;
    let mut records = Vec::new();
    for (index, name, size) in selected {
        let target = temporary.path().join(&name);
        fs::create_dir_all(target.parent().ok_or("missing output parent")?)?;
        let mut e = wheel.by_index(index)?;
        let mut file = File::create(&target)?;
        let copied = io::copy(&mut (&mut e).take(size + 1), &mut file)?;
        require(
            copied == size,
            "native payload size differs from ZIP metadata",
        )?;
        let mut extra = [0; 1];
        require(e.read(&mut extra)? == 0, "native payload exceeds ZIP size")?;
        if ["bin/ffmpeg", "bin/ffprobe"].contains(&name.as_str()) {
            fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
        }
        records.push(FileRecord {
            path: name,
            bytes: size,
            sha256: digest(&target)?,
        });
    }
    fs::rename(temporary.path(), output)?;
    Ok(records)
}
/// Native external commands only; injectable for owned build-command tests.
pub trait Runner {
    fn run(&mut self, args: &[String], cwd: Option<&Path>, log: Option<&Path>) -> Result<()>;
}
pub struct NativeRunner;
impl Runner for NativeRunner {
    fn run(&mut self, args: &[String], cwd: Option<&Path>, log: Option<&Path>) -> Result<()> {
        let (first, rest) = args.split_first().ok_or("empty native command")?;
        let mut command = Command::new(first);
        command.args(rest);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        if let Some(log) = log {
            let file = OpenOptions::new().append(true).create(true).open(log)?;
            command
                .stdout(Stdio::from(file.try_clone()?))
                .stderr(Stdio::from(file));
        }
        let status = command.status()?;
        require(
            status.success(),
            format!("native command failed ({status}): {first}"),
        )
    }
}
fn download(url: &str, archive: &Path, limit: u64) -> Vec<String> {
    [
        "curl",
        "--retry",
        "3",
        "--connect-timeout",
        "20",
        "--max-time",
        "180",
        "-fsSL",
        url,
        "-o",
    ]
    .into_iter()
    .map(String::from)
    .chain([
        archive.to_string_lossy().into_owned(),
        "--max-filesize".into(),
        limit.to_string(),
    ])
    .collect()
}
/// The wheel's avutil ABI headers must exactly match the pinned scaler source.
pub fn verify_swscale_headers(source: &Path, install: &Path) -> Result<()> {
    for name in ["version.h", "pixfmt.h"] {
        let header = install.join("include/libavutil").join(name);
        require(
            fs::read(&header)? == fs::read(source.join("libavutil").join(name))?,
            format!(
                "{}: avutil header differs from the pinned scaler source",
                header.display()
            ),
        )?;
    }
    Ok(())
}
#[derive(Clone)]
enum SourceKind {
    File,
    Directory,
    Symbolic(String),
    Hard(String),
}
fn source_relative(name: &str, version: &str) -> Result<()> {
    safe(name)?;
    let root = format!("ffmpeg-{version}");
    require(
        name == root || name.starts_with(&format!("{root}/")),
        "source archive outside pinned source directory",
    )
}
fn link_target(name: &str, link: &str, hard: bool, version: &str) -> Result<String> {
    require(
        !link.is_empty() && !link.starts_with('/') && !link.contains(['\\', '\0']),
        "unsafe source archive link",
    )?;
    let mut parts: Vec<&str> = if hard {
        Vec::new()
    } else {
        name.split('/').collect::<Vec<_>>()[..name.split('/').count() - 1].to_vec()
    };
    for p in link.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                require(parts.pop().is_some(), "source archive link escapes staging")?;
            }
            _ => parts.push(p),
        }
    }
    let target = parts.join("/");
    source_relative(&target, version)?;
    Ok(target)
}
fn source_entries(archive: &Path, version: &str) -> Result<BTreeMap<String, SourceKind>> {
    let mut tar = tar::Archive::new(xz2::read::XzDecoder::new_multi_decoder(File::open(
        archive,
    )?));
    let mut entries = BTreeMap::new();
    let mut total = 0u64;
    for e in tar.entries()?.raw(true) {
        let mut e = e?;
        require(entries.len() < 100000, "source archive member limit")?;
        let raw = e.header().as_bytes();
        require(
            !raw[..100].contains(&b'\\') && !raw[345..500].contains(&b'\\'),
            "unsafe source path separator",
        )?;
        let name = e
            .path()?
            .to_str()
            .ok_or("non-UTF8 source path")?
            .trim_end_matches('/')
            .to_owned();
        source_relative(&name, version)?;
        let kind = e.header().entry_type();
        let kind = if kind.is_file() {
            total = total
                .checked_add(e.size())
                .ok_or("source archive size overflow")?;
            require(
                total <= SOURCE_LIMIT,
                "source archive exceeds staging limit",
            )?;
            SourceKind::File
        } else if kind.is_dir() {
            require(e.size() == 0, "nonempty source directory record")?;
            SourceKind::Directory
        } else if kind.is_symlink() || kind.is_hard_link() {
            let bytes = e.header().as_bytes()[157..257]
                .split(|b| *b == 0)
                .next()
                .ok_or("missing source link")?;
            let link = std::str::from_utf8(bytes)?.to_owned();
            link_target(&name, &link, kind.is_hard_link(), version)?;
            require(e.size() == 0, "nonempty source link record")?;
            if kind.is_symlink() {
                SourceKind::Symbolic(link)
            } else {
                SourceKind::Hard(link)
            }
        } else {
            return Err("unsupported source archive member type".into());
        };
        require(
            entries.insert(name, kind).is_none(),
            "duplicate source archive member",
        )?;
        io::copy(&mut e, &mut io::sink())?;
    }
    let mut decoder = tar.into_inner();
    let mut tail = [0; 10240];
    let mut padding = 0;
    loop {
        let n = decoder.read(&mut tail)?;
        if n == 0 {
            break;
        }
        padding += n;
        require(
            padding <= 10240 && tail[..n].iter().all(|b| *b == 0),
            "unexpected source archive tail",
        )?;
    }
    require(!entries.is_empty(), "empty source archive")?;
    for (name, kind) in &entries {
        let mut parent = Path::new(name).parent();
        while let Some(p) = parent {
            if let Some(s) = p.to_str() {
                require(
                    entries
                        .get(s)
                        .is_none_or(|k| matches!(k, SourceKind::Directory)),
                    "source archive parent is not a directory",
                )?;
            }
            parent = p.parent();
        }
        if let SourceKind::Symbolic(link) | SourceKind::Hard(link) = kind {
            let target = link_target(name, link, matches!(kind, SourceKind::Hard(_)), version)?;
            require(
                entries
                    .get(&target)
                    .is_some_and(|k| matches!(k, SourceKind::File)),
                "source link target is not a regular archive member",
            )?;
        }
    }
    Ok(entries)
}
pub(crate) fn extract_source(archive: &Path, output: &Path, version: &str) -> Result<()> {
    let entries = source_entries(archive, version)?;
    let mut tar = tar::Archive::new(xz2::read::XzDecoder::new_multi_decoder(File::open(
        archive,
    )?));
    for e in tar.entries()?.raw(true) {
        let mut e = e?;
        let name = e
            .path()?
            .to_str()
            .ok_or("non-UTF8 source name")?
            .trim_end_matches('/')
            .to_owned();
        let target = output.join(&name);
        match &entries[&name] {
            SourceKind::Directory => fs::create_dir_all(target)?,
            SourceKind::File => {
                fs::create_dir_all(target.parent().ok_or("missing source parent")?)?;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)?;
                io::copy(&mut e, &mut file)?;
                fs::set_permissions(
                    target,
                    fs::Permissions::from_mode((e.header().mode()? & 0o755) | 0o600),
                )?;
            }
            _ => {}
        }
    }
    for (name, kind) in entries {
        let target = output.join(&name);
        if let SourceKind::Symbolic(link) | SourceKind::Hard(link) = &kind {
            fs::create_dir_all(target.parent().ok_or("missing link parent")?)?;
            if matches!(kind, SourceKind::Symbolic(_)) {
                symlink(link, target)?;
            } else {
                fs::hard_link(
                    output.join(link_target(&name, link, true, version)?),
                    target,
                )?;
            }
        }
    }
    Ok(())
}
struct ScalerPin<'a> {
    version: &'a str,
    sha256: &'a str,
    url: &'a str,
}
#[derive(Clone)]
pub struct Toolchain {
    pub cc: String,
    pub ar: String,
    pub ranlib: String,
}
impl Default for Toolchain {
    fn default() -> Self {
        Self {
            cc: "aarch64-linux-gnu-gcc".into(),
            ar: "aarch64-linux-gnu-ar".into(),
            ranlib: "aarch64-linux-gnu-ranlib".into(),
        }
    }
}
fn scaler(
    output: &Path,
    install: &Path,
    source_archive: Option<&Path>,
    tools: &Toolchain,
    runner: &mut impl Runner,
    pin: &ScalerPin<'_>,
) -> Result<Value> {
    space(output, 512 * 1024 * 1024)?;
    fs::create_dir_all(output.parent().ok_or("missing scaler output parent")?)?;
    fs::create_dir(output)?;
    let archive = output.join(format!("ffmpeg-{}.tar.xz", pin.version));
    if let Some(origin) = source_archive {
        require(
            fs::metadata(origin)?.len() <= 128 * 1024 * 1024,
            "source archive exceeds download limit",
        )?;
        fs::copy(origin, &archive)?;
    } else {
        runner.run(&download(pin.url, &archive, 128 * 1024 * 1024), None, None)?;
    }
    require(
        digest(&archive)? == pin.sha256,
        format!(
            "{}: source SHA256 differs from pinned FFmpeg {}",
            archive.display(),
            pin.version
        ),
    )?;
    extract_source(&archive, output, pin.version)?;
    let source = output.join(format!("ffmpeg-{}", pin.version));
    verify_swscale_headers(&source, install)?;
    let work = output.join("build");
    fs::create_dir(&work)?;
    let mut configure = vec![
        source.join("configure").to_string_lossy().into_owned(),
        format!("--prefix={}", install.display()),
        "--arch=aarch64".into(),
        "--target-os=linux".into(),
        "--enable-cross-compile".into(),
        format!("--cc={}", tools.cc),
        format!("--ar={}", tools.ar),
        format!("--ranlib={}", tools.ranlib),
    ];
    configure.extend(
        [
            "--enable-static",
            "--disable-shared",
            "--enable-pic",
            "--disable-autodetect",
            "--disable-everything",
            "--disable-debug",
            "--disable-doc",
            "--disable-programs",
            "--disable-avcodec",
            "--disable-avformat",
            "--disable-avdevice",
            "--disable-avfilter",
            "--disable-swresample",
            "--disable-postproc",
            "--enable-avutil",
            "--enable-swscale",
        ]
        .into_iter()
        .map(String::from),
    );
    let commands = vec![
        configure,
        vec![
            "make".into(),
            "-j2".into(),
            "libswscale/libswscale.a".into(),
        ],
    ];
    let mut receipt = json!({"source_version":pin.version,"source_sha256":pin.sha256,"url":pin.url,"license":"LGPL-2.1-or-later","commands":commands,"files":{}});
    let record = output.join("receipt.json");
    write_json(&record, &receipt)?;
    let log = output.join("build.log");
    File::create(&log)?;
    for command in &commands {
        space(output, 512 * 1024 * 1024)?;
        runner.run(command, Some(&work), Some(&log))?;
    }
    space(output, 16 * 1024 * 1024)?;
    let mut files = vec![(
        work.join("libswscale/libswscale.a"),
        "lib/libswscale.a".into(),
    )];
    files.extend(
        ["swscale.h", "version.h", "version_major.h"]
            .into_iter()
            .map(|name| {
                (
                    source.join("libswscale").join(name),
                    format!("include/libswscale/{name}"),
                )
            }),
    );
    files.extend(
        ["COPYING.LGPLv2.1", "LICENSE.md"]
            .into_iter()
            .map(|name| (source.join(name), format!("share/licenses/swscale/{name}"))),
    );
    for (origin, relative) in files {
        let target = install.join(&relative);
        fs::create_dir_all(target.parent().ok_or("missing install parent")?)?;
        fs::copy(origin, &target)?;
        receipt["files"][&relative] = digest(&target)?.into();
    }
    write_json(&record, &receipt)?;
    Ok(receipt)
}
/// Build only libswscale against the unchanged verified FFmpeg wheel headers.
pub fn build_swscale(
    output: &Path,
    install: &Path,
    source_archive: Option<&Path>,
    tools: &Toolchain,
    runner: &mut impl Runner,
) -> Result<Value> {
    scaler(
        output,
        install,
        source_archive,
        tools,
        runner,
        &ScalerPin {
            version: SWSCALE_VERSION,
            sha256: SWSCALE_SHA256,
            url: SWSCALE_URL,
        },
    )
}
pub struct Options {
    pub root: PathBuf,
    pub output: PathBuf,
    pub archive_directory: Option<PathBuf>,
    pub swscale_archive: Option<PathBuf>,
    pub tools: Toolchain,
}
/// Complete staging. Configure/make are native tools; no wheel launcher is run.
pub fn stage(options: &Options, runner: &mut impl Runner) -> Result<Value> {
    let output = if options.output.is_absolute() {
        options.output.clone()
    } else {
        std::env::current_dir()?.join(&options.output)
    };
    let usage = space(&output, 512 * 1024 * 1024)?;
    require(
        fs::symlink_metadata(&output).is_err_and(|e| e.kind() == io::ErrorKind::NotFound),
        "staging output already exists",
    )?;
    let lock: toml::Value = fs::read_to_string(options.root.join("uv.lock"))?.parse()?;
    let pins = packages();
    for package in &pins {
        verify_lock(&lock, package)?;
    }
    fs::create_dir_all(&output)?;
    write_json(&output.join("space.json"), &usage)?;
    let mut result = serde_json::Map::new();
    for package in pins {
        let archive = output.join(package.filename());
        if let Some(directory) = &options.archive_directory {
            space(&output, package.size)?;
            let origin = directory.join(package.filename());
            require(
                fs::metadata(&origin)?.len() == package.size,
                "native wheel SHA256/size differs",
            )?;
            fs::copy(origin, &archive)?;
        } else {
            let command = download(&package.url(), &archive, package.size);
            write_json(
                &output.join(format!("{}-download.json", package.name)),
                &command,
            )?;
            runner.run(&command, None, None)?;
        }
        verify_archive(&archive, &package)?;
        let install = output.join(&package.name).join("install");
        let records = extract_native(&archive, &package, &install)?;
        let mut value = serde_json::to_value(&package)?;
        value["url"] = package.url().into();
        value["install"] = install.to_string_lossy().into_owned().into();
        value["files"] = serde_json::to_value(records)?;
        result.insert(package.name, value);
    }
    let scale = build_swscale(
        &output.join("swscale"),
        &output.join("ffmpeg/install"),
        options.swscale_archive.as_deref(),
        &options.tools,
        runner,
    )?;
    result.insert("swscale".into(), scale);
    let mut required = vec![
        (output.join("ffmpeg/install/include/libswscale/swscale.h")),
        output.join("ffmpeg/install/include/libavcodec/avcodec.h"),
        output.join("libyuv/install/lib/libyuv.a"),
        output.join("libyuv/install/include/libyuv.h"),
    ];
    required.extend(
        [
            "avcodec",
            "avformat",
            "avutil",
            "swscale",
            "swresample",
            "x264",
            "z",
            "va",
            "va-drm",
            "drm",
        ]
        .into_iter()
        .map(|name| output.join(format!("ffmpeg/install/lib/lib{name}.a"))),
    );
    for path in required {
        require(
            path.is_file(),
            format!("missing staged provider: {}", path.display()),
        )?;
    }
    write_json(&output.join("receipt.json"), &result)?;
    Ok(
        json!({"status":"PASS","FFMPEG_DIR":result["ffmpeg"]["install"],"ENCODER_LIBYUV_LIB":output.join("libyuv/install/lib")}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archive(path: &Path, files: &[(&str, &[u8])]) {
        let compressed = xz2::write::XzEncoder::new(File::create(path).unwrap(), 1);
        let mut tar = tar::Builder::new(compressed);
        for (name, bytes) in files {
            let mut h = tar::Header::new_ustar();
            h.set_path(name).unwrap();
            h.set_mode(if name.ends_with("configure") {
                0o755
            } else {
                0o644
            });
            h.set_size(bytes.len() as u64);
            h.set_cksum();
            tar.append(&h, *bytes).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }
    struct OwnedRunner {
        commands: Vec<Vec<String>>,
        fail: bool,
    }
    impl Runner for OwnedRunner {
        fn run(&mut self, args: &[String], cwd: Option<&Path>, log: Option<&Path>) -> Result<()> {
            self.commands.push(args.to_vec());
            require(log.is_some(), "missing build log")?;
            if self.fail {
                return Err("owned command failure".into());
            }
            if args[0] == "make" {
                let target = cwd
                    .ok_or("missing working directory")?
                    .join("libswscale/libswscale.a");
                fs::create_dir_all(target.parent().unwrap())?;
                fs::write(target, b"owned static scaler fixture")?;
            }
            Ok(())
        }
    }
    #[test]
    fn scaler_commands_license_receipt_and_exact_headers() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.tar.xz");
        archive(
            &source,
            &[
                ("ffmpeg-7.1/configure", b"owned configure fixture"),
                ("ffmpeg-7.1/libavutil/version.h", b"version"),
                ("ffmpeg-7.1/libavutil/pixfmt.h", b"pixels"),
                ("ffmpeg-7.1/libswscale/swscale.h", b"api"),
                ("ffmpeg-7.1/libswscale/version.h", b"scaler version"),
                ("ffmpeg-7.1/libswscale/version_major.h", b"major"),
                ("ffmpeg-7.1/COPYING.LGPLv2.1", b"owned LGPL license fixture"),
                ("ffmpeg-7.1/LICENSE.md", b"owned license notices"),
            ],
        );
        let install = temp.path().join("install");
        fs::create_dir_all(install.join("include/libavutil")).unwrap();
        fs::write(install.join("include/libavutil/version.h"), b"version").unwrap();
        fs::write(install.join("include/libavutil/pixfmt.h"), b"pixels").unwrap();
        let hash = digest(&source).unwrap();
        let pin = ScalerPin {
            version: "7.1",
            sha256: &hash,
            url: "https://fixture.invalid/pinned-source",
        };
        let tools = Toolchain {
            cc: "owned-cc".into(),
            ar: "owned-ar".into(),
            ranlib: "owned-ranlib".into(),
        };
        let mut runner = OwnedRunner {
            commands: Vec::new(),
            fail: false,
        };
        let output = temp.path().join("build");
        let receipt = scaler(&output, &install, Some(&source), &tools, &mut runner, &pin).unwrap();
        assert_eq!(runner.commands.len(), 2);
        assert_eq!(
            runner.commands[1],
            ["make", "-j2", "libswscale/libswscale.a"]
        );
        assert_eq!(
            runner.commands[0][0],
            output.join("ffmpeg-7.1/configure").to_str().unwrap()
        );
        for option in [
            "--cc=owned-cc",
            "--ar=owned-ar",
            "--ranlib=owned-ranlib",
            "--enable-cross-compile",
            "--disable-avcodec",
            "--disable-avformat",
            "--enable-swscale",
        ] {
            assert!(runner.commands[0].iter().any(|s| s == option));
        }
        assert_eq!(receipt["source_sha256"], hash);
        assert_eq!(receipt["license"], "LGPL-2.1-or-later");
        assert_eq!(receipt["files"].as_object().unwrap().len(), 6);
        for (name, hash) in receipt["files"].as_object().unwrap() {
            assert_eq!(hash.as_str().unwrap(), digest(&install.join(name)).unwrap());
        }
        assert_eq!(
            fs::read(install.join("share/licenses/swscale/COPYING.LGPLv2.1")).unwrap(),
            b"owned LGPL license fixture"
        );
        let recorded: Value =
            serde_json::from_slice(&fs::read(output.join("receipt.json")).unwrap()).unwrap();
        assert_eq!(recorded, receipt);
        let mut failing = OwnedRunner {
            commands: Vec::new(),
            fail: true,
        };
        let bad = temp.path().join("failed-build");
        assert!(scaler(&bad, &install, Some(&source), &tools, &mut failing, &pin).is_err());
        assert_eq!(failing.commands.len(), 1);
        let failed: Value =
            serde_json::from_slice(&fs::read(bad.join("receipt.json")).unwrap()).unwrap();
        assert_eq!(failed["files"], json!({}));
    }
    #[test]
    fn source_archive_validation_precedes_extraction() {
        let temp = tempfile::tempdir().unwrap();
        let duplicate = temp.path().join("duplicate.tar.xz");
        archive(
            &duplicate,
            &[("ffmpeg-7.1/owned", b"one"), ("ffmpeg-7.1/owned", b"two")],
        );
        let output = temp.path().join("output");
        fs::create_dir(&output).unwrap();
        assert!(extract_source(&duplicate, &output, "7.1").is_err());
        assert!(fs::read_dir(&output).unwrap().next().is_none());
        let outside = temp.path().join("outside.tar.xz");
        archive(&outside, &[("other-root/owned", b"one")]);
        assert!(extract_source(&outside, &output, "7.1").is_err());
        assert!(fs::read_dir(&output).unwrap().next().is_none());
        assert!(link_target("ffmpeg-7.1/dir/link", "../../../outside", false, "7.1").is_err());
        assert!(link_target("ffmpeg-7.1/dir/link", "/etc/passwd", false, "7.1").is_err());
        assert_eq!(
            link_target("ffmpeg-7.1/dir/link", "../owned", false, "7.1").unwrap(),
            "ffmpeg-7.1/owned"
        );
    }
}
