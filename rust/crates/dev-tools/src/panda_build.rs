//! Direct pinned-toolchain Panda firmware build. Never flash or connect a device.
use crate::{
    disk_space,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use base64::Engine;
use rsa::{
    pkcs1::DecodeRsaPrivateKey,
    traits::{PrivateKeyParts, PublicKeyParts},
    BigUint, RsaPrivateKey, RsaPublicKey,
};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const NAMES: [&str; 4] = [
    "panda.bin.signed",
    "panda_h7.bin.signed",
    "bootstub.panda.bin",
    "bootstub.panda_h7.bin",
];
const PRIVATE_KEY: &str = "panda/certs/debug";
const MAX_FILE: u64 = 32 << 20;
const CONTROLS: [(&str, &str); 5] = [
    (
        "panda/SConscript",
        "44521b59c3f0f781c67e8e4b94d6703c8f4b08d193ed2904a2be690d08d36d5b",
    ),
    (
        "panda/SConstruct",
        "25e087356e2f7875260d4f1290cec468bbfdd3d06952002bb7f257abfafb8e0b",
    ),
    (
        "panda/board/SConscript",
        "5f09764e1a72c17f9dc7f95f0a5f17a4faca601cacd886f3fc4c3fdfc10111d3",
    ),
    (
        "panda/firmware_version.py",
        "37be0788651d800e4f1dc9fbd0616bfef5007e0702dd814fa88bfd611bb7bfae",
    ),
    (
        "panda/crypto/sign.py",
        "d81444036a91dc0e30157b1b012c4ef6dbf37192e19bf538e2f2e03b56551c99",
    ),
];
fn check(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn read(path: &Path) -> Result<Vec<u8>> {
    let flags =
        i32::try_from((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits())?;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(path)?;
    let metadata = file.metadata()?;
    check(
        metadata.is_file() && metadata.len() <= MAX_FILE,
        "Panda input must be a bounded regular file",
    )?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    check(bytes.len() as u64 <= MAX_FILE, "Panda input exceeds limit")?;
    Ok(bytes)
}
fn walk(directory: &Path, prefix: &str, output: &mut BTreeMap<String, PathBuf>) -> Result<()> {
    check(directory.is_dir(), "Panda source directory missing")?;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or("non-UTF8 Panda source path")?;
        if ["obj", "tests", "__pycache__", ".git"].contains(&name) {
            continue;
        }
        let relative = format!("{prefix}/{name}");
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            walk(&path, &relative, output)?;
        } else {
            check(
                metadata.is_file(),
                "Panda source contains symlink or special file",
            )?;
            if !matches!(
                path.extension().and_then(|x| x.to_str()),
                Some("o" | "dblite")
            ) {
                output.insert(relative, path);
            }
        }
    }
    Ok(())
}
fn inputs(root: &Path, public_only: bool) -> Result<BTreeMap<String, String>> {
    let mut paths = BTreeMap::new();
    for directory in ["panda", "opendbc/safety"] {
        walk(&root.join(directory), directory, &mut paths)?;
    }
    if public_only {
        paths.remove(PRIVATE_KEY);
    }
    check(paths.len() <= 10000, "excessive Panda source files")?;
    let mut total = 0usize;
    paths
        .into_iter()
        .map(|(name, path)| {
            let bytes = read(&path)?;
            total = total
                .checked_add(bytes.len())
                .ok_or("source size overflow")?;
            check(total <= 256 << 20, "Panda source exceeds bounded snapshot")?;
            Ok((name, format!("{:x}", Sha256::digest(bytes))))
        })
        .collect()
}
/// Exactly firmware_version.py's content identity; retained build scripts are opaque inputs.
pub fn firmware_identity(root: &Path) -> Result<String> {
    let mut paths = BTreeMap::new();
    for (label, directory) in [
        ("board", "panda/board"),
        ("safety", "opendbc/safety"),
        ("crypto", "panda/crypto"),
    ] {
        let mut candidates = BTreeMap::new();
        walk(&root.join(directory), label, &mut candidates)?;
        for (name, path) in candidates {
            let suffix = path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ["c", "h", "s", "ld"].contains(&suffix.as_str())
                || (label == "crypto" && suffix == "py")
                || path.file_name().is_some_and(|x| x == "SConscript")
            {
                paths.insert(name, path);
            }
        }
    }
    for name in [
        "SConscript",
        "SConstruct",
        "firmware_version.py",
        "certs/debug.pub",
        "certs/release.pub",
    ] {
        paths.insert(name.into(), root.join("panda").join(name));
    }
    let mut hash = Sha256::new();
    for (name, path) in paths {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(Sha256::digest(read(&path)?));
    }
    Ok(format!(
        "DEV-{}-DEBUG",
        &format!("{:x}", hash.finalize())[..12]
    ))
}
pub fn gitversion_header(identity: &str) -> Result<String> {
    check(
        identity.len() <= 128
            && identity
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid Panda firmware identity",
    )?;
    Ok(format!(
        "extern const uint8_t gitversion[{}];\nconst uint8_t gitversion[{}] = \"{identity}\";\n",
        identity.len(),
        identity.len()
    ))
}
fn public(bytes: &[u8]) -> Result<RsaPublicKey> {
    let text = std::str::from_utf8(bytes)?;
    let mut words = text.split_whitespace();
    check(
        words.next() == Some("ssh-rsa"),
        "require SSH RSA public key",
    )?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(words.next().ok_or("missing SSH public key")?)?;
    let mut remaining = decoded.as_slice();
    fn field<'a>(remaining: &mut &'a [u8]) -> Result<&'a [u8]> {
        let size =
            u32::from_be_bytes(remaining.get(..4).ok_or("short SSH key")?.try_into()?) as usize;
        let end = 4usize.checked_add(size).ok_or("SSH length overflow")?;
        let result = remaining.get(4..end).ok_or("short SSH field")?;
        *remaining = &remaining[end..];
        Ok(result)
    }
    check(field(&mut remaining)? == b"ssh-rsa", "invalid SSH key type")?;
    let e = BigUint::from_bytes_be(field(&mut remaining)?);
    let n = BigUint::from_bytes_be(field(&mut remaining)?);
    check(
        remaining.is_empty() && n.bits() == 1024,
        "Panda source public key must be RSA1024",
    )?;
    Ok(RsaPublicKey::new(n, e)?)
}
fn c_words(value: &BigUint) -> String {
    let mut bytes = value.to_bytes_le();
    bytes.resize(128, 0);
    let words = bytes
        .chunks_exact(4)
        .map(|b| format!("{}U", u32::from_le_bytes(b.try_into().expect("four bytes"))))
        .collect::<Vec<_>>();
    format!("{{{}}}", words.join(","))
}
pub fn certificate_header(debug: &[u8], release: &[u8]) -> Result<String> {
    let mut result = String::new();
    for (name, bytes) in [("debug", debug), ("release", release)] {
        let key = public(bytes)?;
        let rr = (BigUint::from(1u32) << 2048usize) % key.n();
        let mut bytes = key.n().to_bytes_le();
        bytes.resize(4, 0);
        let n = u32::from_le_bytes(bytes[..4].try_into()?);
        let mut inverse = 1u32;
        for _ in 0..5 {
            inverse = inverse.wrapping_mul(2u32.wrapping_sub(n.wrapping_mul(inverse)));
        }
        result.push_str(&format!("RSAPublicKey {name}_rsa_key = {{\n  .len = 0x20,\n  .n0inv = {}U,\n  .n = {},\n  .rr = {},\n  .exponent = {},\n}};\n",inverse.wrapping_neg(),c_words(key.n()),c_words(&rr),key.e()));
    }
    Ok(result)
}
fn body(raw: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    check(raw.len() >= 4, "Panda unsigned body too short")?;
    let length = raw
        .len()
        .checked_add(8)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("Panda body length overflow")?;
    let mut body = raw.to_vec();
    body.extend_from_slice(b"VERS");
    body.extend_from_slice(&2u32.to_le_bytes());
    let hash = Sha1::digest(&body[4..]);
    body[..4].copy_from_slice(&length.to_le_bytes());
    let mut encoded = vec![0, 1];
    encoded.extend_from_slice(&[0xff; 105]);
    encoded.push(0);
    encoded.extend_from_slice(&hash);
    Ok((body, encoded))
}
pub fn sign_firmware(raw: &[u8], private: &[u8], debug: &[u8]) -> Result<Vec<u8>> {
    let key = RsaPrivateKey::from_pkcs1_pem(
        std::str::from_utf8(private).map_err(|_| "invalid private signing key")?,
    )
    .map_err(|_| "invalid private signing key")?;
    key.validate().map_err(|_| "invalid private signing key")?;
    let expected = public(debug)?;
    check(
        key.n() == expected.n() && key.e() == expected.e(),
        "signing key differs from source debug public key",
    )?;
    let (mut signed, encoded) = body(raw)?;
    let signature = BigUint::from_bytes_be(&encoded)
        .modpow(key.d(), key.n())
        .to_bytes_be();
    signed.extend(std::iter::repeat_n(0, 128 - signature.len()));
    signed.extend(signature);
    verify_signed_firmware(raw, &signed, debug)?;
    Ok(signed)
}
pub fn verify_signed_firmware(raw: &[u8], signed: &[u8], debug: &[u8]) -> Result<()> {
    let key = public(debug)?;
    let (body, encoded) = body(raw)?;
    check(
        signed.len() == body.len() + 128,
        "Panda signed length mismatch",
    )?;
    check(
        signed[..body.len()] == body,
        "Panda source body/SETLEN/VERS mismatch",
    )?;
    let signature = BigUint::from_bytes_be(&signed[body.len()..]);
    check(&signature < key.n(), "Panda signature outside modulus")?;
    check(
        signature.modpow(key.e(), key.n()) == BigUint::from_bytes_be(&encoded),
        "Panda raw SHA1 RSA signature mismatch",
    )
}
pub fn verify_bootstub(bytes: &[u8]) -> Result<()> {
    check(bytes.len() >= 8, "short Panda bootstub vectors")?;
    let stack = u32::from_le_bytes(bytes[..4].try_into()?);
    let reset = u32::from_le_bytes(bytes[4..8].try_into()?);
    check(
        (0x2000_0000..0x3000_0000).contains(&stack)
            && reset > 0x0800_0000
            && reset < 0x0802_0000
            && reset % 2 == 1,
        "Panda bootstub Cortex-M vectors invalid",
    )
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Artifact {
    name: String,
    sha256: String,
    size: u64,
}
fn verified_artifacts(folder: &Path, key: &[u8]) -> Result<Vec<(Artifact, Vec<u8>)>> {
    NAMES
        .into_iter()
        .map(|name| {
            let path = folder.join(name);
            let bytes = read(&path)?;
            check(!bytes.is_empty(), "empty Panda firmware")?;
            if let Some(raw) = name.strip_suffix(".signed") {
                verify_signed_firmware(&read(&folder.join(raw))?, &bytes, key)?;
            } else {
                verify_bootstub(&bytes)?;
            }
            Ok((
                Artifact {
                    name: name.into(),
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    size: bytes.len() as u64,
                },
                bytes,
            ))
        })
        .collect()
}
fn artifacts(folder: &Path, key: &[u8]) -> Result<Vec<Artifact>> {
    Ok(verified_artifacts(folder, key)?
        .into_iter()
        .map(|(artifact, _)| artifact)
        .collect())
}
fn tool_identity(path: &Path) -> Result<(PathBuf, String)> {
    let path = path.canonicalize()?;
    let hash = format!("{:x}", Sha256::digest(read(&path)?));
    Ok((path, hash))
}
fn controls(root: &Path) -> Result<()> {
    for (name, expected) in CONTROLS {
        check(
            format!("{:x}", Sha256::digest(read(&root.join(name))?)) == expected,
            "Panda build-control source changed; review direct-build contract",
        )?;
    }
    Ok(())
}
fn parents(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        match fs::symlink_metadata(parent) {
            Ok(metadata) => check(
                metadata.is_dir(),
                "Panda destination ancestor must be a real directory",
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn capacity(path: &Path) -> Result<()> {
    disk_space::inspect(path, 512 << 20)?.require()
}
fn flags(h7: bool) -> Vec<String> {
    let project = if h7 {
        vec![
            "-mcpu=cortex-m7",
            "-mhard-float",
            "-DSTM32H7",
            "-DSTM32H725xx",
            "-Iboard/stm32h7/inc",
            "-mfpu=fpv5-d16",
        ]
    } else {
        vec![
            "-mcpu=cortex-m4",
            "-mhard-float",
            "-DSTM32F4",
            "-DSTM32F413xx",
            "-Iboard/stm32f4/inc",
            "-mfpu=fpv4-sp-d16",
        ]
    };
    let mut flags = project.into_iter().map(String::from).collect::<Vec<_>>();
    flags.extend(["-fsingle-precision-constant", "-Os", "-g", "-DPANDA"].map(String::from));
    if h7 {
        flags.push("-DENABLE_SPI".into());
    }
    flags.extend(
        [
            "-DALLOW_DEBUG",
            "-Wall",
            "-Wextra",
            "-Wstrict-prototypes",
            "-Werror",
            "-mlittle-endian",
            "-mthumb",
            "-nostdlib",
            "-fno-builtin",
            "-std=gnu11",
            "-fmax-errors=1",
        ]
        .map(String::from),
    );
    flags.push(format!(
        "-Tboard/{}/{}_flash.ld",
        if h7 { "stm32h7" } else { "stm32f4" },
        if h7 { "stm32h7x5" } else { "stm32f4" }
    ));
    flags
}
fn command(
    tool: &Path,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
    limit: usize,
) -> CommandSpec {
    let mut spec = CommandSpec::new(
        std::iter::once(tool.to_string_lossy().into_owned())
            .chain(args.iter().cloned())
            .collect(),
        timeout,
    );
    spec.cwd = Some(cwd.into());
    spec.stdout_limit = limit;
    spec.stderr_limit = limit;
    spec
}
fn execute(
    tool: &Path,
    args: &[String],
    cwd: &Path,
    log: &mut fs::File,
    commands: &mut Vec<Vec<String>>,
    runner: &mut impl Runner,
) -> Result<()> {
    let spec = command(tool, args, cwd, Duration::from_secs(300), 1024 * 1024);
    commands.push(spec.argv.clone());
    let captured = runner.run(&spec)?;
    let growth = captured
        .stdout
        .len()
        .checked_add(captured.stderr.len())
        .ok_or("Panda log length overflow")?;
    check(
        log.metadata()?.len().saturating_add(growth as u64) <= 16 << 20,
        "Panda aggregate build log exceeds 16 MiB",
    )?;
    log.write_all(&captured.stdout)?;
    log.write_all(&captured.stderr)?;
    if captured.code != 0 {
        return Err(format!(
            "Panda compiler/objcopy failed ({}): {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        )
        .into());
    }
    Ok(())
}
fn freeze_snapshot(root: &Path, source: &Path, expected: &BTreeMap<String, String>) -> Result<()> {
    for (name, hash) in expected {
        let bytes = read(&root.join(name))?;
        check(
            format!("{:x}", Sha256::digest(&bytes)) == *hash,
            "Panda snapshot source changed after admission",
        )?;
        let target = source.join(name);
        fs::create_dir_all(target.parent().ok_or("source parent missing")?)?;
        fs::write(target, bytes)?;
    }
    Ok(())
}
fn publish_directory(source: &Path, output: &Path) -> Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    Ok(())
}
/// Serial compilation is bounded below the accepted one/two job ceiling.
pub fn build(root: &Path, output: &Path, toolchain: &Path, jobs: u32) -> Result<()> {
    build_with_runner(root, output, toolchain, jobs, &mut NativeRunner)
}
pub fn build_with_runner(
    root: &Path,
    output: &Path,
    toolchain: &Path,
    jobs: u32,
    runner: &mut impl Runner,
) -> Result<()> {
    check((1..=2).contains(&jobs), "Panda jobs must be one or two")?;
    let absolute_output = std::env::current_dir()?.join(output);
    let output = absolute_output.as_path();
    let root = fs::canonicalize(root)?;
    let toolchain = fs::canonicalize(toolchain)?;
    controls(&root)?;
    let before = inputs(&root, false)?;
    let mut snapshot = before.clone();
    snapshot.remove(PRIVATE_KEY);
    let (gcc, compiler_sha256) = tool_identity(&toolchain.join("bin/arm-none-eabi-gcc"))?;
    let (objcopy, objcopy_sha256) = tool_identity(&toolchain.join("bin/arm-none-eabi-objcopy"))?;
    let compiler_version = runner.run(&command(
        &gcc,
        &["--version".into()],
        &root,
        Duration::from_secs(10),
        64 * 1024,
    ))?;
    check(
        compiler_version.code == 0,
        "Panda compiler version command failed",
    )?;
    let compiler_version = String::from_utf8(compiler_version.stdout)?;
    check(
        compiler_version.contains("Arm GNU Toolchain 13.2.rel1")
            && compiler_version.contains("13.2.1 20231009"),
        "require pinned Arm GNU Toolchain 13.2.rel1",
    )?;
    let parent = output.parent().ok_or("Panda build output needs parent")?;
    parents(parent)?;
    check(
        !output.exists() && !output.is_symlink(),
        "Panda build output already exists",
    )?;
    capacity(parent)?;
    fs::create_dir_all(parent)?;
    let work = tempfile::Builder::new()
        .prefix(".panda-build-")
        .tempdir_in(parent)?;
    let source = work.path().join("source");
    freeze_snapshot(&root, &source, &snapshot)?;
    check(
        inputs(&source, true)? == snapshot,
        "Panda public snapshot mismatch",
    )?;
    check(
        !source.join(PRIVATE_KEY).exists(),
        "private signing key in public snapshot",
    )?;
    controls(&source)?;
    let identity = firmware_identity(&source)?;
    let debug = read(&source.join("panda/certs/debug.pub"))?;
    let release = read(&source.join("panda/certs/release.pub"))?;
    let cwd = source.join("panda");
    let obj = cwd.join("board/obj");
    fs::create_dir(&obj)?;
    fs::write(obj.join("gitversion.h"), gitversion_header(&identity)?)?;
    fs::write(obj.join("version"), &identity)?;
    fs::write(obj.join("cert.h"), certificate_header(&debug, &release)?)?;
    let mut log = fs::File::create(work.path().join("build.log"))?;
    // Read the key once, after the public snapshot has passed admission.
    let private = read(&root.join(PRIVATE_KEY))?;
    check(
        before.get(PRIVATE_KEY) == Some(&format!("{:x}", Sha256::digest(&private))),
        "Panda signing key changed after source admission",
    )?;
    let mut commands = Vec::new();
    // Match the original ordered target request: F4/H7 mains, then F4/H7 bootstubs.
    for (h7, boot) in [(false, false), (true, false), (false, true), (true, true)] {
        let name = if h7 { "panda_h7" } else { "panda" };
        let project_flags = flags(h7);
        let startup = format!("board/obj/startup_{name}.o");
        if !boot {
            let startup_source = if h7 {
                "board/stm32h7/startup_stm32h7x5xx.s"
            } else {
                "board/stm32f4/startup_stm32f413xx.s"
            };
            let mut args = project_flags.clone();
            args.extend([
                "-o".into(),
                startup.clone(),
                "-c".into(),
                startup_source.into(),
            ]);
            execute(&gcc, &args, &cwd, &mut log, &mut commands, runner)?;
        }
        let units = if boot {
            vec![
                ("rsa", "crypto/rsa.c"),
                ("sha", "crypto/sha.c"),
                ("bootstub", "board/bootstub.c"),
            ]
        } else {
            vec![("main", "board/main.c")]
        };
        for (unit, input) in units {
            let mut args = vec!["-o".into(), format!("board/{unit}-{name}.o"), "-c".into()];
            args.extend(project_flags.clone());
            args.extend([
                "-Iboard".into(),
                "-I.".into(),
                "-I.".into(),
                "-Iboard".into(),
                format!("-I{}", source.join("opendbc/safety").display()),
                input.into(),
            ]);
            execute(&gcc, &args, &cwd, &mut log, &mut commands, runner)?;
        }
        let stem = if boot {
            format!("bootstub.{name}")
        } else {
            name.into()
        };
        let elf = format!("board/obj/{stem}.elf");
        let mut args = vec!["-o".into(), elf.clone()];
        if !boot {
            args.push(format!(
                "-Wl,--section-start,.isr_vector={}",
                if h7 { "0x8020000" } else { "0x8004000" }
            ));
        }
        args.extend(project_flags);
        args.push(startup);
        if boot {
            args.extend([
                format!("board/rsa-{name}.o"),
                format!("board/sha-{name}.o"),
                format!("board/bootstub-{name}.o"),
            ]);
        } else {
            args.push(format!("board/main-{name}.o"));
        }
        execute(&gcc, &args, &cwd, &mut log, &mut commands, runner)?;
        execute(
            &objcopy,
            &[
                "-O".into(),
                "binary".into(),
                elf,
                format!("board/obj/{stem}.bin"),
            ],
            &cwd,
            &mut log,
            &mut commands,
            runner,
        )?;
        if !boot {
            // Signing key exists only in process memory; never write it to the snapshot.
            fs::write(
                obj.join(format!("{name}.bin.signed")),
                sign_firmware(&read(&obj.join(format!("{name}.bin")))?, &private, &debug)?,
            )?;
        }
    }
    let artifacts = artifacts(&obj, &debug)?;
    check(
        before == inputs(&root, false)? && snapshot == inputs(&source, true)?,
        "Panda source mutated during build",
    )?;
    let report = serde_json::json!({"source_inputs":before,"snapshot_inputs":snapshot,"source_unchanged":true,"artifacts":artifacts,"firmware_version":identity,"compiler_version":compiler_version,"compiler_sha256":compiler_sha256,"objcopy_sha256":objcopy_sha256,"command":commands,"signing":"unchanged DEBUG/ALLOW_DEBUG; RSA1024 raw SHA1, SETLEN=1, VERS=2","scope":"unchanged independent safety firmware; build and verification only, no device","snapshot_scope":"public source only; excludes panda/certs/debug; complete source_inputs includes its digest"});
    fs::write(
        work.path().join("receipt.json"),
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    check(
        !output.exists() && !output.is_symlink(),
        "Panda output appeared during build",
    )?;
    publish_directory(work.path(), output)?;
    Ok(())
}
#[derive(Deserialize)]
struct Receipt {
    source_inputs: BTreeMap<String, String>,
    snapshot_inputs: BTreeMap<String, String>,
    source_unchanged: bool,
    artifacts: Vec<Artifact>,
    firmware_version: String,
}
/// Admit every firmware, license and receipt destination before publishing any.
/// Concurrent destination edits are outside this ordinary-admission transaction.
pub fn stage(root: &Path, output: &Path) -> Result<()> {
    capacity(root)?;
    publish_copies(admitted_copies(root, output)?)
}
fn admitted_copies(root: &Path, output: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    parents(&output.join("source/panda/board/obj"))?;
    parents(&output.join("source/opendbc/safety"))?;
    let receipt_bytes = read(&output.join("receipt.json"))?;
    let receipt: Receipt = serde_json::from_slice(&receipt_bytes)?;
    check(
        receipt.source_unchanged
            && inputs(root, false)? == receipt.source_inputs
            && inputs(&output.join("source"), true)? == receipt.snapshot_inputs,
        "Panda firmware source changed since build",
    )?;
    let mut expected = receipt.source_inputs.clone();
    expected.remove(PRIVATE_KEY);
    check(
        expected == receipt.snapshot_inputs
            && !output.join("source").join(PRIVATE_KEY).try_exists()?,
        "Panda snapshot identity/private-key boundary mismatch",
    )?;
    check(
        !output.join("source").join(PRIVATE_KEY).is_symlink(),
        "private signing key symlink in snapshot",
    )?;
    check(
        firmware_identity(&output.join("source"))? == receipt.firmware_version,
        "Panda firmware version mismatch",
    )?;
    let obj = output.join("source/panda/board/obj");
    let key = read(&output.join("source/panda/certs/debug.pub"))?;
    check(
        receipt.snapshot_inputs.get("panda/certs/debug.pub")
            == Some(&format!("{:x}", Sha256::digest(&key))),
        "Panda public key changed after snapshot admission",
    )?;
    let frozen = verified_artifacts(&obj, &key)?;
    check(
        frozen
            .iter()
            .map(|(artifact, _)| artifact)
            .eq(receipt.artifacts.iter()),
        "Panda artifacts changed since build",
    )?;
    let mut copies = frozen
        .into_iter()
        .map(|(artifact, bytes)| (root.join("panda/board/obj").join(artifact.name), bytes))
        .collect::<Vec<_>>();
    let license = read(&root.join("panda/LICENSE"))?;
    check(
        receipt.source_inputs.get("panda/LICENSE")
            == Some(&format!("{:x}", Sha256::digest(&license))),
        "Panda license changed after source admission",
    )?;
    copies.push((root.join("rust/bin/licenses/panda/LICENSE"), license));
    copies.push((
        root.join("rust/bin/licenses/panda/provenance.json"),
        receipt_bytes,
    ));
    for (target, bytes) in &copies {
        parents(target.parent().ok_or("Panda destination parent missing")?)?;
        if target.exists() || target.is_symlink() {
            check(
                read(target)? == *bytes,
                "conflicting Panda package artifact",
            )?;
        }
    }
    Ok(copies)
}
fn publish_copies(copies: Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
    // Prepare every missing file completely before publishing the first final name.
    let mut prepared = Vec::new();
    for (target, bytes) in copies {
        if !target.exists() {
            let parent = target.parent().ok_or("Panda destination parent missing")?;
            fs::create_dir_all(parent)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            temporary.write_all(&bytes)?;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o644))?;
            temporary.as_file().sync_all()?;
            prepared.push((target, temporary));
        }
    }
    // Concurrent destination edits are excluded; roll back only newly published
    // inodes still owned by this invocation if a later no-clobber publish fails.
    use std::os::unix::fs::MetadataExt;
    let mut published: Vec<(PathBuf, u64, u64)> = Vec::new();
    for (target, temporary) in prepared {
        let metadata = temporary.as_file().metadata()?;
        match temporary.persist_noclobber(&target) {
            Ok(_) => published.push((target, metadata.dev(), metadata.ino())),
            Err(error) => {
                for (path, device, inode) in published {
                    if fs::symlink_metadata(&path)
                        .is_ok_and(|m| m.dev() == device && m.ino() == inode)
                    {
                        fs::remove_file(path)?;
                    }
                }
                return Err(error.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use crate::qa_process::{CaptureError, Captured};
    mod fixtures {
        use super::*;
        const UNSIGNED: &[u8] = include_bytes!("../tests/fixtures/panda/unsigned.bin");
        const SIGNED: &[u8] = include_bytes!("../tests/fixtures/panda/signed.bin");
        fn repository() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap()
                .into()
        }
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/panda_fixture.rs"
        ));
    }
    #[test]
    fn stage_publishes_exact_admitted_firmware_and_receipt_after_source_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let (root, output) = fixtures::stage_fixture(temp.path());
        let copies = admitted_copies(&root, &output).unwrap();
        let receipt = read(&output.join("receipt.json")).unwrap();
        let expected = include_bytes!("../tests/fixtures/panda/signed.bin");
        fs::write(
            output.join("source/panda/board/obj/panda.bin.signed"),
            b"changed after admission",
        )
        .unwrap();
        fs::write(output.join("receipt.json"), b"changed after admission").unwrap();
        publish_copies(copies).unwrap();
        assert_eq!(
            read(&root.join("panda/board/obj/panda.bin.signed")).unwrap(),
            expected.as_slice()
        );
        assert_eq!(
            read(&root.join("rust/bin/licenses/panda/provenance.json")).unwrap(),
            receipt
        );
    }
    #[test]
    fn compiler_and_objcopy_fifos_reject_before_any_external_command() {
        struct Never;
        impl Runner for Never {
            fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
                panic!("unadmitted tool executed")
            }
        }
        for fifo_name in ["arm-none-eabi-gcc", "arm-none-eabi-objcopy"] {
            let temp = tempfile::tempdir().unwrap();
            let toolchain = temp.path().join("compiler");
            fs::create_dir_all(toolchain.join("bin")).unwrap();
            for name in ["arm-none-eabi-gcc", "arm-none-eabi-objcopy"] {
                let path = toolchain.join("bin").join(name);
                if name == fifo_name {
                    rustix::fs::mkfifoat(
                        rustix::fs::CWD,
                        &path,
                        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                    )
                    .unwrap();
                } else {
                    fs::write(path, b"owned compiler identity").unwrap();
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap();
            let output = temp.path().join("build");
            assert!(build_with_runner(root, &output, &toolchain, 2, &mut Never).is_err());
            assert!(!output.exists());
        }
    }
    #[test]
    fn changed_and_grown_snapshot_inputs_are_rejected_before_copy() {
        for grown in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let input = temp.path().join("input");
            fs::write(&input, b"admitted").unwrap();
            let expected =
                BTreeMap::from([("input".into(), format!("{:x}", Sha256::digest(b"admitted")))]);
            if grown {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&input)
                    .unwrap()
                    .set_len(MAX_FILE + 1)
                    .unwrap();
            } else {
                fs::write(&input, b"mutated").unwrap();
            }
            let snapshot = temp.path().join("snapshot");
            assert!(freeze_snapshot(temp.path(), &snapshot, &expected).is_err());
            assert!(!snapshot.exists());
        }
    }
    #[test]
    fn snapshot_reads_reject_leaf_symlinks_and_fifos_without_blocking() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file");
        fs::write(&file, b"bytes").unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(file, &link).unwrap();
        assert!(read(&link).is_err());
        let fifo = temp.path().join("fifo");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(read(&fifo).is_err());
    }
    #[test]
    fn publication_never_replaces_an_existing_empty_destination() {
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("complete"), b"artifact").unwrap();
        fs::create_dir(&output).unwrap();
        let inode = fs::metadata(&output).unwrap().ino();
        assert!(publish_directory(&source, &output).is_err());
        assert_eq!(fs::metadata(&output).unwrap().ino(), inode);
        assert!(fs::read_dir(output).unwrap().next().is_none());
        assert!(source.join("complete").is_file());
    }
    #[test]
    fn failed_or_interrupted_compiler_does_not_publish_partial_output() {
        struct Fail {
            calls: usize,
            timeout: bool,
        }
        impl Runner for Fail {
            fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
                self.calls += 1;
                assert!(!spec.timeout.is_zero() && spec.timeout <= Duration::from_secs(300));
                assert!(spec.stdout_limit <= 1024 * 1024 && spec.stderr_limit <= 1024 * 1024);
                if self.calls == 1 {
                    return Ok(Captured {
                        code: 0,
                        stdout: b"Arm GNU Toolchain 13.2.rel1 13.2.1 20231009\n".to_vec(),
                        stderr: vec![],
                    });
                }
                if self.timeout {
                    Err(CaptureError::Timeout {
                        timeout: spec.timeout,
                        stdout: vec![],
                        stderr: vec![],
                    }
                    .into())
                } else {
                    Ok(Captured {
                        code: 17,
                        stdout: vec![],
                        stderr: b"owned compiler failure".to_vec(),
                    })
                }
            }
        }
        for timeout in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let toolchain = temp.path().join("compiler");
            fs::create_dir_all(toolchain.join("bin")).unwrap();
            for name in ["arm-none-eabi-gcc", "arm-none-eabi-objcopy"] {
                fs::write(
                    toolchain.join("bin").join(name),
                    b"owned mock compiler identity",
                )
                .unwrap();
            }
            let output = temp.path().join("build");
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap();
            let mut runner = Fail { calls: 0, timeout };
            let result = build_with_runner(root, &output, &toolchain, 2, &mut runner);
            assert!(result.is_err());
            assert_eq!(runner.calls, 2);
            assert!(!output.exists());
            assert_eq!(
                fs::read_dir(temp.path()).unwrap().count(),
                1,
                "owned temporary build must be cleaned"
            );
        }
    }
}
