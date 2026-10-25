//! Stage the architecture-specific json11 wheel pinned in the repository lock.
use crate::{
    disk_space, python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    time::Duration,
};
pub struct Staged {
    pub prefix: PathBuf,
    pub dependency: Value,
}
const GROWTH: u64 = 256 * 1024 * 1024;
/// Returns the original prefix/dependency pair without inventing a caller receipt.
pub fn stage(
    root: &Path,
    output: &Path,
    architecture: &str,
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<Staged> {
    fs::create_dir_all(output)?;
    disk_space::Space::new(free_bytes, GROWTH)?.require()?;
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing lock packages")?;
    let package = packages
        .iter()
        .find(|p| p.get("name").and_then(toml::Value::as_str) == Some("comma-deps-json11"))
        .ok_or("missing pinned json11 package")?;
    if !["x86_64", "aarch64"].contains(&architecture) {
        return Err(format!("unsupported native json11 architecture: {architecture}").into());
    }
    let selector = format!("manylinux_2_28_{architecture}");
    let wheel = package
        .get("wheels")
        .and_then(toml::Value::as_array)
        .ok_or("missing locked json11 wheels")?
        .iter()
        .find(|w| {
            w.get("url")
                .and_then(toml::Value::as_str)
                .is_some_and(|url| url.contains(&selector))
        })
        .ok_or("missing architecture-specific json11 wheel")?;
    let url = wheel
        .get("url")
        .and_then(toml::Value::as_str)
        .ok_or("wheel URL missing")?;
    let filename = url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or("invalid wheel filename")?;
    let archive = output.join(filename);
    let argv: Vec<OsString> = vec![
        "curl".into(),
        "-fsSL".into(),
        url.into(),
        "-o".into(),
        archive.as_os_str().to_owned(),
    ];
    let captured = runner.run(&CommandSpec::new_os(argv, Duration::from_secs(300)))?;
    if captured.code != 0 {
        return Err(format!(
            "json11 download exited {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        )
        .into());
    }
    let sha256 = digest(&archive)?;
    if wheel.get("hash").and_then(toml::Value::as_str) != Some(format!("sha256:{sha256}").as_str())
    {
        return Err("json11 wheel hash mismatch".into());
    }
    let mut wheel = zip::ZipArchive::new(File::open(&archive)?)?;
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    // Inspect the complete locked archive before creating any extracted output.
    for index in 0..wheel.len() {
        let entry = wheel.by_index(index)?;
        let path = entry.enclosed_name().ok_or("unsafe json11 ZIP path")?;
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || !paths.insert(path)
        {
            return Err("unsafe or duplicate json11 ZIP path".into());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("json11 ZIP symlink is not supported".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("json11 ZIP size overflow")?;
        if total > GROWTH {
            return Err("json11 ZIP exceeds reserved staging growth".into());
        }
    }
    let extracted = output.join("json11");
    fs::create_dir_all(&extracted)?;
    for index in 0..wheel.len() {
        let mut entry = wheel.by_index(index)?;
        let path = extracted.join(entry.enclosed_name().ok_or("unsafe json11 ZIP path")?);
        if entry.is_dir() {
            fs::create_dir_all(path)?;
        } else {
            fs::create_dir_all(path.parent().ok_or("ZIP entry parent missing")?)?;
            let mut file = File::create(path)?;
            io::copy(&mut entry, &mut file)?;
        }
    }
    let mut prefixes = fs::read_dir(&extracted)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    prefixes.retain(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".data"))
    });
    prefixes.sort();
    let prefix = prefixes
        .into_iter()
        .map(|path| path.join("purelib/json11/install"))
        .find(|path| path.is_dir())
        .ok_or("json11 install prefix missing from wheel")?;
    Ok(Staged {
        prefix,
        dependency: json!({"json11_version":package.get("version").and_then(toml::Value::as_str).ok_or("json11 version missing")?,"json11_sha256":sha256,"json11_architecture":architecture}),
    })
}
fn parse_cli(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<(PathBuf, PathBuf, String)>> {
    let mut args = args.into_iter();
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        if key == "--help" || key == "-h" {
            return Ok(None);
        }
        if !["--root", "--output", "--architecture"]
            .iter()
            .any(|known| key == *known)
        {
            return Err("unknown json11 staging argument".into());
        }
        let value = args.next().ok_or("argument requires value")?;
        if values.insert(key, value).is_some() {
            return Err("duplicate json11 staging argument".into());
        }
    }
    let root = PathBuf::from(
        values
            .remove(&OsString::from("--root"))
            .ok_or("--root required")?,
    );
    let output = PathBuf::from(
        values
            .remove(&OsString::from("--output"))
            .ok_or("--output required")?,
    );
    let architecture = values
        .remove(&OsString::from("--architecture"))
        .unwrap_or_else(|| std::env::consts::ARCH.into())
        .into_string()
        .map_err(|_| "json11 architecture must be UTF-8")?;
    Ok(Some((root, output, architecture)))
}
fn staged_json(staged: &Staged) -> Result<String> {
    // Match the former json! object's ordering with either serde_json map backend.
    // The workspace may enable preserve_order even when this package did not.
    let fields = json!({"prefix": null, "dependency": &staged.dependency});
    let encoded = fields
        .as_object()
        .expect("stager receipt object")
        .iter()
        .map(|(name, value)| -> Result<String> {
            let encoded = if name == "prefix" {
                python_fs::json_string(staged.prefix.as_os_str())?
            } else {
                serde_json::to_string(value)?
            };
            Ok(format!("{}:{encoded}", serde_json::to_string(name)?))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(format!("{{{}}}", encoded.join(",")))
}
pub fn cli() -> Result<()> {
    let Some((root, output, architecture)) = parse_cli(std::env::args_os().skip(1))? else {
        println!("Usage: stage-json11 --root ROOT --output OUTPUT [--architecture x86_64|aarch64]");
        return Ok(());
    };
    let space = disk_space::inspect(&output, GROWTH)?;
    let staged = stage(
        &root,
        &output,
        &architecture,
        &mut NativeRunner,
        space.free_bytes,
    )?;
    println!("{}", staged_json(&staged)?);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::{parse_cli, staged_json, Staged};
    use serde_json::json;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    #[test]
    fn raw_output_argument_survives_cli_and_receipt() {
        let raw = OsString::from_vec(b"json11-\xff".to_vec());
        let (_, output, arch) = parse_cli([
            "--root".into(),
            "/repo".into(),
            "--output".into(),
            raw.clone(),
            "--architecture".into(),
            "x86_64".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(output, PathBuf::from(raw.clone()));
        assert_eq!(arch, "x86_64");
        let receipt = staged_json(&Staged {
            prefix: output.join("install"),
            dependency: json!({"json11_architecture":"x86_64"}),
        })
        .unwrap();
        assert!(receipt.contains(r#""prefix":"json11-\udcff/install""#));
        assert!(parse_cli(["--output".into(), raw, "--help".into()])
            .unwrap()
            .is_none());
        let normal = Staged {
            prefix: PathBuf::from("/tmp/json11/install"),
            dependency: json!({"json11_architecture":"x86_64"}),
        };
        assert_eq!(
            staged_json(&normal).unwrap(),
            json!({"prefix": normal.prefix, "dependency": normal.dependency}).to_string()
        );
    }
}
