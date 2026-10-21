//! Owned Git and overlay-command fixture for updater comparisons.
//! Original observations must be captured in Actions before claiming parity.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub struct Seed {
    pub remote: PathBuf,
    pub previous: String,
}

pub struct Setup {
    pub config: Value,
    pub environment: BTreeMap<OsString, OsString>,
}

fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("non-UTF-8 updater fixture path: {}", path.display()).into())
}

fn git(runner: &mut dyn Runner, cwd: &Path, args: &[OsString], dated: bool) -> Result<String> {
    let mut argv = vec![OsString::from("/usr/bin/git")];
    argv.extend_from_slice(args);
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(60));
    command.cwd = Some(cwd.to_path_buf());
    command.merge_output = true; // subprocess.check_output(..., stderr=STDOUT)
    command.env.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
    command
        .env
        .insert("GIT_CONFIG_GLOBAL".into(), "/dev/null".into());
    if dated {
        command
            .env
            .insert("GIT_AUTHOR_DATE".into(), "2026-01-01T00:00:00Z".into());
        command
            .env
            .insert("GIT_COMMITTER_DATE".into(), "2026-01-01T00:00:00Z".into());
    }
    let observed = runner.run(&command)?;
    if observed.code != 0 {
        return Err(format!(
            "fixture git exited {}: {}",
            observed.code,
            String::from_utf8_lossy(&observed.stdout)
        )
        .into());
    }
    Ok(String::from_utf8(observed.stdout)?.trim().to_owned())
}

fn git_args(runner: &mut dyn Runner, cwd: &Path, args: &[&str]) -> Result<String> {
    git(
        runner,
        cwd,
        &args
            .iter()
            .map(|arg| OsString::from(*arg))
            .collect::<Vec<_>>(),
        false,
    )
}

pub fn seed_with(runner: &mut dyn Runner, directory: &Path) -> Result<Seed> {
    let remote = directory.join("remote");
    fs::create_dir(&remote)?;
    git_args(runner, &remote, &["init", "-b", "dev"])?;
    git_args(
        runner,
        &remote,
        &["config", "user.email", "fixture@example.invalid"],
    )?;
    git_args(
        runner,
        &remote,
        &["config", "user.name", "Owned updater fixture"],
    )?;
    let mut previous = None;
    for version in ["1.0", "2.0"] {
        fs::create_dir_all(remote.join("common"))?;
        fs::write(
            remote.join("common/version.h"),
            format!("#define COMMA_VERSION \"{version}\"\n"),
        )?;
        fs::write(
            remote.join("RELEASES.md"),
            format!("{version}\n===\n* verified & staged\n  * nested\n\nolder release"),
        )?;
        let launch = remote.join("launch_env.sh");
        fs::write(&launch, b"export AGNOS_VERSION=\"fixture-os\"\n")?;
        fs::set_permissions(&launch, fs::Permissions::from_mode(0o755))?;
        fs::write(
            remote.join("build.json"),
            format!("{{\"channel\": \"dev\", \"openpilot\": {{\"version\": \"{version}\"}}}}"),
        )?;
        let link = remote.join("version-link");
        if !link.exists() {
            symlink("common/version.h", &link)?;
        }
        let agnos = remote.join("openpilot/system/hardware/tici");
        fs::create_dir_all(&agnos)?;
        for name in ["agnos.json", "agnos-tici.json"] {
            fs::write(agnos.join(name), b"[]")?;
        }
        git_args(runner, &remote, &["add", "."])?;
        git(
            runner,
            &remote,
            &["commit".into(), "-m".into(), version.into()],
            true,
        )?;
        if version == "1.0" {
            previous = Some(git_args(runner, &remote, &["rev-parse", "HEAD"])?);
        }
    }
    git_args(runner, &remote, &["branch", "release2"])?;
    git_args(runner, &remote, &["branch", "release-tizi"])?;
    Ok(Seed {
        remote,
        previous: previous.ok_or("missing first updater fixture commit")?,
    })
}

pub fn seed(directory: &Path) -> Result<Seed> {
    seed_with(&mut NativeRunner, directory)
}

fn which(name: &str) -> Result<PathBuf> {
    let path = env::var_os("PATH").ok_or("missing PATH for updater fixture")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if let Ok(metadata) = fs::metadata(&candidate) {
            if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                return Ok(candidate);
            }
        }
    }
    Err(format!("missing updater fixture command: {name}").into())
}

pub fn setup_with(
    runner: &mut dyn Runner,
    root: &Path,
    seed: &Seed,
    launcher: &Path,
    shim: &Path,
) -> Result<Setup> {
    fs::create_dir(root)?;
    let base = root.join("base");
    git(
        runner,
        root,
        &[
            "clone".into(),
            seed.remote.as_os_str().into(),
            base.as_os_str().into(),
        ],
        false,
    )?;
    git_args(runner, &base, &["reset", "--hard", &seed.previous])?;
    fs::create_dir_all(root.join("data/params/d"))?;
    fs::create_dir(root.join("tmp"))?;
    let model = root.join("sys/firmware/devicetree/base/model");
    fs::create_dir_all(model.parent().ok_or("model parent missing")?)?;
    fs::write(model, b"comma c3\0")?;
    let bin = root.join("bin");
    fs::create_dir(&bin)?;
    symlink(shim, bin.join("sudo"))?;
    for name in ["git", "bash", "ionice", "find"] {
        symlink(which(name)?, bin.join(name))?;
    }
    let mut environment = env::vars_os().collect::<BTreeMap<_, _>>();
    environment.insert(
        "PATH".into(),
        OsString::from(format!("{}:/usr/bin:/bin", text(&bin)?)),
    );
    environment.insert("UPDATED_FIXTURE_ROOT".into(), root.as_os_str().to_owned());
    environment.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
    environment.insert("GIT_CONFIG_GLOBAL".into(), "/dev/null".into());
    environment.insert("TZ".into(), "UTC".into());
    environment.remove(OsStr::new("OPENPILOT_PREFIX"));
    let config = json!({
        "paths":{"base":text(&base)?,"staging":text(&root.join("data/stage"))?,"lock":text(&root.join("tmp/updated.lock"))?,"system_root":text(root)?},
        "launcher":text(launcher)?,"now":1790812800.0,"device":"pc","os_version":"fixture-os","agnos":false,"steps":[]
    });
    Ok(Setup {
        config,
        environment,
    })
}

pub fn setup(root: &Path, seed: &Seed, launcher: &Path, shim: &Path) -> Result<Setup> {
    setup_with(&mut NativeRunner, root, seed, launcher, shim)
}

fn decode_hex(raw: &str) -> Result<Vec<u8>> {
    fn digit(value: u8) -> Result<u8> {
        match value {
            b'0'..=b'9' => Ok(value - b'0'),
            b'a'..=b'f' => Ok(value - b'a' + 10),
            b'A'..=b'F' => Ok(value - b'A' + 10),
            _ => Err("invalid Params hex in updater fixture".into()),
        }
    }
    let bytes = raw.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err("odd Params hex in updater fixture".into());
    }
    bytes
        .chunks_exact(2)
        .map(|pair| Ok((digit(pair[0])? << 4) | digit(pair[1])?))
        .collect()
}
fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}
fn replace_bytes(input: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    if needle.is_empty() {
        return input.to_vec();
    }
    let mut result = Vec::new();
    let mut from = 0;
    while let Some(at) = input[from..]
        .windows(needle.len())
        .position(|part| part == needle)
    {
        let end = from + at;
        result.extend_from_slice(&input[from..end]);
        result.extend_from_slice(replacement);
        from = end + needle.len();
    }
    result.extend_from_slice(&input[from..]);
    result
}

pub fn normalize(value: &Value, root: &Path) -> Result<Value> {
    let path = text(root)?;
    fn walk(value: &Value, root: &str) -> Result<Value> {
        Ok(match value {
            Value::Object(fields) => {
                let mut result = serde_json::Map::new();
                for (key, item) in fields {
                    if key == "params" {
                        let source = item.as_object().ok_or("params must be an object")?;
                        let mut normalized = serde_json::Map::new();
                        for (name, raw) in source {
                            let bytes =
                                decode_hex(raw.as_str().ok_or("Params value must be hex")?)?;
                            normalized.insert(
                                name.clone(),
                                Value::String(encode_hex(&replace_bytes(
                                    &bytes,
                                    root.as_bytes(),
                                    b"<ROOT>",
                                ))),
                            );
                        }
                        result.insert(key.clone(), Value::Object(normalized));
                    } else {
                        result.insert(key.clone(), walk(item, root)?);
                    }
                }
                Value::Object(result)
            }
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|item| walk(item, root))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Value::String(text) => Value::String(text.replace(root, "<ROOT>")),
            _ => value.clone(),
        })
    }
    walk(value, &path)
}

fn owned(root: &Path, arg: &OsStr) -> Result<PathBuf> {
    let path = Path::new(arg);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    if absolute
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || !absolute.starts_with(root)
        || absolute == root
    {
        return Err(format!(
            "overlay fixture path outside owned root: {}",
            absolute.display()
        )
        .into());
    }
    let mut ancestor = absolute.as_path();
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or("missing overlay parent")?;
    }
    if !fs::canonicalize(ancestor)?.starts_with(root) {
        return Err("overlay fixture path escapes through symlink".into());
    }
    Ok(absolute)
}

fn copytree(source: &Path, target: &Path) -> Result<()> {
    reject_destination_symlinks(source, target)?;
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;
        if metadata.file_type().is_symlink() {
            symlink(fs::read_link(&from)?, &to)?;
        } else if metadata.is_dir() {
            copytree(&from, &to)?;
        } else if metadata.is_file() {
            // Opening the final component with NOFOLLOW preserves the preflight
            // boundary if a private destination changes before this write.
            let mut input = fs::File::open(&from)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                .open(&to)?;
            std::io::copy(&mut input, &mut output)?;
            fs::set_permissions(&to, metadata.permissions())?;
        } else {
            return Err("unsupported overlay fixture source file".into());
        }
    }
    fs::set_permissions(target, fs::metadata(source)?.permissions())?;
    Ok(())
}

fn reject_destination_symlinks(source: &Path, target: &Path) -> Result<()> {
    if fs::symlink_metadata(target).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!("overlay target symlink: {}", target.display()).into());
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if fs::symlink_metadata(&to).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(format!("overlay destination symlink: {}", to.display()).into());
        }
        if fs::symlink_metadata(&from)?.is_dir() {
            reject_destination_symlinks(&from, &to)?;
        }
    }
    Ok(())
}

fn python_json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\u{8}' => result.push_str("\\b"),
            '\u{c}' => result.push_str("\\f"),
            value if value < '\u{20}' || value >= '\u{80}' => {
                let mut encoded = [0; 2];
                for unit in value.encode_utf16(&mut encoded).iter() {
                    result.push_str(&format!("\\u{unit:04x}"));
                }
            }
            value => result.push(value),
        }
    }
    result.push('"');
    result
}

fn python_json_args(args: &[&str]) -> String {
    format!(
        "[{}]",
        args.iter()
            .map(|arg| python_json_string(arg))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// This entrypoint is installed at the owned PATH `sudo` alias. It never calls sudo or mount.
pub fn sudo_cli() -> Result<()> {
    let root = fs::canonicalize(PathBuf::from(
        env::var_os("UPDATED_FIXTURE_ROOT").ok_or("missing UPDATED_FIXTURE_ROOT")?,
    ))?;
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let log = root.join("overlay-commands.jsonl");
    let strings = args
        .iter()
        .map(|arg| arg.to_str().ok_or("non-UTF-8 overlay argument"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut file = OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(file, "{}", python_json_args(&strings))?;
    match strings.as_slice() {
        ["rm", "-rf", target] => {
            let target = owned(&root, OsStr::new(target))?;
            if target.exists() {
                fs::remove_dir_all(target)?;
            }
        }
        ["mount", "-t", "overlay", "-o", options, _, target] => {
            let items = options
                .split(',')
                .map(|item| item.split_once('=').ok_or("invalid overlay option"))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let options = items.into_iter().collect::<BTreeMap<_, _>>();
            let source = owned(
                &root,
                OsStr::new(*options.get("lowerdir").ok_or("missing lowerdir")?),
            )?;
            let workdir = owned(
                &root,
                OsStr::new(*options.get("workdir").ok_or("missing workdir")?),
            )?;
            let target = owned(&root, OsStr::new(target))?;
            copytree(&source, &target)?;
            fs::create_dir(workdir.join("work"))?;
        }
        ["chmod", "755", target] => {
            let target = owned(&root, OsStr::new(target))?;
            fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
        }
        ["umount", "-l", target] => {
            owned(&root, OsStr::new(target))?;
        }
        _ => return Err(format!("unsupported overlay fixture command: {strings:?}").into()),
    }
    Ok(())
}
