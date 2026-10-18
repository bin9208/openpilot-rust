//! Native DBC composition and procedural generation, ported from opendbc's MIT
//! generator. See dbc_generator/LICENSE.opendbc for retained original licensing.
//! DBC files are mutable inputs. Captured generated outputs are tests only.
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
mod chrysler;
mod cli;
mod mando;
mod pins;
mod procedural;
pub use cli::{generator_cli, stage_cli};

#[derive(Debug)]
pub struct Error {
    kind: String,
    errno: Option<i32>,
    message: String,
    path_bytes: Option<Box<[u8]>>,
    decode_bytes: Option<Box<[u8]>>,
    encode_bytes: Option<Box<[u8]>>,
}
impl Error {
    pub fn new(kind: &str, errno: Option<i32>, message: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            errno,
            message: message.to_owned(),
            path_bytes: None,
            decode_bytes: None,
            encode_bytes: None,
        }
    }
    /// Optional machine boundary for pending callers that must reconstruct
    /// Python filesystem/Unicode exception types without parsing Display text.
    pub fn json_line(&self) -> String {
        let mut value =
            serde_json::json!({"kind":self.kind,"errno":self.errno,"message":self.message});
        for (key, bytes) in [
            ("path_bytes", &self.path_bytes),
            ("decode_bytes", &self.decode_bytes),
            ("encode_bytes", &self.encode_bytes),
        ] {
            if let Some(bytes) = bytes {
                value[key] = serde_json::json!(bytes);
            }
        }
        serde_json::to_string(&value).expect("error strings and byte arrays") + "\n"
    }
    pub(super) fn at_path(mut self, path: &Path) -> Self {
        self.path_bytes = Some(path.as_os_str().as_bytes().into());
        self
    }
    pub(super) fn with_decode(mut self, bytes: Vec<u8>) -> Self {
        self.decode_bytes = Some(bytes.into_boxed_slice());
        self
    }
    fn with_encode(mut self, bytes: Vec<u8>) -> Self {
        self.encode_bytes = Some(bytes.into_boxed_slice());
        self
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn raw_os_error(&self) -> Option<i32> {
        self.errno
    }
    pub(super) fn io(error: io::Error, path: &Path) -> Self {
        let kind = match error.raw_os_error() {
            Some(2) => "FileNotFoundError",
            Some(17) => "FileExistsError",
            Some(20) => "NotADirectoryError",
            Some(21) => "IsADirectoryError",
            Some(13) => "PermissionError",
            _ => "OSError",
        };
        Self::new(
            kind,
            error.raw_os_error(),
            &format!("{}: {error}", path.display()),
        )
        .at_path(path)
    }
    fn argument(message: &str) -> Self {
        Self::new("ArgumentError", None, message)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    ChryslerRam,
    TeslaBosch,
    TeslaContinental,
    HyundaiCorner180,
    HyundaiCorner235,
    HyundaiCorner430,
    HyundaiCanfd,
    HyundaiDenso,
    HyundaiMandoCorner,
    HyundaiMandoFront,
    RivianMandoFront,
}
impl Program {
    pub const ALL: &'static [Self] = &[
        Self::ChryslerRam,
        Self::TeslaBosch,
        Self::TeslaContinental,
        Self::HyundaiCorner180,
        Self::HyundaiCorner235,
        Self::HyundaiCorner430,
        Self::HyundaiCanfd,
        Self::HyundaiDenso,
        Self::HyundaiMandoCorner,
        Self::HyundaiMandoFront,
        Self::RivianMandoFront,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::ChryslerRam => "chrysler-ram",
            Self::TeslaBosch => "tesla-bosch",
            Self::TeslaContinental => "tesla-continental",
            Self::HyundaiCorner180 => "hyundai-corner-180",
            Self::HyundaiCorner235 => "hyundai-corner-235",
            Self::HyundaiCorner430 => "hyundai-corner-430",
            Self::HyundaiCanfd => "hyundai-canfd",
            Self::HyundaiDenso => "hyundai-denso",
            Self::HyundaiMandoCorner => "hyundai-mando-corner",
            Self::HyundaiMandoFront => "hyundai-mando-front",
            Self::RivianMandoFront => "rivian-mando-front",
        }
    }
    pub fn parse(name: &str) -> Result<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|p| p.name() == name)
            .ok_or_else(|| Error::argument(&format!("unknown native program {name:?}")))
    }
    pub fn brand(self) -> &'static str {
        match self {
            Self::ChryslerRam => "chrysler",
            Self::TeslaBosch | Self::TeslaContinental => "tesla",
            Self::RivianMandoFront => "rivian",
            _ => "hyundai",
        }
    }
    pub fn output(self) -> Option<&'static str> {
        Some(match self {
            Self::ChryslerRam => return None,
            Self::TeslaBosch => "tesla_radar_bosch.dbc",
            Self::TeslaContinental => "tesla_radar_continental.dbc",
            Self::HyundaiCorner180 => "hyundai_canfd_corner_radar_180.dbc",
            Self::HyundaiCorner235 => "hyundai_canfd_corner_radar_235.dbc",
            Self::HyundaiCorner430 => "hyundai_canfd_corner_radar_430.dbc",
            Self::HyundaiCanfd => "hyundai_canfd_radar.dbc",
            Self::HyundaiDenso => "hyundai_kia_denso_front_radar.dbc",
            Self::HyundaiMandoCorner => "hyundai_kia_mando_corner_radar.dbc",
            Self::HyundaiMandoFront => "hyundai_kia_mando_front_radar.dbc",
            Self::RivianMandoFront => "rivian_mando_front_radar.dbc",
        })
    }
}
/// Final filenames supplied to SCons from the same live writer registry.
pub fn program_outputs() -> Vec<String> {
    Program::ALL
        .iter()
        .filter_map(|p| p.output())
        .map(|s| s.replace(".dbc", "_generated.dbc"))
        .collect()
}

pub fn read_dbc(source: &Path, filename: &Path) -> Result<String> {
    let path = source.join(filename);
    let bytes = fs::read(&path).map_err(|e| Error::io(e, &path))?;
    let text = String::from_utf8(bytes).map_err(|e| {
        Error::new(
            "UnicodeDecodeError",
            None,
            &format!("{}: {e}", path.display()),
        )
        .at_path(&path)
        .with_decode(e.into_bytes())
    })?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}
fn replace_suffix(value: &OsStr) -> OsString {
    let data = value.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if data[index..].starts_with(b".dbc") {
            result.extend_from_slice(b"_generated.dbc");
            index += 4;
        } else {
            result.push(data[index]);
            index += 1;
        }
    }
    OsString::from_vec(result)
}
fn put(out: &mut File, path: &Path, text: &str) -> Result<()> {
    out.write_all(text.as_bytes())
        .map_err(|e| Error::io(e, path))
}
pub fn create_dbc(source: &Path, filename: &Path, output: &Path) -> Result<PathBuf> {
    let text = read_dbc(source, filename)?;
    let pattern = Regex::new("CM_ \"IMPORT (.*?)\";\\n").expect("constant original include regex");
    let target = output.join(replace_suffix(filename.as_os_str()));
    let mut out = File::create(&target).map_err(|e| Error::io(e, &target))?;
    put(
        &mut out,
        &target,
        "CM_ \"AUTOGENERATED FILE, DO NOT EDIT\";\n",
    )?;
    for capture in pattern.captures_iter(&text) {
        let name = &capture[1];
        put(
            &mut out,
            &target,
            &format!("\n\nCM_ \"Imported file {name} starts here\";\n"),
        )?;
        put(&mut out, &target, &read_dbc(source, Path::new(name))?)?;
    }
    let name = filename.to_str().ok_or_else(|| {
        let mut header = b"\nCM_ \"".to_vec();
        header.extend_from_slice(filename.as_os_str().as_bytes());
        header.extend_from_slice(b" starts here\";\n");
        Error::new(
            "UnicodeEncodeError",
            None,
            "filename contains surrogateescaped bytes in output text",
        )
        .with_encode(header)
    })?;
    put(
        &mut out,
        &target,
        &format!("\nCM_ \"{name} starts here\";\n"),
    )?;
    put(&mut out, &target, &pattern.replace_all(&text, ""))?;
    Ok(target)
}

pub fn render_program(program: Program, directory: &Path) -> Result<()> {
    if program == Program::ChryslerRam {
        return chrysler::write(directory);
    }
    let path = directory.join(program.output().expect("non-Chrysler writer output"));
    let mut out = File::create(&path).map_err(|e| Error::io(e, &path))?;
    let result = match program {
        Program::TeslaBosch => procedural::tesla_radar_bosch(&mut out),
        Program::TeslaContinental => procedural::tesla_radar_continental(&mut out),
        Program::HyundaiCorner180 => procedural::hyundai_canfd_corner_radar_180(&mut out),
        Program::HyundaiCorner235 => procedural::hyundai_canfd_corner_radar_235(&mut out),
        Program::HyundaiCorner430 => procedural::hyundai_canfd_corner_radar_430(&mut out),
        Program::HyundaiCanfd => procedural::hyundai_canfd_radar(&mut out),
        Program::HyundaiDenso => procedural::hyundai_kia_denso_front_radar(&mut out),
        Program::HyundaiMandoCorner => mando::write(&mut out),
        Program::HyundaiMandoFront => procedural::hyundai_kia_mando_front_radar(&mut out),
        Program::RivianMandoFront => procedural::rivian_mando_front_radar(&mut out),
        Program::ChryslerRam => unreachable!(),
    };
    result.map_err(|e| Error::io(e, &path))
}
pub fn tesla_point(address: i64, name: &str) -> String {
    let mut data = Vec::new();
    procedural::tesla_point_to(&mut data, address, name).expect("Vec write");
    String::from_utf8(data).expect("native syntax UTF8")
}
pub fn tesla_values(address: i64) -> String {
    let mut data = Vec::new();
    procedural::tesla_values_to(&mut data, address).expect("Vec write");
    String::from_utf8(data).expect("native syntax UTF8")
}
pub fn corner_object(family: u16, address: i64) -> Result<String> {
    let mut data = Vec::new();
    match family {
        180 => procedural::corner_180_to(&mut data, address),
        235 => procedural::corner_235_to(&mut data, address),
        430 => procedural::corner_430_to(&mut data, address),
        _ => return Err(Error::argument("unknown corner radar family")),
    }
    .expect("Vec write");
    Ok(String::from_utf8(data).expect("native syntax UTF8"))
}

fn clear_old(output: &Path) -> Result<()> {
    // Intentional inherited bug correction: output is a literal owned directory,
    // not a glob pattern that can delete a sibling's generated files.
    if let Ok(entries) = fs::read_dir(output) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let bytes = name.as_bytes();
            if !bytes.starts_with(b".") && bytes.ends_with(b"_generated.dbc") {
                let path = entry.path();
                fs::remove_file(&path).map_err(|e| Error::io(e, &path))?;
            }
        }
    }
    Ok(())
}
fn admit_scripts(source: &Path) -> Result<()> {
    // Absence of retired originals is allowed. Present originals must remain
    // exact, including dangling symlinks (which are not actual absence).
    for (relative, expected) in pins::SOURCES {
        let path = source.join(relative);
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(Error::io(e, &path)),
            Ok(_) => {}
        }
        let bytes = fs::read(&path).map_err(|e| Error::io(e, &path))?;
        if format!("{:x}", Sha256::digest(bytes)) != *expected {
            return Err(Error::new(
                "UnsupportedProgram",
                None,
                &format!("changed retired Python policy {}", path.display()),
            ));
        }
    }
    if let Ok(brands) = fs::read_dir(source) {
        for brand in brands.flatten() {
            if brand.file_name().as_bytes().starts_with(b".") || !brand.path().is_dir() {
                continue;
            }
            let Ok(files) = fs::read_dir(brand.path()) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name();
                if name.as_bytes().starts_with(b".") || !name.as_bytes().ends_with(b".py") {
                    continue;
                }
                let relative = PathBuf::from(brand.file_name()).join(name);
                if !pins::SOURCES
                    .iter()
                    .any(|(known, _)| Path::new(known) == relative)
                {
                    return Err(Error::new(
                        "UnsupportedProgram",
                        None,
                        &format!(
                            "custom Python program {} requires native conversion",
                            relative.display()
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}
fn compose_walk(source: &Path, output: &Path, at_root: bool) -> Result<()> {
    // os.walk's default onerror ignores unreadable/nonexistent directories.
    let Ok(entries) = fs::read_dir(source) else {
        return Ok(());
    };
    let mut directories = Vec::new();
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| Error::io(e, &path))?;
        if kind.is_dir() {
            directories.push(path);
        } else if !(kind.is_symlink() && path.is_dir()) {
            files.push(entry.file_name());
        }
    }
    if !at_root {
        for name in files {
            let bytes = name.as_bytes();
            if !bytes.starts_with(b"_") && bytes.ends_with(b".dbc") {
                create_dbc(source, Path::new(&name), output)?;
            }
        }
    }
    for directory in directories {
        compose_walk(&directory, output, false)?;
    }
    Ok(())
}
pub fn create_all(output: &Path, source: &Path, programs: &[Program]) -> Result<()> {
    create_selected(output, source, programs, false)
}
/// Complete in-place generation, including native-owned writer directories
/// that Git no longer retains after retiring their sole Python source file.
pub fn create_all_complete(output: &Path, source: &Path) -> Result<()> {
    create_selected(output, source, Program::ALL, true)
}
fn create_selected(
    output: &Path,
    source: &Path,
    programs: &[Program],
    ensure_directories: bool,
) -> Result<()> {
    clear_old(output)?;
    admit_scripts(source)?;
    if ensure_directories {
        for program in programs {
            let directory = source.join(program.brand());
            fs::create_dir_all(&directory).map_err(|e| Error::io(e, &directory))?;
        }
    }
    // Paths remain literal; the former glob expanded directory metacharacters
    // and could skip the requested source or operate on unrelated siblings.
    for program in programs {
        render_program(*program, &source.join(program.brand())).map_err(|e| {
            Error::new(
                "CalledProcessError",
                None,
                &format!("native {}: {e}", program.name()),
            )
        })?;
    }
    compose_walk(source, output, true)
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target).map_err(|e| Error::io(e, target))?;
    for entry in fs::read_dir(source).map_err(|e| Error::io(e, source))? {
        let entry = entry.map_err(|e| Error::io(e, source))?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| Error::io(e, &from))?;
        }
    }
    Ok(())
}
pub fn generate_all(source: &Path) -> Result<Vec<(String, String)>> {
    let temporary =
        tempfile::tempdir().map_err(|e| Error::io(e, Path::new("temporary DBC generation")))?;
    let input = temporary.path().join("generator");
    let output = temporary.path().join("output");
    copy_tree(source, &input)?;
    fs::create_dir(&output).map_err(|e| Error::io(e, &output))?;
    create_all_complete(&output, &input)?;
    let mut generated = Vec::new();
    for entry in fs::read_dir(&output).map_err(|e| Error::io(e, &output))? {
        let entry = entry.map_err(|e| Error::io(e, &output))?;
        let name = entry.file_name();
        if name.as_bytes().starts_with(b".") || !name.as_bytes().ends_with(b"_generated.dbc") {
            continue;
        }
        let stem = Path::new(&name)
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| {
                Error::new(
                    "UnicodeEncodeError",
                    None,
                    "generated filename contains surrogate bytes",
                )
                .with_encode(name.as_bytes().to_vec())
            })?
            .to_owned();
        generated.push((stem, read_dbc(&output, Path::new(&name))?));
    }
    generated.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(generated)
}

pub struct StageReport {
    added: Vec<(String, String)>,
}
impl StageReport {
    pub fn json_line(&self) -> String {
        let pairs = self
            .added
            .iter()
            .map(|(name, hash)| {
                format!(
                    "{}: {}",
                    openpilot_log_format::JsonValue::text(name)
                        .to_json()
                        .expect("text"),
                    openpilot_log_format::JsonValue::text(hash)
                        .to_json()
                        .expect("text")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{\"added\": {{{pairs}}}, \"existing_files\": \"preserved\"}}\n")
    }
}
pub fn stage_with(
    root: &Path,
    generator: impl FnOnce() -> Result<Vec<(String, String)>>,
) -> Result<StageReport> {
    let generated = generator()?;
    let mut added = Vec::new();
    for (name, text) in generated {
        let filename = format!("{name}.dbc");
        let path = root.join(&filename);
        if path.exists() {
            continue;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| Error::io(e, &path))?;
        file.write_all(text.as_bytes())
            .map_err(|e| Error::io(e, &path))?;
        added.push((filename, format!("{:x}", Sha256::digest(text.as_bytes()))));
    }
    Ok(StageReport { added })
}
pub fn stage(root: &Path, source: &Path) -> Result<StageReport> {
    stage_with(root, || generate_all(source))
}
/// Sorted JSON mapping for remaining, explicitly pending Python callers.
pub fn generated_json(source: &Path) -> Result<String> {
    let pairs: BTreeMap<_, _> = generate_all(source)?.into_iter().collect();
    serde_json::to_string(&pairs).map_err(|e| Error::new("ValueError", None, &e.to_string()))
}
