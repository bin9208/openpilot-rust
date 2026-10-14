//! Stage only a complete pinned OpenCV provider with passed ARM comparisons.
use crate::{
    disk_space,
    opencv_build::{self, ARCHIVE_SHA256, PATCH_SHA256, REVISION},
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};
fn check(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}
fn arm_elf(path: &Path) -> Result<()> {
    let mut header = [0; 20];
    File::open(path)?.read_exact(&mut header)?;
    check(
        &header[..4] == b"\x7fELF"
            && header[4] == 2
            && header[5] == 1
            && u16::from_le_bytes([header[18], header[19]]) == 183,
        "OpenCV staged object is not an actual aarch64 ELF",
    )
}
#[derive(Debug)]
struct Artifact {
    source: PathBuf,
    destination: PathBuf,
    sha: String,
}
fn file(source: PathBuf, destination: PathBuf, sha: Option<&str>) -> Result<Artifact> {
    let actual = digest(&source)?;
    if let Some(sha) = sha {
        check(actual == sha, "verified OpenCV provider identity changed")?;
    }
    Ok(Artifact {
        source,
        destination,
        sha: actual,
    })
}
fn safe_parent(root: &Path, path: &Path) -> Result<()> {
    check(
        path.starts_with(root),
        "OpenCV destination escaped package root",
    )?;
    for parent in path.ancestors().skip(1) {
        if parent == root {
            break;
        }
        check(
            !parent.is_symlink(),
            "package companion directory is a symlink",
        )?;
    }
    Ok(())
}
pub fn stage(root: &Path, provider: &Path, evidence: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let provider = provider.canonicalize()?;
    let evidence = evidence.canonicalize()?;
    let space = disk_space::inspect(&root, 256 * 1024 * 1024)?;
    space.require()?;
    let identity = read(&provider.join("receipt.json"))?;
    let comparisons = read(&evidence.join("opencv-native/result.json"))?;
    let webcam = read(&evidence.join("webcam-native/result.json"))?;
    check(
        identity["status"] == "BUILT"
            && identity["architecture"] == "aarch64"
            && identity["source_unchanged"] == true
            && identity["source_revision"] == REVISION
            && identity["source_archive_sha256"] == ARCHIVE_SHA256
            && identity["hardening_patch_sha256"] == PATCH_SHA256,
        "combined OpenCV provider identity is not pinned and complete",
    )?;
    let pixels = webcam["pixels"]
        .as_array()
        .ok_or("webcam pixel comparisons missing")?;
    check(
        comparisons["status"] == "pass"
            && !pixels.is_empty()
            && pixels.iter().all(|row| row["match"] == true)
            && webcam["capture"]["match"] == true,
        "actual ARM source comparisons have not passed",
    )?;
    for name in ["provider-abi.json", "consumer-abi.json"] {
        check(
            read(&evidence.join(name))?["compatible"] == true,
            "actual AGNOS dependency closure has not passed",
        )?;
    }
    let sealed = identity["sealed"]
        .as_object()
        .ok_or("OpenCV sealed file map missing")?;
    let manifest = fs::read_to_string(provider.join("native-libraries.sha256"))?;
    let mut listed = serde_json::Map::new();
    for line in manifest.lines() {
        let (hash, relative) = line
            .split_once("  ")
            .ok_or("invalid OpenCV native hash manifest")?;
        opencv_build::safe_relative(Path::new(relative))?;
        check(
            listed.insert(relative.into(), json!(hash)).is_none(),
            "duplicate OpenCV sealed path",
        )?;
    }
    check(
        &listed == sealed,
        "OpenCV receipt and native hash manifest differ",
    )?;
    for (relative, hash) in sealed {
        opencv_build::safe_relative(Path::new(relative))?;
        let path = provider.join(relative);
        check(
            path.canonicalize()?.starts_with(&provider),
            "OpenCV sealed file escapes provider",
        )?;
        check(
            digest(&path)? == hash.as_str().ok_or("invalid sealed OpenCV hash")?,
            "OpenCV sealed provider content changed",
        )?;
    }
    let binaries = root.join("rust/bin");
    let lib = binaries.join("lib");
    let licenses = binaries.join("licenses/opencv");
    let libraries = identity["libraries"]
        .as_object()
        .ok_or("provider libraries missing")?;
    let expected: BTreeSet<_> = opencv_build::Profile::Runtime
        .libraries()
        .iter()
        .map(|name| format!("install/lib/libopencv_{name}.so.4.13.0"))
        .collect();
    check(
        libraries.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "combined provider must contain exactly five reviewed libraries",
    )?;
    let mut artifacts = Vec::new();
    for (relative, sha) in libraries {
        opencv_build::safe_relative(Path::new(relative))?;
        let source = provider.join(relative);
        arm_elf(&source)?;
        artifacts.push(file(
            source,
            lib.join(
                Path::new(relative)
                    .file_name()
                    .ok_or("library basename missing")?,
            ),
            Some(sha.as_str().ok_or("invalid library hash")?),
        )?);
    }
    let frozen = read(&evidence.join("frozen-binaries.json"))?;
    let mut selected = BTreeSet::new();
    for row in frozen["binaries"]
        .as_array()
        .ok_or("frozen binaries missing")?
    {
        let source = PathBuf::from(row["frozen"].as_str().ok_or("frozen path missing")?);
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("frozen basename missing")?;
        if ["openpilot-xiaoge", "openpilot-webcam"].contains(&name) {
            check(
                selected.insert(name.to_owned()),
                "duplicate frozen OpenCV consumer",
            )?;
            arm_elf(&source)?;
            artifacts.push(file(
                source.clone(),
                binaries.join(name),
                Some(row["sha256"].as_str().ok_or("frozen hash missing")?),
            )?);
        }
    }
    check(
        selected.len() == 2,
        "both frozen ARM OpenCV consumers are required",
    )?;
    let required = [
        "OpenCV-Apache-2.0.txt",
        "OpenCV-COPYRIGHT.txt",
        "protobuf-BSD-3-Clause.txt",
        "flatbuffers-Apache-2.0.txt",
        "zlib-License.txt",
        "libjpeg-turbo-licenses.md",
        "libjpeg-turbo-README.ijg",
        "carotene-BSD-3-Clause.txt",
        "KleidiCV-Apache-2.0.txt",
        "external-hal-provenance.json",
    ];
    for name in required {
        check(
            fs::metadata(provider.join("licenses").join(name))?.len() > 0,
            "OpenCV license/provenance missing or empty",
        )?;
    }
    for entry in fs::read_dir(provider.join("licenses"))? {
        let path = entry?.path();
        check(
            path.is_file() && !path.is_symlink(),
            "unexpected OpenCV license node",
        )?;
        artifacts.push(file(
            path.clone(),
            licenses.join(path.file_name().ok_or("license basename missing")?),
            None,
        )?);
    }
    for name in [
        "source.json",
        "opencv-defined-access.patch",
        "native-libraries.sha256",
    ] {
        artifacts.push(file(provider.join(name), licenses.join(name), None)?);
    }
    let source_identity = read(&provider.join("source.json"))?;
    check(
        source_identity["revision"] == REVISION
            && source_identity["sha256"] == ARCHIVE_SHA256
            && source_identity["external_hardening_patch_sha256"] == PATCH_SHA256,
        "OpenCV source provenance differs",
    )?;
    check(
        digest(&provider.join("opencv-defined-access.patch"))? == PATCH_SHA256,
        "staged OpenCV hardening patch differs",
    )?;
    let mut aliases = Vec::new();
    for entry in fs::read_dir(provider.join("install/lib"))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("provider alias name invalid")?;
        if name.starts_with("libopencv_") && name.contains(".so") && path.is_symlink() {
            let target = path.canonicalize()?;
            check(
                target.parent() == Some(provider.join("install/lib").as_path()),
                "OpenCV provider alias escapes install directory",
            )?;
            let target = target.file_name().ok_or("alias target missing")?.to_owned();
            check(
                expected.contains(&format!("install/lib/{}", target.to_string_lossy())),
                "OpenCV alias references an unreviewed object",
            )?;
            aliases.push((lib.join(name), PathBuf::from(target)));
        }
    }
    for artifact in &artifacts {
        if let Ok(relative) = artifact.source.strip_prefix(&provider) {
            if relative != Path::new("native-libraries.sha256") {
                check(
                    sealed.get(relative.to_str().ok_or("non-UTF8 provider artifact")?)
                        == Some(&json!(artifact.sha)),
                    "OpenCV staged provider file was not sealed by its build",
                )?;
            }
        }
    }
    artifacts.sort_by(|left, right| left.destination.cmp(&right.destination));
    aliases.sort();
    for artifact in &artifacts {
        safe_parent(&root, &artifact.destination)?;
        check(
            !artifact.destination.is_symlink(),
            "unexpected package artifact symlink",
        )?;
        if artifact.destination.exists() {
            check(
                artifact.destination.is_file() && digest(&artifact.destination)? == artifact.sha,
                "conflicting package artifact",
            )?;
        }
    }
    for (destination, target) in &aliases {
        safe_parent(&root, destination)?;
        if destination.is_symlink() {
            check(
                fs::read_link(destination)? == *target,
                "conflicting package alias",
            )?;
        } else {
            check(!destination.exists(), "package alias would replace a file")?;
        }
    }
    let records = artifacts
        .iter()
        .map(|item| json!({"source":item.source,"path":item.destination,"sha256":item.sha}))
        .collect::<Vec<_>>();
    let linked = aliases
        .iter()
        .map(|(path, target)| json!({"path":path,"target":target}))
        .collect::<Vec<_>>();
    let mut validation_hashes = serde_json::Map::new();
    for name in [
        "opencv-native/result.json",
        "webcam-native/result.json",
        "provider-abi.json",
        "consumer-abi.json",
        "frozen-binaries.json",
    ] {
        validation_hashes.insert(name.into(), json!(digest(&evidence.join(name))?));
    }
    let provenance = json!({"architecture":"aarch64","source_revision":identity["source_revision"],"source_archive_sha256":identity["source_archive_sha256"],"hardening_patch_sha256":identity["hardening_patch_sha256"],"ownership":"OpenCV and its bundled primitives are external providers; consumer policy is Rust.","libraries":identity["libraries"],"files":records,"aliases":linked,"validation_directory":evidence,"validation_hashes":validation_hashes,"scope":"Actual source ARM image/model/video comparisons and AGNOS ELF closure. No hardware or complete installed-runtime acceptance.","system_dependencies":"Use pinned AGNOS libraries. No C runtime, FFmpeg or system libraries are copied by this stager."});
    let output = licenses.join("provenance.json");
    safe_parent(&root, &output)?;
    check(!output.is_symlink(), "unexpected provenance symlink")?;
    if output.exists() {
        check(
            read(&output)? == provenance,
            "conflicting OpenCV provenance",
        )?;
    }
    for artifact in &artifacts {
        disk_space::inspect(&root, fs::metadata(&artifact.source)?.len())?.require()?;
        check(
            digest(&artifact.source)? == artifact.sha,
            "OpenCV provider changed during staging",
        )?;
        fs::create_dir_all(
            artifact
                .destination
                .parent()
                .ok_or("destination parent missing")?,
        )?;
        if !artifact.destination.exists() {
            fs::copy(&artifact.source, &artifact.destination)?;
        }
        check(
            digest(&artifact.destination)? == artifact.sha,
            "copied OpenCV artifact differs",
        )?;
    }
    for (destination, target) in &aliases {
        if !destination.is_symlink() {
            symlink(target, destination)?;
        }
    }
    if !output.exists() {
        opencv_build::write_json(&output, &provenance)?;
    }
    Ok(
        json!({"files":artifacts.len(),"aliases":aliases.len(),"provenance":output,"free_before":space.free_bytes}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write(path: &Path, value: &Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        opencv_build::write_json(path, value).unwrap();
    }
    fn elf(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = vec![0; 24];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn fixture(base: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let root = base.join("package");
        let provider = base.join("provider");
        let evidence = base.join("evidence");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(provider.join("licenses")).unwrap();
        let mut libraries = serde_json::Map::new();
        for name in opencv_build::Profile::Runtime.libraries() {
            let relative = format!("install/lib/libopencv_{name}.so.4.13.0");
            elf(&provider.join(&relative));
            libraries.insert(
                relative,
                json!(
                    digest(&provider.join(format!("install/lib/libopencv_{name}.so.4.13.0")))
                        .unwrap()
                ),
            );
            symlink(
                format!("libopencv_{name}.so.4.13.0"),
                provider.join(format!("install/lib/libopencv_{name}.so")),
            )
            .unwrap();
        }
        for name in [
            "OpenCV-Apache-2.0.txt",
            "OpenCV-COPYRIGHT.txt",
            "protobuf-BSD-3-Clause.txt",
            "flatbuffers-Apache-2.0.txt",
            "zlib-License.txt",
            "libjpeg-turbo-licenses.md",
            "libjpeg-turbo-README.ijg",
            "carotene-BSD-3-Clause.txt",
            "KleidiCV-Apache-2.0.txt",
            "external-hal-provenance.json",
        ] {
            fs::write(provider.join("licenses").join(name), "fixture notice").unwrap();
        }
        write(
            &provider.join("source.json"),
            &json!({"revision":REVISION,"sha256":ARCHIVE_SHA256,"external_hardening_patch_sha256":PATCH_SHA256}),
        );
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../opencv-runtime/native/opencv-defined-access.patch"),
            provider.join("opencv-defined-access.patch"),
        )
        .unwrap();
        let mut sealed = serde_json::Map::new();
        for relative in libraries
            .keys()
            .cloned()
            .chain(["source.json".into(), "opencv-defined-access.patch".into()])
            .chain(
                fs::read_dir(provider.join("licenses"))
                    .unwrap()
                    .map(|entry| {
                        format!("licenses/{}", entry.unwrap().file_name().to_str().unwrap())
                    }),
            )
        {
            sealed.insert(
                relative.clone(),
                json!(digest(&provider.join(relative)).unwrap()),
            );
        }
        let manifest = sealed
            .iter()
            .map(|(path, hash)| format!("{}  {path}\n", hash.as_str().unwrap()))
            .collect::<String>();
        fs::write(provider.join("native-libraries.sha256"), manifest).unwrap();
        write(
            &provider.join("receipt.json"),
            &json!({"status":"BUILT","architecture":"aarch64","source_unchanged":true,"source_revision":REVISION,"source_archive_sha256":ARCHIVE_SHA256,"hardening_patch_sha256":PATCH_SHA256,"libraries":libraries,"sealed":sealed}),
        );
        write(
            &evidence.join("opencv-native/result.json"),
            &json!({"status":"pass"}),
        );
        write(
            &evidence.join("webcam-native/result.json"),
            &json!({"pixels":[{"match":true}],"capture":{"match":true}}),
        );
        for name in ["provider-abi.json", "consumer-abi.json"] {
            write(&evidence.join(name), &json!({"compatible":true}));
        }
        let frozen = ["openpilot-xiaoge", "openpilot-webcam"].map(|name| {
            let path = evidence.join(name);
            elf(&path);
            json!({"frozen":path,"sha256":digest(&path).unwrap()})
        });
        write(
            &evidence.join("frozen-binaries.json"),
            &json!({"binaries":frozen}),
        );
        (root, provider, evidence)
    }
    #[test]
    fn complete_provider_stages_owned_binaries_aliases_and_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let (root, provider, evidence) = fixture(temp.path());
        stage(&root, &provider, &evidence).unwrap();
        stage(&root, &provider, &evidence).unwrap();
        assert_eq!(
            fs::read_link(root.join("rust/bin/lib/libopencv_core.so")).unwrap(),
            Path::new("libopencv_core.so.4.13.0")
        );
        assert_eq!(
            fs::metadata(root.join("rust/bin/openpilot-webcam"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    #[test]
    fn bad_evidence_and_conflicts_do_not_partially_stage() {
        for case in ["pixels", "abi", "hash", "conflict", "parent"] {
            let temp = tempfile::tempdir().unwrap();
            let (root, provider, evidence) = fixture(temp.path());
            match case {
                "pixels" => write(
                    &evidence.join("webcam-native/result.json"),
                    &json!({"pixels":[],"capture":{"match":true}}),
                ),
                "abi" => write(
                    &evidence.join("consumer-abi.json"),
                    &json!({"compatible":false}),
                ),
                "hash" => fs::write(
                    provider.join("install/lib/libopencv_core.so.4.13.0"),
                    "changed",
                )
                .unwrap(),
                "conflict" => {
                    fs::create_dir_all(root.join("rust/bin")).unwrap();
                    fs::write(root.join("rust/bin/openpilot-webcam"), "conflict").unwrap();
                }
                "parent" => {
                    fs::create_dir_all(root.join("rust")).unwrap();
                    symlink(&evidence, root.join("rust/bin")).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(stage(&root, &provider, &evidence).is_err(), "{case}");
            if case != "parent" {
                assert!(!root.join("rust/bin/openpilot-xiaoge").exists(), "{case}");
            }
            assert!(
                !root.join("rust/bin/lib/libopencv_core.so.4.13.0").exists(),
                "{case}"
            );
            assert!(
                !root
                    .join("rust/bin/licenses/opencv/provenance.json")
                    .exists(),
                "{case}"
            );
        }
    }
}
