//! Native C/C++ drivers and their reported compiler/linker helpers. The complete
//! header/library/toolchain trees are separate mandatory generation inputs.
use crate::{digest_file, Recipe, Result};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Tools {
    pub linker: PathBuf,
    pub executables: BTreeMap<PathBuf, String>,
    pub environment: BTreeMap<String, PathBuf>,
}

fn resolve(value: &str, toolchain: &Path) -> Result<PathBuf> {
    if value.is_empty() {
        return Err(
            "native compiler settings require one executable; pass options through compiler flags"
                .into(),
        );
    }
    let path = Path::new(value);
    if path.is_absolute() {
        path.canonicalize()?;
        return Ok(path.to_owned());
    }
    if path.components().count() != 1 || value.chars().any(char::is_whitespace) {
        return Err("native tool path must be absolute or a single executable name".into());
    }
    for prefix in [toolchain, Path::new("/usr/bin"), Path::new("/bin")] {
        let path = prefix.join(value);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(format!("native tool is missing: {value}").into())
}

pub(crate) fn inspect(recipe: &Recipe, toolchain: &Path) -> Result<Tools> {
    let linker = Path::new("/usr/bin/cc").canonicalize()?;
    let mut tools = Tools {
        linker: linker.clone(),
        executables: BTreeMap::new(),
        environment: BTreeMap::new(),
    };
    tools.executables.insert(linker, String::new());
    if let Some(value) = recipe.environment.get("OPENPILOT_CAPNP") {
        if !Path::new(value).is_absolute() || value.contains(['\n', '\r']) {
            return Err("native schema compiler requires an absolute executable path".into());
        }
        let path = resolve(value, toolchain)?;
        tools
            .executables
            .insert(path.canonicalize()?, String::new());
        tools.environment.insert("OPENPILOT_CAPNP".into(), path);
    }
    for (key, fallback) in [
        ("CC", "gcc"),
        ("CXX", "g++"),
        ("AR", "ar"),
        ("RANLIB", "ranlib"),
    ] {
        // This driver already requires host == target. cc-rs therefore checks
        // HOST_* before the unprefixed setting. Pin that choice explicitly.
        let host = format!("HOST_{key}");
        let value = recipe
            .environment
            .get(&host)
            .or_else(|| recipe.environment.get(key))
            .map_or(fallback, String::as_str);
        let path = resolve(value, toolchain)?;
        tools
            .executables
            .insert(path.canonicalize()?, String::new());
        tools.environment.insert(key.into(), path.clone());
        tools.environment.insert(host, path);
    }
    let path = std::env::join_paths([toolchain, Path::new("/usr/bin"), Path::new("/bin")])?;
    for driver in [
        &tools.linker,
        &tools.environment["CC"],
        &tools.environment["CXX"],
    ] {
        for name in ["ld", "collect2", "as", "cc1", "cc1plus"] {
            let output = Command::new(driver)
                .arg(format!("-print-prog-name={name}"))
                .env_clear()
                .envs(&recipe.environment)
                .envs(&tools.environment)
                .env("PATH", &path)
                .output()?;
            if !output.status.success() {
                return Err("cannot identify native C/C++ compiler helpers".into());
            }
            let value = String::from_utf8(output.stdout)?;
            let value = value.trim();
            // Clang's cc1 is integrated and it need not provide GCC collect2.
            let optional = matches!(name, "collect2" | "cc1" | "cc1plus");
            match resolve(value, toolchain) {
                Ok(path) => {
                    tools
                        .executables
                        .insert(path.canonicalize()?, String::new());
                }
                Err(_) if optional && value == name => (),
                Err(error) => return Err(error),
            }
        }
    }
    for (path, digest) in &mut tools.executables {
        let mode = std::fs::metadata(path)?.permissions().mode();
        if mode & 0o111 == 0 || mode & 0o6000 != 0 {
            return Err("native tool must be executable without set-id permissions".into());
        }
        *digest = digest_file(path)?;
    }
    Ok(tools)
}
