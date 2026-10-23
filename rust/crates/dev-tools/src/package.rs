//! Deterministic, exact-source release packaging and fail-closed verification.
use crate::seal::{digest, Result, BOOT_BINARIES};
use flate2::{read::MultiGzDecoder, Compression, GzBuilder};
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
const AGNOS: &str = "19.8-carrot-bt1";
const MAX_ASSET: u64 = 2 * 1024 * 1024 * 1024;
const MAX_UNPACKED: u64 = 8 * 1024 * 1024 * 1024;
const MAX_METADATA: u64 = 4 * 1024 * 1024;
const INSTALLER: &str = "openpilot-runtime-install-aarch64";
const RECEIPT: &str = "runtime-installer-build.json";
const SOURCE_ASSETS: &[&str] = &[
    "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf",
    "openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png",
    "openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png",
    "openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png",
    "openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png",
    "openpilot/selfdrive/assets/images/speed_bg.png",
    "openpilot/selfdrive/assets/images/traffic_green.png",
    "openpilot/selfdrive/assets/images/traffic_red.png",
    "openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png",
    "openpilot/selfdrive/carrot/web/assets/alert_camera.svg",
    "openpilot/selfdrive/carrot/web/assets/alert_police.svg",
    "openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx",
    "openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx",
    "panda/board/obj/bootstub.panda.bin",
    "panda/board/obj/bootstub.panda_h7.bin",
    "panda/board/obj/panda.bin.signed",
    "panda/board/obj/panda_h7.bin.signed",
];
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON value")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut m = serde_json::Map::new();
                while let Some((k, v)) = a.next_entry::<String, Unique>()? {
                    if m.insert(k, v.0).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON field"));
                    }
                }
                Ok(Unique(Value::Object(m)))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = a.next_element::<Unique>()? {
                    v.push(x.0);
                }
                Ok(Unique(Value::Array(v)))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}
fn json_file(path: &Path) -> Result<Value> {
    let m = fs::symlink_metadata(path)?;
    require(
        m.is_file() && !m.file_type().is_symlink() && m.len() > 0 && m.len() <= MAX_METADATA,
        "invalid metadata file",
    )?;
    let value: Unique = serde_json::from_reader(File::open(path)?)?;
    require(value.0.is_object(), "expected JSON object")?;
    Ok(value.0)
}
fn hex(v: &Value, n: usize) -> Result<&str> {
    let s = v.as_str().ok_or("expected SHA string")?;
    require(
        s.len() == n
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid SHA identity",
    )?;
    Ok(s)
}
fn safe(name: &str) -> Result<()> {
    require(
        !name.is_empty()
            && name.len() <= 4096
            && !name.starts_with('/')
            && !name.contains(['\\', '\0'])
            && name
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".."),
        format!("invalid archive path: {name}"),
    )
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let o = Command::new("git").args(args).current_dir(root).output()?;
    require(o.status.success(), String::from_utf8_lossy(&o.stderr))?;
    Ok(String::from_utf8(o.stdout)?.trim().into())
}
pub(crate) fn clean_source(root: &Path) -> Result<(String, u32)> {
    let remote = git(root, &["remote", "get-url", "origin"])?;
    require(
        [
            "https://github.com/bin9208/openpilot-rust",
            "https://github.com/bin9208/openpilot-rust.git",
            "git@github.com:bin9208/openpilot-rust.git",
            "ssh://git@github.com/bin9208/openpilot-rust.git",
        ]
        .contains(&remote.as_str()),
        "wrong source repository origin",
    )?;
    require(
        git(root, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty(),
        "source checkout must be clean",
    )?;
    let commit = git(root, &["rev-parse", "HEAD"])?;
    hex(&Value::String(commit.clone()), 40)?;
    let time = git(root, &["show", "-s", "--format=%ct", &commit])?.parse()?;
    Ok((commit, time))
}
fn u16le(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn u64le(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}
fn native(path: &Path, static_only: bool) -> Result<()> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    require(
        size > 0 && size < MAX_ASSET,
        "asset exceeds 2 GiB limit or is empty",
    )?;
    let mut h = [0; 64];
    f.read_exact(&mut h)?;
    require(
        &h[..7] == b"\x7fELF\x02\x01\x01" && u16le(&h, 18) == 183,
        "expected AArch64 ELF64",
    )?;
    if !static_only {
        return Ok(());
    }
    require(
        [2, 3].contains(&u16le(&h, 16)),
        "installer must be executable ELF",
    )?;
    let offset = u64le(&h, 32);
    let count = u16le(&h, 56);
    require(
        u16le(&h, 54) == 56
            && count > 0
            && count <= 4096
            && offset >= 64
            && offset
                .checked_add(u64::from(count) * 56)
                .is_some_and(|n| n <= size),
        "invalid ELF program headers",
    )?;
    f.seek(SeekFrom::Start(offset))?;
    let mut programs = Vec::new();
    for _ in 0..count {
        let mut p = [0; 56];
        f.read_exact(&mut p)?;
        programs.push(p);
    }
    require(
        programs
            .iter()
            .any(|p| u32::from_le_bytes(p[..4].try_into().unwrap()) == 1),
        "installer has no ELF load segment",
    )?;
    for p in programs {
        let kind = u32::from_le_bytes(p[..4].try_into().unwrap());
        let loc = u64le(&p, 8);
        let len = u64le(&p, 32);
        require(
            loc.checked_add(len).is_some_and(|n| n <= size),
            "ELF segment outside installer",
        )?;
        require(kind != 3, "installer is not static: PT_INTERP")?;
        if kind == 2 {
            require(len.is_multiple_of(16), "invalid ELF dynamic segment")?;
            f.seek(SeekFrom::Start(loc))?;
            for _ in 0..len / 16 {
                let mut d = [0; 16];
                f.read_exact(&mut d)?;
                let tag = u64le(&d, 0);
                if tag == 0 {
                    break;
                }
                require(tag != 1, "installer is not static: DT_NEEDED")?;
            }
        }
    }
    Ok(())
}
#[derive(Clone)]
struct Member {
    sha: String,
    size: u64,
    mode: u32,
    link: Option<String>,
}
fn walk(root: &Path, path: &Path, out: &mut BTreeSet<String>) -> Result<()> {
    for e in fs::read_dir(path)? {
        let p = e?.path();
        let m = fs::symlink_metadata(&p)?;
        if p.is_dir() {
            require(!m.file_type().is_symlink(), "directory alias")?;
            walk(root, &p, out)?;
        } else {
            require(p.is_file(), "unsupported rust/bin artifact")?;
            out.insert(
                p.strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF8 artifact")?
                    .into(),
            );
        }
    }
    Ok(())
}
fn sealed(root: &Path, commit: &str) -> Result<BTreeMap<String, Member>> {
    let manifest = root.join("rust/runtime-manifest.json");
    let v = json_file(&manifest)?;
    require(
        v["schema"].as_u64() == Some(1)
            && v["source_commit"] == commit
            && v["agnos_version"] == AGNOS,
        "sealed manifest identity mismatch",
    )?;
    let rows = v["files"].as_array().ok_or("expected manifest array")?;
    require(
        !rows.is_empty() && rows.len() <= 100000,
        "invalid manifest file count",
    )?;
    let mut members: BTreeMap<String, Member> = BTreeMap::new();
    for row in rows {
        let name = row["path"].as_str().ok_or("expected path string")?;
        safe(name)?;
        require(
            name.starts_with("rust/bin/") || SOURCE_ASSETS.contains(&name),
            format!("unexpected manifest source asset: {name}"),
        )?;
        let executable = row["executable"].as_bool().ok_or("invalid executable")?;
        require(!members.contains_key(name), "duplicate manifest member")?;
        let p = root.join(name);
        let target = p.canonicalize()?;
        require(
            target.starts_with(root) && p.is_file(),
            "artifact escapes repository or is not a file",
        )?;
        let mut parent = p.parent();
        while let Some(dir) = parent {
            if dir == root {
                break;
            }
            require(
                !fs::symlink_metadata(dir)?.file_type().is_symlink(),
                "directory alias",
            )?;
            parent = dir.parent();
        }
        let m = fs::metadata(&p)?;
        let mode = m.permissions().mode() & 0o7777;
        require(
            m.len() > 0 && m.len() < MAX_ASSET,
            "asset exceeds 2 GiB limit or is empty",
        )?;
        require(
            mode <= 0o777 && (!executable || mode & 0o111 != 0),
            "unsafe or missing executable mode",
        )?;
        let sha = hex(&row["sha256"], 64)?.to_owned();
        require(
            digest(&p)? == sha,
            format!("sealed artifact hash mismatch: {name}"),
        )?;
        let mut magic = [0; 4];
        let n = File::open(&p)?.read(&mut magic)?;
        if executable || (n == 4 && magic == *b"\x7fELF") {
            native(&p, false)?;
        }
        let link = if fs::symlink_metadata(&p)?.file_type().is_symlink() {
            let target_name = target.strip_prefix(root)?.to_str().ok_or("invalid link")?;
            let base = Path::new(name).parent().ok_or("invalid link parent")?;
            let mut a = base.components().collect::<Vec<_>>();
            let mut b = Path::new(target_name).components().collect::<Vec<_>>();
            while !a.is_empty() && !b.is_empty() && a[0] == b[0] {
                a.remove(0);
                b.remove(0);
            }
            let mut relative = PathBuf::new();
            for _ in a {
                relative.push("..");
            }
            for c in b {
                relative.push(c);
            }
            Some(relative.to_str().ok_or("invalid archive symlink")?.into())
        } else {
            None
        };
        members.insert(
            name.into(),
            Member {
                sha,
                size: m.len(),
                mode,
                link,
            },
        );
    }
    require(
        members
            .iter()
            .filter(|(n, _)| SOURCE_ASSETS.contains(&n.as_str()))
            .map(|(_, m)| m.size)
            .sum::<u64>()
            <= 128 * 1024 * 1024,
        "source assets exceed 128 MiB activation limit",
    )?;
    for (name, m) in &members {
        if m.link.is_some() {
            let target = root.join(name).canonicalize()?;
            let target = target
                .strip_prefix(root)?
                .to_str()
                .ok_or("invalid target")?;
            require(
                members
                    .get(target)
                    .is_some_and(|t| t.link.is_none() && t.sha == m.sha),
                "symlink target not a sealed regular member",
            )?;
        }
    }
    for name in BOOT_BINARIES {
        require(
            members
                .get(&format!("rust/bin/{name}"))
                .is_some_and(|m| m.mode & 0o111 != 0),
            format!("missing sealed boot executable: {name}"),
        )?;
    }
    require(
        members
            .keys()
            .any(|n| n.starts_with("rust/bin/usbgpu-assets/")),
        "missing sealed GPU bootstrap assets",
    )?;
    let mut actual = BTreeSet::new();
    walk(root, &root.join("rust/bin"), &mut actual)?;
    require(
        actual
            == members
                .keys()
                .filter(|n| n.starts_with("rust/bin/"))
                .cloned()
                .collect(),
        "unsealed or unsupported rust/bin artifact",
    )?;
    members.insert(
        "rust/runtime-manifest.json".into(),
        Member {
            sha: digest(&manifest)?,
            size: fs::metadata(manifest)?.len(),
            mode: 0o644,
            link: None,
        },
    );
    require(
        members
            .values()
            .filter(|m| m.link.is_none())
            .map(|m| m.size)
            .sum::<u64>()
            <= MAX_UNPACKED,
        "runtime exceeds 8 GiB unpacked limit",
    )?;
    Ok(members)
}
fn receipt(path: &Path, installer: &Path, commit: &str) -> Result<Value> {
    let v = json_file(path)?;
    require(
        v["schema"].as_u64() == Some(1)
            && v["source_commit"] == commit
            && v["architecture"] == "aarch64"
            && v["target"] == "aarch64-unknown-linux-musl",
        "installer build receipt identity mismatch",
    )?;
    require(
        hex(&v["installer_sha256"], 64)? == digest(installer)?,
        "installer build receipt hash mismatch",
    )?;
    require(
        v["build_command"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty()),
        "installer build command missing",
    )?;
    Ok(v)
}
fn sidecar(path: &Path) -> Result<String> {
    Ok(format!(
        "{}  {}\n",
        digest(path)?,
        path.file_name()
            .ok_or("missing filename")?
            .to_str()
            .ok_or("invalid filename")?
    ))
}
fn keys(v: &Value, expected: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == expected.len() && expected.iter().all(|k| o.contains_key(*k)))
}
fn write_json(path: &Path, v: &Value) -> Result<()> {
    let mut f = File::create(path)?;
    serde_json::to_writer_pretty(&mut f, v)?;
    f.write_all(b"\n")?;
    Ok(())
}
pub fn disk_guard(destination: &Path, growth: u64) -> Result<()> {
    crate::disk_space::inspect(destination, growth)?.require()
}
#[cfg(test)]
fn check_space(free: u64, growth: u64) -> Result<()> {
    crate::disk_space::Space::new(free, growth)?.require()
}
fn verify_archive(path: &Path, members: &BTreeMap<String, Member>, timestamp: u32) -> Result<u64> {
    let mut file = File::open(path)?;
    let mut h = [0; 10];
    file.read_exact(&mut h)?;
    let mut expected = vec![0x1f, 0x8b, 8, 0];
    expected.extend(timestamp.to_le_bytes());
    expected.extend([0, 255]);
    require(h.as_slice() == expected, "noncanonical gzip header")?;
    file.rewind()?;
    let decoder = MultiGzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut seen = BTreeSet::new();
    let mut unpacked = 0;
    for entry in archive.entries()?.raw(true) {
        let mut e = entry?;
        let raw = e.header().as_bytes();
        require(
            !raw[..100].contains(&b'\\') && !raw[345..500].contains(&b'\\'),
            "invalid archive path separator",
        )?;
        let name = e.path()?.to_str().ok_or("invalid archive name")?.to_owned();
        safe(&name)?;
        let m = members.get(&name).ok_or("unexpected archive member")?;
        require(seen.insert(name), "duplicate archive member")?;
        let h = e.header();
        require(
            h.mode()? == m.mode
                && h.uid()? == 0
                && h.gid()? == 0
                && h.mtime()? == u64::from(timestamp)
                && h.username_bytes().is_none_or(<[u8]>::is_empty)
                && h.groupname_bytes().is_none_or(<[u8]>::is_empty),
            "archive mode/identity mismatch",
        )?;
        if let Some(link) = &m.link {
            require(
                h.entry_type().is_symlink()
                    && std::str::from_utf8(
                        e.header().as_bytes()[157..257]
                            .split(|b| *b == 0)
                            .next()
                            .ok_or("invalid link field")?,
                    )? == link
                    && e.size() == 0,
                "archive symlink mismatch",
            )?;
        } else {
            require(
                h.entry_type().is_file()
                    && e.size() == m.size
                    && e.size() > 0
                    && e.size() < MAX_ASSET,
                "archive member type/size mismatch",
            )?;
            let mut hash = Sha256::new();
            let mut buf = [0; 65536];
            loop {
                let n = e.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
            require(
                format!("{:x}", hash.finalize()) == m.sha,
                "archive member hash mismatch",
            )?;
            unpacked += m.size;
        }
    }
    let mut decoder = archive.into_inner();
    let mut padding = 0;
    let mut buf = [0; 10240];
    loop {
        let n = decoder.read(&mut buf)?;
        if n == 0 {
            break;
        }
        padding += n;
        require(
            padding <= 10240 && buf[..n].iter().all(|b| *b == 0),
            "unexpected trailing archive content",
        )?;
    }
    require(
        seen == members.keys().cloned().collect(),
        "missing archive members",
    )?;
    require(
        unpacked <= MAX_UNPACKED,
        "runtime exceeds 8 GiB unpacked limit",
    )?;
    Ok(unpacked)
}
/// Verify all release assets against the clean exact-source checkout.
pub fn validate_assets(root: &Path, assets: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let (commit, timestamp) = clean_source(&root)?;
    let v = json_file(&assets.join("runtime-release.json"))?;
    require(
        keys(
            &v,
            &[
                "schema",
                "source_commit",
                "architecture",
                "agnos_version",
                "manifest_sha256",
                "archive",
            ],
        ),
        "invalid descriptor fields",
    )?;
    require(
        v["schema"].as_u64() == Some(1)
            && v["source_commit"] == commit
            && v["architecture"] == "aarch64"
            && v["agnos_version"] == AGNOS,
        "release descriptor identity mismatch",
    )?;
    let members = sealed(&root, &commit)?;
    require(
        v["manifest_sha256"] == members["rust/runtime-manifest.json"].sha,
        "release manifest digest mismatch",
    )?;
    let name = format!("openpilot-rust-{commit}-aarch64.tar.gz");
    let info = &v["archive"];
    require(
        keys(info, &["name", "sha256", "bytes", "unpacked_bytes"]) && info["name"] == name,
        "invalid archive descriptor",
    )?;
    let expected: BTreeSet<_> = [
        name.clone(),
        format!("{name}.sha256"),
        "runtime-release.json".into(),
        INSTALLER.into(),
        format!("{INSTALLER}.sha256"),
        RECEIPT.into(),
    ]
    .into_iter()
    .collect();
    let mut actual = BTreeSet::new();
    for e in fs::read_dir(assets)? {
        let e = e?;
        actual.insert(
            e.file_name()
                .into_string()
                .map_err(|_| "invalid asset name")?,
        );
        let m = fs::symlink_metadata(e.path())?;
        require(
            m.is_file() && !m.file_type().is_symlink() && m.len() > 0 && m.len() < MAX_ASSET,
            "invalid release asset",
        )?;
    }
    require(actual == expected, "unexpected or missing release assets")?;
    let archive = assets.join(name);
    require(
        info["bytes"].as_u64() == Some(fs::metadata(&archive)?.len())
            && hex(&info["sha256"], 64)? == digest(&archive)?,
        "archive size/hash mismatch",
    )?;
    require(
        info["unpacked_bytes"].as_u64() == Some(verify_archive(&archive, &members, timestamp)?),
        "archive unpacked size mismatch",
    )?;
    let installer = assets.join(INSTALLER);
    require(
        fs::metadata(&installer)?.permissions().mode() & 0o111 != 0,
        "installer executable mode missing",
    )?;
    native(&installer, true)?;
    receipt(&assets.join(RECEIPT), &installer, &commit)?;
    require(
        members["rust/bin/openpilot-runtime-install"].sha == digest(&installer)?,
        "packaged runtime installer differs from verified standalone installer",
    )?;
    for p in [&archive, &installer] {
        let checksum = p.with_file_name(format!(
            "{}.sha256",
            p.file_name()
                .ok_or("invalid asset")?
                .to_str()
                .ok_or("invalid asset")?
        ));
        require(
            fs::metadata(&checksum)?.len() <= MAX_METADATA
                && fs::read_to_string(checksum)? == sidecar(p)?,
            "checksum sidecar mismatch",
        )?;
    }
    require(
        clean_source(&root)?.0 == commit,
        "source identity changed during validation",
    )?;
    Ok(v)
}
/// Stage, verify, and atomically publish a deterministic release directory.
pub fn package(root: &Path, installer: &Path, receipt_path: &Path, output: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let (commit, timestamp) = clean_source(&root)?;
    let members = sealed(&root, &commit)?;
    let im = fs::symlink_metadata(installer)?;
    require(
        im.is_file() && !im.file_type().is_symlink() && im.permissions().mode() & 0o111 != 0,
        "invalid installer file/mode",
    )?;
    native(installer, true)?;
    let receipt = receipt(receipt_path, installer, &commit)?;
    require(
        members["rust/bin/openpilot-runtime-install"].sha == digest(installer)?,
        "packaged runtime installer differs from verified standalone installer",
    )?;
    require(
        fs::symlink_metadata(output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "release output already exists",
    )?;
    let unpacked = members
        .values()
        .filter(|m| m.link.is_none())
        .map(|m| m.size)
        .sum::<u64>();
    disk_guard(output, unpacked * 21 / 10 + im.len() + 64 * 1024 * 1024)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = tempfile::Builder::new()
        .prefix(".runtime-release-")
        .tempdir_in(parent)?;
    let staged = temp.path().join("assets");
    fs::create_dir(&staged)?;
    let name = format!("openpilot-rust-{commit}-aarch64.tar.gz");
    let archive = staged.join(&name);
    let raw = File::create(&archive)?;
    let compressed = GzBuilder::new()
        .mtime(timestamp)
        .operating_system(255)
        .write(raw, Compression::new(6));
    let mut bundle = tar::Builder::new(compressed);
    for (name, m) in &members {
        let mut h = tar::Header::new_ustar();
        h.set_path(name)?;
        h.set_mode(m.mode);
        h.set_uid(0);
        h.set_gid(0);
        h.set_mtime(u64::from(timestamp));
        if let Some(link) = &m.link {
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            h.set_link_name(link)?;
            h.set_cksum();
            bundle.append(&h, std::io::empty())?;
        } else {
            h.set_entry_type(tar::EntryType::Regular);
            h.set_size(m.size);
            h.set_cksum();
            bundle.append(&h, File::open(root.join(name))?)?;
        }
    }
    let compressed = bundle.into_inner()?;
    let raw = compressed.finish()?;
    raw.sync_all()?;
    require(
        fs::metadata(&archive)?.len() < MAX_ASSET,
        "archive exceeds 2 GiB asset limit",
    )?;
    let helper = staged.join(INSTALLER);
    fs::copy(installer, &helper)?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755))?;
    write_json(&staged.join(RECEIPT), &receipt)?;
    for p in [&archive, &helper] {
        fs::write(
            p.with_file_name(format!(
                "{}.sha256",
                p.file_name()
                    .ok_or("missing filename")?
                    .to_str()
                    .ok_or("invalid filename")?
            )),
            sidecar(p)?,
        )?;
    }
    let descriptor = json!({"schema":1,"source_commit":commit,"architecture":"aarch64","agnos_version":AGNOS,"manifest_sha256":members["rust/runtime-manifest.json"].sha,"archive":{"name":name,"sha256":digest(&archive)?,"bytes":fs::metadata(&archive)?.len(),"unpacked_bytes":unpacked}});
    write_json(&staged.join("runtime-release.json"), &descriptor)?;
    validate_assets(&root, &staged)?;
    fs::rename(staged, output)?;
    Ok(descriptor)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disk_growth_checks_u64_boundaries_without_a_fixed_reserve() {
        assert!(check_space(u64::MAX, u64::MAX).is_ok());
        assert!(check_space(100, 100).is_ok());
        assert!(check_space(99, 100)
            .unwrap_err()
            .to_string()
            .contains("insufficient disk for estimated operation growth"));
        assert!(check_space(0, 0).is_ok());
    }
    #[test]
    fn paths_and_nested_duplicate_json_fail_closed() {
        for name in [
            "",
            "/root",
            "../escape",
            "a/../b",
            "a//b",
            "a/./b",
            "a\\b",
            "a\0b",
            "a/",
        ] {
            assert!(safe(name).is_err(), "{name:?}");
        }
        assert!(safe("rust/bin/helper").is_ok());
        assert!(serde_json::from_str::<Unique>(r#"{"archive":{"bytes":1,"bytes":2}}"#).is_err());
    }
}
