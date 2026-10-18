//! Build-time card and RadarCAN assets. Generation uses native source-derived
//! DBC policies over private copied inputs; it never executes Python.
pub use crate::dbc_generator::Error;
use crate::dbc_generator::{self, Program};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
};

mod provenance;
type Result<T> = std::result::Result<T, Error>;

// Required card filenames from the original packager, not the complete generator
// registry. Checked-in nonmembers must be copied even if another native policy
// could generate a differently formatted version of them.
const GENERATED: &[&str] = &[
    "nissan_x_trail_2017_generated.dbc",
    "nissan_leaf_2018_generated.dbc",
    "chrysler_pacifica_2017_hybrid_generated.dbc",
    "chrysler_ram_dt_generated.dbc",
    "chrysler_ram_hd_generated.dbc",
    "subaru_global_2017_generated.dbc",
    "subaru_global_2020_hybrid_generated.dbc",
    "subaru_forester_2017_generated.dbc",
    "subaru_outback_2015_generated.dbc",
    "subaru_outback_2019_generated.dbc",
    "toyota_new_mc_pt_generated.dbc",
    "toyota_nodsu_pt_generated.dbc",
    "toyota_secoc_pt_generated.dbc",
    "toyota_tnga_k_pt_generated.dbc",
    "honda_accord_2018_can_generated.dbc",
    "honda_civic_hatchback_ex_2017_can_generated.dbc",
    "honda_civic_ex_2022_can_generated.dbc",
    "honda_crv_ex_2017_can_generated.dbc",
    "honda_crv_ex_2017_body_generated.dbc",
    "acura_rdx_2020_can_generated.dbc",
    "honda_insight_ex_2019_can_generated.dbc",
    "acura_ilx_2016_can_generated.dbc",
    "honda_crv_touring_2016_can_generated.dbc",
    "honda_crv_executive_2016_can_generated.dbc",
    "honda_fit_ex_2018_can_generated.dbc",
    "honda_odyssey_exl_2018_generated.dbc",
    "honda_odyssey_extreme_edition_2018_china_can_generated.dbc",
    "acura_rdx_2018_can_generated.dbc",
    "honda_civic_touring_2016_can_generated.dbc",
];

fn io_error(error: io::Error, path: &Path) -> Error {
    let kind = match error.raw_os_error() {
        Some(2) => "FileNotFoundError",
        Some(13) | Some(1) => "PermissionError",
        Some(17) => "FileExistsError",
        Some(20) => "NotADirectoryError",
        Some(21) => "IsADirectoryError",
        _ => "OSError",
    };
    Error::new(
        kind,
        error.raw_os_error(),
        &format!("{}: {error}", path.display()),
    )
}
fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| io_error(error, path))
}
fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(read(path)?)))
}
fn reject_fifo(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.file_type().is_fifo() {
        return Err(Error::new(
            "SpecialFileError",
            None,
            &format!("{} is a named pipe", path.display()),
        ));
    }
    Ok(())
}
fn copy(from: &Path, to: &Path) -> Result<()> {
    // shutil checks both paths before opening either, following symlinks.
    // Failed stat calls are left to open so its original failure phase remains.
    for path in [from, to] {
        if let Ok(metadata) = fs::metadata(path) {
            reject_fifo(path, &metadata)?;
        }
    }
    // A path can become a FIFO after stat. Nonblocking opens and descriptor
    // checks prevent that replacement from hanging the packager.
    let flags = i32::try_from(rustix::fs::OFlags::NONBLOCK.bits())
        .map_err(|_| Error::new("OverflowError", None, "O_NONBLOCK exceeds i32"))?;
    // shutil.copyfile copies data rather than source permissions. fs::copy would
    // incorrectly make a new packaged file read-only when the input is read-only.
    let mut input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(from)
        .map_err(|error| io_error(error, from))?;
    let metadata = input.metadata().map_err(|error| io_error(error, from))?;
    reject_fifo(from, &metadata)?;
    if metadata.is_dir() {
        return Err(Error::new(
            "IsADirectoryError",
            Some(21),
            &from.display().to_string(),
        ));
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(flags)
        .open(to)
        .map_err(|error| io_error(error, to))?;
    reject_fifo(to, &output.metadata().map_err(|error| io_error(error, to))?)?;
    io::copy(&mut input, &mut output).map_err(|error| io_error(error, to))?;
    Ok(())
}
fn mkdir(path: &Path) -> Result<()> {
    fs::create_dir(path).map_err(|error| io_error(error, path))
}
fn original_absolute(path: &Path) -> Result<PathBuf> {
    // pathlib removes redundant '.' and '/' when constructing a Path, while
    // absolute() leaves symlinks and '..' unresolved.
    let mut result = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().map_err(|error| io_error(error, Path::new(".")))?
    };
    for component in path.components() {
        if component != Component::CurDir {
            result.push(component.as_os_str());
        }
    }
    Ok(result)
}
fn original_path(path: &Path) -> PathBuf {
    let result: PathBuf = path
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();
    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}
fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    // pathlib orders component tuples, decoding filesystem bytes with
    // surrogateescape. Whole-path strings and raw OsStr bytes order differently.
    paths.sort_by_cached_key(|path| {
        path.components()
            .map(|part| crate::python_fs::codepoints(part.as_os_str()))
            .collect::<Vec<_>>()
    });
    paths
}
fn glob(directory: &Path, suffix: &[u8]) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound
                    | io::ErrorKind::NotADirectory
                    | io::ErrorKind::PermissionDenied
            ) =>
        {
            return Ok(Vec::new())
        }
        Err(error) => return Err(io_error(error, directory)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io_error(error, directory))?;
        if entry.file_name().as_bytes().ends_with(suffix) {
            paths.push(entry.path());
        }
    }
    Ok(sorted(paths))
}
fn radar_inputs(source: &Path) -> Result<Vec<PathBuf>> {
    fn walk(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound
                        | io::ErrorKind::NotADirectory
                        | io::ErrorKind::PermissionDenied
                ) =>
            {
                return Ok(())
            }
            Err(error) => return Err(io_error(error, directory)),
        };
        for entry in entries {
            let entry = entry.map_err(|error| io_error(error, directory))?;
            let path = entry.path();
            if path
                .components()
                .any(|part| part.as_os_str() == "__pycache__")
                || path.extension().is_some_and(|e| e == "pyc")
            {
                continue;
            }
            if entry
                .file_type()
                .map_err(|error| io_error(error, &path))?
                .is_dir()
            {
                walk(&path, paths)?;
            } else if path.is_file() {
                paths.push(path);
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    walk(source, &mut paths)?;
    Ok(sorted(paths))
}
fn growth(inputs: &[PathBuf], headroom: u64) -> Result<u64> {
    let mut bytes = 0u64;
    for path in inputs {
        let size = fs::metadata(path)
            .map_err(|error| io_error(error, path))?
            .len();
        bytes = bytes
            .checked_add(size)
            .ok_or_else(|| Error::new("OverflowError", None, "DBC input sizes exceed u64"))?;
    }
    bytes
        .checked_mul(4)
        .and_then(|v| v.checked_add(headroom))
        .ok_or_else(|| Error::new("OverflowError", None, "DBC growth exceeds u64"))
}
fn space(destination: &Path, growth: u64, card: bool) -> Result<u64> {
    // Unlike the general build guard, the original packagers require the actual
    // immediate parent to exist. Never walk to a more distant existing ancestor.
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let info = rustix::fs::statvfs(parent).map_err(|error| io_error(error.into(), parent))?;
    let free = info
        .f_bavail
        .checked_mul(info.f_frsize)
        .ok_or_else(|| Error::new("OverflowError", None, "free disk size exceeds u64"))?;
    let required = openpilot_disk_space::RESERVE_BYTES
        .checked_add(growth)
        .ok_or_else(|| Error::new("OverflowError", None, "DBC reserve exceeds u64"))?;
    if free < required {
        return Err(if card {
            Error::new(
                "InsufficientSpaceError",
                None,
                &format!("card assets require {required} free bytes; found {free}"),
            )
        } else {
            Error::new(
                "OSError",
                None,
                &format!("RadarCAN assets need {required} free bytes; found {free}"),
            )
        });
    }
    Ok(free)
}

fn json_error(error: impl std::fmt::Display) -> Error {
    Error::new("ValueError", None, &error.to_string())
}
fn parsed(text: &str) -> Result<JsonValue> {
    JsonValue::parse(text).map_err(json_error)
}
fn object(fields: impl IntoIterator<Item = (JsonValue, JsonValue)>) -> Result<JsonValue> {
    let mut text = String::from("{");
    for (index, (key, value)) in fields.into_iter().enumerate() {
        if index != 0 {
            text.push_str(", ");
        }
        text.push_str(&key.to_json().map_err(json_error)?);
        text.push_str(": ");
        text.push_str(&value.to_json().map_err(json_error)?);
    }
    text.push('}');
    parsed(&text)
}
fn manifest(fields: Vec<(&str, JsonValue)>) -> Result<JsonValue> {
    object(
        fields
            .into_iter()
            .map(|(key, value)| (JsonValue::text(key), value)),
    )
}
fn hashes(paths: &[PathBuf], relative: &Path) -> Result<JsonValue> {
    let mut values = Vec::with_capacity(paths.len());
    for path in paths {
        let name = path
            .strip_prefix(relative)
            .map_err(|e| Error::new("ValueError", None, &e.to_string()))?;
        values.push((
            crate::python_fs::json_value(name.as_os_str()),
            JsonValue::text(&hash(path)?),
        ));
    }
    object(values)
}
fn save(path: &Path, manifest: &JsonValue) -> Result<JsonValue> {
    // Shared Python JSON writer preserves order, indent=2, ensure_ascii=True,
    // and raw surrogateescape keys in both source and asset maps.
    let mut bytes = crate::control_tools_policy_check::json::pretty_with_limit(manifest, 0, 995)
        .map_err(json_error)?
        .into_bytes();
    bytes.push(b'\n');
    fs::write(path, &bytes).map_err(|error| io_error(error, path))?;
    Ok(manifest.clone())
}

/// Publish card's new package atomically, preserving checked-in nonmembers of
/// the original 29-name required generated set.
pub fn card(repository: &Path, destination: &Path) -> Result<JsonValue> {
    let repository = crate::path_resolve::resolve_non_strict(repository).map_err(|error| {
        if let Some(error) = error.downcast_ref::<io::Error>() {
            let copied = error
                .raw_os_error()
                .map(io::Error::from_raw_os_error)
                .unwrap_or_else(|| io::Error::from(error.kind()));
            io_error(copied, repository)
        } else {
            Error::new("RuntimeError", None, &error.to_string())
        }
    })?;
    let destination = original_absolute(destination)?;
    if destination.exists() {
        return Err(Error::new(
            "FileExistsError",
            None,
            &destination.display().to_string(),
        ));
    }
    let source = repository.join("opendbc_repo/opendbc/dbc");
    let originals = glob(&source, b".dbc")?;
    let generator = source.join("generator");
    // The native generator replaces generator.py. Its absence is expected after
    // retirement, but a missing input directory remains the original I/O error.
    fs::metadata(&generator).map_err(|error| io_error(error, &generator))?;
    let mut inputs = Vec::new();
    let old_generator = generator.join("generator.py");
    match fs::symlink_metadata(&old_generator) {
        Ok(_) => inputs.push(old_generator),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error, &old_generator)),
    }
    for brand in ["nissan", "chrysler", "subaru", "toyota", "honda"] {
        inputs.extend(glob(&generator.join(brand), b".dbc")?);
        inputs.extend(glob(&generator.join(brand), b".py")?);
    }
    let mut all_inputs = originals.clone();
    all_inputs.extend(inputs.iter().cloned());
    space(&destination, growth(&all_inputs, 16 * 1024 * 1024)?, true)?;
    let source_sha256 = hashes(&all_inputs, &repository)?;
    let parent = destination
        .parent()
        .expect("absolute destination has a parent");
    let temporary = tempfile::Builder::new()
        .prefix("card-assets-")
        .tempdir_in(parent)
        .map_err(|error| io_error(error, parent))?;
    let staged_inputs = temporary.path().join("generator");
    let assets = temporary.path().join("dbc");
    mkdir(&staged_inputs)?;
    mkdir(&assets)?;
    for original in inputs {
        let copied =
            staged_inputs.join(original.strip_prefix(&generator).expect("generator input"));
        fs::create_dir_all(copied.parent().expect("input parent"))
            .map_err(|error| io_error(error, &copied))?;
        copy(&original, &copied)?;
    }
    let programs = if staged_inputs.join("chrysler").is_dir() {
        &[Program::ChryslerRam][..]
    } else {
        &[]
    };
    dbc_generator::create_all(&assets, &staged_inputs, programs)
        .map_err(|error| Error::new("CalledProcessError", None, &error.to_string()))?;
    for original in originals {
        if !GENERATED
            .iter()
            .any(|name| original.file_name().is_some_and(|value| value == *name))
        {
            copy(
                &original,
                &assets.join(original.file_name().expect("DBC filename")),
            )?;
        }
    }
    for name in GENERATED {
        let path = assets.join(name);
        fs::metadata(&path).map_err(|error| io_error(error, &path))?;
    }
    let manifest = manifest(vec![
        ("format_version", parsed("1")?),
        ("build_time_only", parsed("true")?),
        ("source_sha256", source_sha256),
        ("asset_sha256", hashes(&glob(&assets, b".dbc")?, &assets)?),
        (
            "generator_provenance",
            parsed(&serde_json::to_string(&provenance::native()?).map_err(json_error)?)?,
        ),
    ])?;
    let value = save(&assets.join("card-assets.json"), &manifest)?;
    fs::rename(&assets, &destination).map_err(|error| io_error(error, &destination))?;
    Ok(value)
}

/// RadarCAN deliberately creates the destination before generation, matching
/// the original empty-directory evidence on generation failure.
pub fn radarcan(source_dbc: &Path, destination: &Path) -> Result<JsonValue> {
    let source = original_path(source_dbc);
    let destination = original_path(destination);
    let inputs = radar_inputs(&source)?;
    let growth_bound = growth(&inputs, 64 * 1024 * 1024)?;
    let free_before = space(&destination, growth_bound, false)?;
    mkdir(&destination)?;
    let generated = dbc_generator::generate_all(&source.join("generator"))?;
    for original in glob(&source, b".dbc")? {
        if !generated.iter().any(|(name, _)| {
            original
                .file_stem()
                .is_some_and(|stem| stem == name.as_str())
        }) {
            copy(
                &original,
                &destination.join(original.file_name().expect("DBC filename")),
            )?;
        }
    }
    for (name, content) in generated {
        let path = destination.join(format!("{name}.dbc"));
        fs::write(&path, content).map_err(|error| io_error(error, &path))?;
    }
    let manifest = manifest(vec![
        ("build_time_only", parsed("true")?),
        ("free_before", parsed(&free_before.to_string())?),
        ("growth_bound", parsed(&growth_bound.to_string())?),
        ("source_sha256", hashes(&inputs, &source)?),
        (
            "asset_sha256",
            hashes(&glob(&destination, b".dbc")?, &destination)?,
        ),
        (
            "generator_provenance",
            parsed(&serde_json::to_string(&provenance::native()?).map_err(json_error)?)?,
        ),
    ])?;
    save(&destination.join("radarcan-assets.json"), &manifest)
}

pub fn card_cli() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(Error::new(
            "Usage",
            None,
            "usage: prepare-card-assets REPOSITORY NEW_DBC_DIRECTORY",
        ));
    }
    let result = card(Path::new(&args[0]), Path::new(&args[1]))?;
    println!(
        "Prepared {} DBC assets with native Rust generator",
        asset_count(&result)
    );
    Ok(())
}
pub fn radarcan_cli() -> Result<()> {
    let destination = std::env::args_os().nth(1).ok_or_else(|| {
        Error::new(
            "Usage",
            None,
            "usage: prepare-radarcan-assets NEW_DIRECTORY",
        )
    })?;
    let repository = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let result = radarcan(
        &repository.join("opendbc_repo/opendbc/dbc"),
        Path::new(&destination),
    )?;
    println!(
        "Prepared {} original RadarCAN DBCs with native Rust generator",
        asset_count(&result)
    );
    Ok(())
}

fn asset_count(manifest: &JsonValue) -> usize {
    match manifest
        .get("asset_sha256")
        .expect("manifest asset field")
        .view()
    {
        JsonView::Object(entries) => entries.len(),
        _ => unreachable!("manifest asset map"),
    }
}
