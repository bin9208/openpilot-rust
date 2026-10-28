//! One original OpenCV root for both webcam and Xiaoge, with no provider build.
use crate::{
    native_codec_inputs::{directory, hash, json, Published},
    native_ffmpeg_prefix::{compose_with_metadata, Part},
    native_opencv_headers, native_provider,
    seal::Result,
};
use openpilot_runtime_build::{assets, digest_file, inputs::Entry};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

const PROVENANCE: &str = "provenance/native-opencv-prefix.json";
const MANIFEST: &str = "native-libraries.sha256";
const NATIVE_MANIFEST: &str = "bdc3c7f0ea23ea1e15272906478d92433997c8f36a4fe2bd39ab167211be2d06";

fn manifest(parts: &[Part; 2]) -> Result<Vec<u8>> {
    let mut files = BTreeMap::new();
    for part in parts {
        for (path, entry) in &part.tree.entries {
            if !(path.starts_with("install/lib") || path.starts_with("install/include/opencv4")) {
                continue;
            }
            let sha = match entry {
                Entry::Directory => continue,
                Entry::File { sha256, .. } => sha256,
                Entry::Symlink { target } => {
                    let target = path
                        .parent()
                        .ok_or("missing OpenCV alias parent")?
                        .join(target);
                    let Some(Entry::File { sha256, .. }) = part.tree.entries.get(&target) else {
                        return Err(
                            "OpenCV alias must resolve directly to an original library".into()
                        );
                    };
                    sha256
                }
            };
            // The pinned manifest sorts UTF-8 strings, not Path components.
            let path = path
                .to_str()
                .ok_or("non-UTF8 native OpenCV compiler input")?;
            if files.insert(path, sha).is_some() {
                return Err("duplicate native OpenCV compiler input".into());
            }
        }
    }
    Ok(files
        .into_iter()
        .map(|(p, h)| format!("{h}  {p}\n"))
        .collect::<String>()
        .into_bytes())
}
fn compose(parts: [Part; 2], store: &Path) -> Result<Published> {
    let manifest = manifest(&parts)?;
    let provenance = json(&(
        "openpilot-native-opencv-prefix-v1",
        "4.13.0",
        "aarch64-unknown-linux-gnu",
        "19.8-carrot-bt1",
        &parts[0].sha256,
        &parts[1].sha256,
        hash(&manifest),
    ))?;
    let identity = hash(&provenance);
    compose_with_metadata(
        parts,
        store,
        BTreeMap::from([(PROVENANCE.into(), provenance), (MANIFEST.into(), manifest)]),
        identity,
    )
}
#[derive(Debug, Serialize)]
pub struct Report {
    #[serde(flatten)]
    pub prefix: Published,
    pub headers: native_opencv_headers::Report,
    pub provider: native_provider::Report,
}
pub fn cli(arguments: &[String]) -> Result<()> {
    let mut values = BTreeMap::new();
    let mut mode = false;
    let mut args = arguments.iter();
    while let Some(key) = args.next() {
        if key == "--prepare-released-prefix" && !mode {
            mode = true;
            continue;
        }
        if ![
            "--header-plan",
            "--provider-plan",
            "--descriptor",
            "--descriptor-sha256",
            "--header-cache",
            "--provider-cache",
            "--store",
        ]
        .contains(&key.as_str())
            || values
                .insert(
                    key.as_str(),
                    args.next().ok_or("missing OpenCV prefix argument")?,
                )
                .is_some()
        {
            return Err("invalid, duplicate or mixed OpenCV prefix argument".into());
        }
    }
    let value = |key: &str| {
        values
            .get(key)
            .map(|v| v.as_str())
            .ok_or_else(|| format!("{key} required for verified OpenCV prefix"))
    };
    if !mode {
        return Err("--prepare-released-prefix required".into());
    }
    let pin = value("--descriptor-sha256")?;
    if pin.len() != 64
        || !pin
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("independent --descriptor-sha256 is invalid".into());
    }
    let header_plan = native_opencv_headers::Plan::read(Path::new(value("--header-plan")?))?;
    let provider_plan = native_provider::Plan::read(Path::new(value("--provider-plan")?))?;
    if provider_plan.profile != "opencv413-shared" {
        return Err("OpenCV prefix requires its original OpenCV shared provider profile".into());
    }
    if provider_plan
        .members
        .get(Path::new("share/licenses/opencv/native-libraries.sha256"))
        .is_none_or(|member| member.sha256 != hash(header_plan.original_header_manifest.as_bytes()))
    {
        return Err("OpenCV header and provider original hash manifests differ".into());
    }
    let descriptor = Path::new(value("--descriptor")?);
    if digest_file(descriptor)? != pin {
        return Err("OpenCV provider descriptor checksum mismatch".into());
    }
    let store = Path::new(value("--store")?);
    directory(store)?;
    let provider = native_provider::prepare(
        &provider_plan,
        descriptor,
        pin,
        Path::new(value("--provider-cache")?),
        &store.join("providers"),
        None,
        &assets::Curl,
    )?;
    let headers = native_opencv_headers::prepare(
        &header_plan,
        Path::new(value("--header-cache")?),
        &store.join("headers"),
        &assets::Curl,
    )?;
    let parts = [
        Part::read(&headers.prepared)?,
        Part::read(&provider.prepared)?,
    ];
    let compiled = manifest(&parts)?;
    if std::str::from_utf8(&compiled)?.lines().count() != 185 || hash(&compiled) != NATIVE_MANIFEST
    {
        return Err(
            "combined OpenCV compiler inputs differ from all original headers/libraries".into(),
        );
    }
    let prefix = compose(parts, &store.join("prefixes"))?;
    println!(
        "{}",
        serde_json::to_string(&Report {
            prefix,
            headers,
            provider
        })?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_codec_inputs::write;
    use openpilot_runtime_build::inputs::Tree;
    use std::{
        fs,
        os::unix::fs::{symlink, MetadataExt},
        path::PathBuf,
        process::Command,
    };
    fn part(parent: &Path, name: &str, files: &[(&str, &[u8])]) -> Part {
        let root = parent.join(name);
        fs::create_dir(&root).unwrap();
        for (path, body) in files {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            write(&root.join(path), body).unwrap();
        }
        if root.join("install/lib/libopencv_core.so.4.13.0").exists() {
            symlink(
                "libopencv_core.so.4.13.0",
                root.join("install/lib/libopencv_core.so"),
            )
            .unwrap();
        }
        let tree = Tree::capture(&root).unwrap();
        let receipt = parent.join(format!("{name}.json"));
        let bytes = json(&tree).unwrap();
        write(&receipt, &bytes).unwrap();
        Part {
            root,
            receipt,
            sha256: hash(&bytes),
            tree,
        }
    }
    fn parts(parent: &Path) -> [Part; 2] {
        [
            part(
                parent,
                "headers",
                &[
                    ("install/include/opencv4/opencv2/core.hpp", b"owned header"),
                    (
                        "install/include/opencv4/opencv2/core/affine.hpp",
                        b"owned nested header",
                    ),
                ],
            ),
            part(
                parent,
                "provider",
                &[("install/lib/libopencv_core.so.4.13.0", b"owned library")],
            ),
        ]
    }
    fn load(parent: &Path) -> [Part; 2] {
        ["headers", "provider"].map(|name| {
            let receipt = parent.join(format!("{name}.json"));
            Part {
                root: parent.join(name),
                sha256: digest_file(&receipt).unwrap(),
                tree: Tree::read(&receipt).unwrap(),
                receipt,
            }
        })
    }
    #[test]
    fn real_hash_manifest_covers_headers_and_resolved_alias_and_repeat_does_not_copy() {
        let temp = tempfile::tempdir().unwrap();
        let input = parts(temp.path());
        let header = input[0]
            .root
            .join("install/include/opencv4/opencv2/core.hpp");
        let first = compose(input, &temp.path().join("store")).unwrap();
        let status = Command::new("sha256sum")
            .args(["--check", MANIFEST])
            .current_dir(&first.root)
            .status()
            .unwrap();
        assert!(status.success());
        let listing = fs::read_to_string(first.root.join(MANIFEST)).unwrap();
        assert_eq!(
            listing
                .lines()
                .map(|line| line.split_once("  ").unwrap().1)
                .collect::<Vec<_>>(),
            [
                "install/include/opencv4/opencv2/core.hpp",
                "install/include/opencv4/opencv2/core/affine.hpp",
                "install/lib/libopencv_core.so",
                "install/lib/libopencv_core.so.4.13.0",
            ]
        );
        assert!(listing.contains("  install/lib/libopencv_core.so\n"));
        assert!(listing.contains("  install/lib/libopencv_core.so.4.13.0\n"));
        assert_eq!(
            fs::metadata(header).unwrap().ino(),
            fs::metadata(first.root.join("install/include/opencv4/opencv2/core.hpp"))
                .unwrap()
                .ino()
        );
        let repeat = compose(load(temp.path()), &temp.path().join("store")).unwrap();
        assert_eq!(first.identity, repeat.identity);
        assert_eq!((repeat.copied_bytes, repeat.linked_bytes), (0, 0));
        assert_eq!(
            fs::read(first.root.join("install/lib/libopencv_core.so")).unwrap(),
            b"owned library"
        );
    }
    #[test]
    fn an_input_cannot_replace_the_generated_compiler_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let input = [
            part(
                temp.path(),
                "headers",
                &[("install/include/opencv4/core.hpp", b"header")],
            ),
            part(temp.path(), "provider", &[(MANIFEST, b"forged manifest")]),
        ];
        let store = temp.path().join("store");
        assert!(compose(input, &store)
            .unwrap_err()
            .to_string()
            .contains("occupies generated"));
        assert!(!store.exists());
    }
    #[test]
    fn an_alias_cannot_escape_to_an_unpinned_library() {
        let temp = tempfile::tempdir().unwrap();
        let [headers, mut provider] = parts(temp.path());
        provider.tree.entries.insert(
            PathBuf::from("install/lib/libopencv_core.so"),
            Entry::Symlink {
                target: "../../../outside.so".into(),
            },
        );
        assert!(manifest(&[headers, provider]).is_err());
    }
}
