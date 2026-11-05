//! The device's ordinary compile/install path. Audited generations are separate.
use crate::{
    generation::{Config, Layout},
    native_tools,
    prepare::Plan,
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
struct Stamp {
    bytes: u64,
    modified: i64,
    modified_ns: i64,
}

fn stamp(path: &Path) -> Result<Stamp> {
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err("user build input is not a file".into());
    }
    Ok(Stamp {
        bytes: meta.len(),
        modified: meta.mtime(),
        modified_ns: meta.mtime_nsec(),
    })
}

#[derive(Clone, Deserialize, Serialize)]
struct Installed {
    source: PathBuf,
    input: Stamp,
    output: Stamp,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub compiled: usize,
    pub fresh: usize,
    pub installed: usize,
    pub root: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub development_audit_executed: bool,
    pub sealed_generation_created: bool,
}

fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("user build destination must stay inside the source checkout".into());
    }
    Ok(())
}

fn install(
    source: &Path,
    destination: &Path,
    executable: bool,
    previous: Option<&Installed>,
) -> Result<(Installed, bool)> {
    let input = stamp(source)?;
    if let Some(old) = previous {
        if old.source == source
            && old.input == input
            && stamp(destination).ok().as_ref() == Some(&old.output)
        {
            return Ok((old.clone(), false));
        }
    }
    let parent = destination
        .parent()
        .ok_or("user build output has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    fs::copy(source, temporary.path())?;
    fs::set_permissions(
        temporary.path(),
        fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
    )?;
    temporary.as_file().sync_all()?;
    // Replace one completed file atomically; do not mutate an executable inode
    // retained by an already running spinner/recovery/previous process.
    temporary.persist(destination)?;
    Ok((
        Installed {
            source: source.into(),
            input,
            output: stamp(destination)?,
        },
        true,
    ))
}

pub fn compile(config: &Config, root: &Path) -> Result<Report> {
    crate::generation::offroad(&config.stores.params)?;
    crate::generation::agnos_identity()?;
    let plan = Plan::read(&config.build_plan)?;
    let layout: Layout = crate::generation::read(&config.layout)?;
    let (_workspace_lock, compiler_root) = workspace(root, &config.stores.sources)?;
    compile_at(
        &compiler_root,
        root,
        &config.cache,
        &config.toolchain,
        &plan,
        &layout,
    )
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let result = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(["-c", "gc.auto=0", "-c", "maintenance.auto=false"])
        .args(args)
        .output()?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned().into());
    }
    Ok(String::from_utf8(result.stdout)?.trim().into())
}

fn workspace(root: &Path, store: &Path) -> Result<(crate::FileLock, PathBuf)> {
    fs::create_dir_all(store)?;
    let lock = crate::FileLock::exclusive(
        fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(store.join("workspace.lock"))?,
    )?;
    let workspace = store.join("build-workspace");
    let dirty = git(root, &["diff", "--name-only", "HEAD"])?;
    if !workspace.is_dir()
        || dirty
            .lines()
            .any(|p| !openpilot_runtime_installer::source_asset_allowed(Path::new(p)))
    {
        return Ok((lock, root.to_owned()));
    }
    if workspace.canonicalize()? != workspace {
        return Err("compiler workspace is redirected".into());
    }
    let common = PathBuf::from(git(
        &workspace,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    if !common.canonicalize()?.starts_with(store.canonicalize()?) {
        return Err("compiler workspace belongs to another repository".into());
    }
    if !git(&workspace, &["diff", "--name-only", "HEAD"])?.is_empty() {
        return Ok((lock, root.to_owned())); // preserve edits; never reset/clean them.
    }
    let head = git(root, &["rev-parse", "HEAD"])?;
    if git(&workspace, &["rev-parse", "HEAD"])? != head {
        git(
            &workspace,
            &[
                "fetch",
                "--depth=1",
                "--no-tags",
                "--no-recurse-submodules",
                "--update-shallow",
                &root.to_string_lossy(),
                &head,
            ],
        )?;
        git(&workspace, &["checkout", "--detach", &head])?;
    }
    Ok((lock, workspace))
}

fn normalized(recipe: &crate::Recipe) -> Result<Vec<u8>> {
    let mut value = recipe.clone();
    value.packages.clear();
    value.binaries.clear();
    value.features.clear();
    value.expected_growth_bytes = 0;
    Ok(serde_json::to_vec(&value)?)
}

fn previous_targets(
    cache: &Path,
    toolchain: &Path,
    plan: &Plan,
) -> Result<BTreeMap<Vec<u8>, PathBuf>> {
    let recipes = plan
        .groups
        .values()
        .map(|recipe| {
            Ok((
                format!("{:x}", Sha256::digest(serde_json::to_vec(recipe)?)),
                normalized(recipe)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut targets = BTreeMap::new();
    for entry in fs::read_dir(cache)? {
        let entry = entry?;
        let report = entry.path().join("last-successful-build.json");
        if !report.is_file() || fs::metadata(&report)?.len() > 4 * 1024 * 1024 {
            continue;
        }
        let value: Value = serde_json::from_reader(fs::File::open(report)?)?;
        let Some(key) = value["recipe_sha256"].as_str().and_then(|s| recipes.get(s)) else {
            continue;
        };
        let Some(path) = value["target_directory"].as_str() else {
            continue;
        };
        let target = PathBuf::from(path);
        if value["schema"] == 3
            && value["toolchain_directory"].as_str() == toolchain.to_str()
            && target == entry.path().join("target")
            && target.is_dir()
        {
            // A cache hint, not an audit result. Cargo checks its own fingerprints
            // and rebuilds changed compiler options/features/source normally.
            targets.insert(key.clone(), target);
        }
    }
    Ok(targets)
}

/// Cargo performs dependency/freshness decisions. This path does not call
/// Prepared::verify, generation::seal, an ELF audit, or capture output objects.
pub fn compile_plan(
    root: &Path,
    cache: &Path,
    toolchain: &Path,
    plan: &Plan,
    layout: &Layout,
) -> Result<Report> {
    compile_at(root, root, cache, toolchain, plan, layout)
}

fn compile_at(
    compiler_root: &Path,
    root: &Path,
    cache: &Path,
    toolchain: &Path,
    plan: &Plan,
    layout: &Layout,
) -> Result<Report> {
    let root = root.canonicalize()?;
    let compiler_root = compiler_root.canonicalize()?;
    let toolchain = toolchain.canonicalize()?;
    fs::create_dir_all(cache)?;
    let cache = cache.canonicalize()?;
    let _lock = crate::FileLock::exclusive(
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(cache.join("user-build.lock"))?,
    )?;
    let rustc = toolchain.join("rustc");
    let cargo = toolchain.join("cargo");
    let compiler = (stamp(&rustc)?, stamp(&cargo)?);
    let previous_targets = previous_targets(&cache, &toolchain, plan)?;
    let index = cache.join("user-installed.json");
    let previous: BTreeMap<PathBuf, Installed> = match fs::read(&index) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e.into()),
    };
    let binary_link = root.join("rust/bin");
    let migrating = fs::symlink_metadata(&binary_link).is_ok_and(|m| m.file_type().is_symlink())
        && binary_link.read_link()? != Path::new(".source-bin");
    // A legacy installed package may own rust/bin. Keep that package intact and
    // switch its link only after all new files are ready. Subsequent builds reuse
    // this one directory rather than creating a package/generation per commit.
    let binaries =
        if migrating || binary_link.read_link().ok().as_deref() == Some(Path::new(".source-bin")) {
            root.join("rust/.source-bin")
        } else {
            binary_link.clone()
        };
    let mut pending = Vec::new();
    let mut compiled = 0;
    let mut fresh = 0;
    for (number, (group, recipe)) in plan.groups.iter().enumerate() {
        let mut recipe = recipe.clone();
        recipe
            .binaries
            .retain(|name| !matches!(name.as_str(), "seal-runtime-package" | "runtime-elf-audit"));
        if recipe.binaries.is_empty() {
            continue;
        }
        recipe.validate()?;
        let tools = native_tools::for_build(&recipe, &toolchain)?;
        let native_stamps = tools
            .environment
            .values()
            .map(|path| Ok((path, stamp(path)?)))
            .collect::<Result<Vec<_>>>()?;
        let mut key_recipe = recipe.clone();
        key_recipe.packages.clear();
        key_recipe.binaries.clear();
        key_recipe.features.clear();
        key_recipe.expected_growth_bytes = 0;
        let key = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                "user-cargo-v1",
                &toolchain,
                &compiler,
                &tools.environment,
                &native_stamps,
                &key_recipe
            ))?)
        );
        let target = previous_targets
            .get(&normalized(&recipe)?)
            .cloned()
            .unwrap_or_else(|| cache.join("user-cargo").join(key).join("target"));
        fs::create_dir_all(target.parent().ok_or("Cargo target parent missing")?)?;
        let _target_lock = crate::FileLock::exclusive(
            fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(target.parent().unwrap().join("build.lock"))?,
        )?;
        fs::create_dir_all(&target)?;
        let space = rustix::fs::statvfs(&target)?;
        if space.f_bavail.saturating_mul(space.f_frsize) < recipe.expected_growth_bytes {
            return Err("insufficient disk space for the next compile group".into());
        }
        let mut command = Command::new(&cargo);
        let linker_key = format!(
            "CARGO_TARGET_{}_LINKER",
            recipe.target.to_ascii_uppercase().replace('-', "_")
        );
        let mut flags = recipe
            .environment
            .get("RUSTFLAGS")
            .map_or_else(Vec::new, |s| {
                s.split_whitespace().map(str::to_owned).collect()
            });
        flags.extend(["-C".into(), format!("linker={}", tools.linker.display())]);
        command
            .current_dir(&compiler_root)
            .env_clear()
            .envs(&recipe.environment)
            .envs(&tools.environment)
            .env(
                "PATH",
                std::env::join_paths([
                    toolchain.as_path(),
                    Path::new("/usr/bin"),
                    Path::new("/bin"),
                ])?,
            )
            .env("HOME", &cache)
            .env("CARGO_HOME", cache.join("cargo-home"))
            .env("CARGO_TARGET_DIR", &target)
            .env("RUSTC", &rustc)
            .env("RUSTDOC", toolchain.join("rustdoc"))
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_BUILD_JOBS", "2")
            .env("CARGO_PROFILE_RELEASE_LTO", "false")
            .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "16")
            .env(linker_key, &tools.linker)
            .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\u{1f}"));
        if let Some(vendor) = recipe.environment.get("OPENPILOT_CARGO_VENDOR") {
            command.args([
                "--config",
                "source.crates-io.replace-with=\"openpilot-native\"",
            ]);
            command.args([
                "--config",
                &format!(
                    "source.openpilot-native.directory={}",
                    serde_json::to_string(vendor)?
                ),
            ]);
        }
        command.args([
            "build",
            "--manifest-path",
            "rust/Cargo.toml",
            "--locked",
            "--offline",
            "--release",
            "--message-format=json-render-diagnostics",
            "--target",
            &recipe.target,
        ]);
        for package in &recipe.packages {
            command.args(["--package", package]);
        }
        for binary in &recipe.binaries {
            command.args(["--bin", binary]);
        }
        if !recipe.features.is_empty() {
            command.args(["--features", &recipe.features.join(",")]);
        }
        eprintln!("Compiling {}/{} · {group}", number + 1, plan.groups.len());
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let result = (|| -> Result<usize> {
            let mut outputs = 0;
            for line in
                BufReader::new(child.stdout.take().ok_or("Cargo output unavailable")?).lines()
            {
                let value: Value = serde_json::from_str(&line?)?;
                if let Some(rendered) = value["message"]["rendered"].as_str() {
                    eprint!("{rendered}");
                }
                if value["reason"] != "compiler-artifact" {
                    continue;
                }
                if let Some(executable) = value["executable"].as_str() {
                    let path = PathBuf::from(executable).canonicalize()?;
                    if !path.starts_with(&target) {
                        return Err("Cargo output escaped its target directory".into());
                    }
                    if value["fresh"] == true {
                        fresh += 1;
                    } else {
                        compiled += 1;
                    }
                    let name = path
                        .file_name()
                        .ok_or("Cargo output name missing")?
                        .to_owned();
                    pending.push((path, binaries.join(name), true));
                    outputs += 1;
                }
            }
            Ok(outputs)
        })();
        if result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait()?;
        if !status.success() || result? != recipe.binaries.len() {
            return Err(format!("Cargo build failed in {group}: {status}").into());
        }
    }
    let provider = layout.provider_root.canonicalize()?;
    for (destination, input) in &layout.files {
        relative(destination)?;
        relative(input)?;
        if !openpilot_runtime_installer::artifact_path_allowed(destination) {
            return Err("unsupported user build destination".into());
        }
        let source = provider.join(input).canonicalize()?;
        if !source.starts_with(&provider) {
            return Err("provider input escaped its prepared directory".into());
        }
        let destination = if let Ok(path) = destination.strip_prefix("rust/bin") {
            binaries.join(path)
        } else {
            root.join(destination)
        };
        let executable = fs::metadata(&source)?.permissions().mode() & 0o111 != 0;
        pending.push((source, destination, executable));
    }
    if pending
        .iter()
        .any(|(_, p, _)| p.file_name().is_some_and(|s| s == "openpilot-webjoystick"))
    {
        pending.push(("/usr/bin/openssl".into(), binaries.join("openssl"), true));
    }
    let mut installed = 0;
    let mut current = BTreeMap::new();
    for (source, destination, executable) in pending {
        let (record, copied) = install(
            &source,
            &destination,
            executable,
            previous.get(&destination),
        )?;
        installed += usize::from(copied);
        current.insert(destination, record);
    }
    if migrating {
        let temporary = root.join("rust/.user-bin-link");
        match fs::remove_file(&temporary) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        std::os::unix::fs::symlink(".source-bin", &temporary)?;
        fs::rename(temporary, &binary_link)?;
    }
    let mut saved = tempfile::NamedTempFile::new_in(&cache)?;
    serde_json::to_writer(saved.as_file_mut(), &current)?;
    saved.as_file_mut().write_all(b"\n")?;
    saved.as_file_mut().sync_all()?;
    saved.persist(index)?;
    eprintln!("Build complete · {compiled} compiled, {fresh} unchanged");
    Ok(Report {
        compiled,
        fresh,
        installed,
        root,
        libraries: layout.libraries.clone(),
        development_audit_executed: false,
        sealed_generation_created: false,
    })
}
