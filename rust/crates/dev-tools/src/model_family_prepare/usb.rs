use super::{identity, json_file, real_parents, relative, require, write_json, Input};
use crate::{
    model_family_process::{held_usb_assets, verify_tree},
    seal::Result,
};
use flate2::bufread::GzDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
const RELEASE_SHA: &str = "6dc8965b2423f111631b6fb6eb9dc5fde3800f46b7785595f595fe348b66d807";
const MODEL_SHA: &str = "e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b";
const MANIFEST_SHA: &str = "1bb0c80c1f75dc757ad104162263eb87c0107385fab9ea563a7f9f98b46bf91e";
const RUNTIME_SHA: &str = "a25b81e90d5259c27bc49b3a93ab499c1909a33e133cba974cf023cd94727eeb";
const INSTALLED_SHA: &str = "dcb0d6838b185db179123dcb092f25dc2d3d03a3e0b1b94276a82bc15c8b69ab";
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
pub struct UsbOptions {
    pub release_archive: PathBuf,
    pub runtime_archive: PathBuf,
    pub installed: PathBuf,
    pub output: PathBuf,
}
struct Selected {
    assets: BTreeMap<String, Value>,
    licenses: BTreeMap<String, Value>,
}
fn extract(reader: impl BufRead, output: &Path) -> Result<Selected> {
    extract_release(reader, output, 1_708_026_925)
}
fn extract_release(reader: impl BufRead, output: &Path, expected_payload: u64) -> Result<Selected> {
    fs::create_dir(output.join("usb-assets"))?;
    fs::create_dir(output.join("usb-licenses"))?;
    let decoder = GzDecoder::new(reader);
    let bounded = decoder.take(2 * 1024 * 1024 * 1024 + 1);
    let mut tar = tar::Archive::new(bounded);
    let mut assets = BTreeMap::new();
    let mut licenses = BTreeMap::new();
    let mut total = 0u64;
    let mut scanned = 0u64;
    let mut members = 0;
    let mut seen = BTreeSet::new();
    for entry in tar.entries()?.raw(true) {
        let mut entry = entry?;
        members += 1;
        require(members <= 5000, "excessive USB release members")?;
        scanned = scanned
            .checked_add(entry.size())
            .ok_or("USB release expansion overflow")?;
        require(
            scanned <= expected_payload,
            "USB release expansion exceeds immutable release bound",
        )?;
        let name = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        let (kind, relative_name) = if let Some(name) = name.strip_prefix("rust/bin/usbgpu-assets/")
        {
            (true, name)
        } else if let Some(name) = name.strip_prefix("rust/bin/licenses/usbgpu/") {
            (false, name)
        } else {
            continue;
        };
        if entry.header().entry_type().is_dir() {
            continue;
        }
        require(
            entry.header().entry_type().is_file(),
            "USB release selected member is an alias or special file",
        )?;
        require(
            seen.insert(name.clone()),
            "duplicate USB release selected member",
        )?;
        let path = relative(relative_name)?;
        let map = if kind { &mut assets } else { &mut licenses };
        require(
            map.len() < if kind { 512 } else { 3 },
            "USB release selected roster exceeds bound",
        )?;
        let max = if kind { 800 * 1024 * 1024 } else { 1024 * 1024 };
        require(
            entry.size() > 0 && entry.size() <= max,
            "USB release selected file exceeds bound",
        )?;
        if !kind {
            require(
                path.components().count() == 1,
                "USB license path is not flat",
            )?;
        }
        total = total
            .checked_add(entry.size())
            .ok_or("USB asset size overflow")?;
        require(total <= MAX_TOTAL, "USB assets exceed aggregate bound")?;
        let target = output
            .join(if kind { "usb-assets" } else { "usb-licenses" })
            .join(path);
        fs::create_dir_all(target.parent().ok_or("USB asset parent missing")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut digest = Sha256::new();
        let mut copied = 0u64;
        let mut bytes = [0; 65536];
        loop {
            let n = entry.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            require(copied <= entry.size(), "USB selected file grew")?;
            file.write_all(&bytes[..n])?;
            digest.update(&bytes[..n]);
        }
        file.flush()?;
        require(copied == entry.size(), "USB selected file truncated")?;
        map.insert(
            relative_name.to_owned(),
            json!({"bytes":copied,"sha256":format!("{:x}",digest.finalize())}),
        );
    }
    let mut bounded = tar.into_inner();
    let mut tail = 0usize;
    let mut bytes = [0; 65536];
    loop {
        let n = bounded.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        tail += n;
        require(
            tail <= 1024 * 1024 && bytes[..n].iter().all(|&b| b == 0),
            "USB release tar has nonzero or excessive trailing bytes",
        )?;
    }
    require(
        bounded.limit() > 0,
        "USB release decompression bound exceeded",
    )?;
    let mut reader = bounded.into_inner().into_inner();
    require(
        reader.fill_buf()?.is_empty(),
        "USB release has compressed trailing bytes or another stream",
    )?;
    require(
        scanned == expected_payload,
        "USB release full payload size changed",
    )?;
    Ok(Selected { assets, licenses })
}
fn verify_selected(output: &Path, selected: &Selected) -> Result<()> {
    Input::open(&output.join("usb-runtime.tar.gz"), 2_731_184)?.expect(2_731_184, RUNTIME_SHA)?;
    Input::open(&output.join("usb-installed.json"), 879)?.expect(879, INSTALLED_SHA)?;
    let assets = output.join("usb-assets");
    let manifest = Input::open(&assets.join("manifest.json"), 1024 * 1024)?;
    manifest.expect(7154, MANIFEST_SHA)?;
    let value = json_file(&manifest.path)?;
    require(
        value["version"] == 1 && value["model_sha256"] == MODEL_SHA,
        "USB asset manifest model/version mismatch",
    )?;
    let mut expected = BTreeMap::new();
    for entry in value["files"]
        .as_array()
        .ok_or("USB companion roster missing")?
    {
        let name = entry["path"].as_str().ok_or("USB companion path missing")?;
        relative(name)?;
        require(
            expected
                .insert(
                    name.to_owned(),
                    json!({"bytes":entry["bytes"],"sha256":entry["sha256"]}),
                )
                .is_none(),
            "duplicate USB companion",
        )?;
    }
    expected.insert("manifest.json".to_owned(), manifest.seal.clone());
    let provider_path = assets.join("native-provider.json");
    let provider = Input::open(&provider_path, 1024 * 1024)?;
    expected.insert("native-provider.json".to_owned(), provider.seal.clone());
    let model = format!("models/{MODEL_SHA}/model.pkl");
    expected.insert(model.clone(), json!({"bytes":776634338,"sha256":MODEL_SHA}));
    require(
        expected == selected.assets,
        "USB asset exact whole roster or seals changed",
    )?;
    let provider = json_file(&provider_path)?;
    require(
        provider["version"] == 1
            && provider["kind"] == "native-generic-hcq"
            && provider["model_sha256"] == MODEL_SHA
            && provider["assets_manifest_sha256"] == MANIFEST_SHA
            && provider["runtime_archive_sha256"] == RUNTIME_SHA
            && provider["pickle"] == json!({"path":model,"size":776634338,"sha256":MODEL_SHA}),
        "USB native provider identity changed",
    )?;
    let license_expected = BTreeMap::from([
        (
            "LICENSE.amdgpu".to_owned(),
            json!({"bytes":2938,"sha256":"572872598565dc3513470de971a32bf9db301f47afeef3636345eadae33b2eee"}),
        ),
        (
            "build-provenance.json".to_owned(),
            json!({"bytes":1059,"sha256":"6cf74684477d01fb5b1622f671b852eb16b86072d57ebc6e56ae852ec4041e36"}),
        ),
        (
            "tinygrad-LICENSE".to_owned(),
            json!({"bytes":1058,"sha256":"00146150f1082f320d608ba481acc3fbfacf8a9914354c2c96b6f44719d5e977"}),
        ),
    ]);
    require(
        selected.licenses == license_expected,
        "USB license roster or seals changed",
    )?;
    for (name, seal) in &license_expected {
        require(
            Input::open(&output.join("usb-licenses").join(name), 1024 * 1024)?.seal == *seal,
            "USB license bytes changed",
        )?;
    }
    require(
        selected.licenses["build-provenance.json"] == selected.assets["source-provenance.json"]
            && selected.licenses["tinygrad-LICENSE"] == selected.assets["tinygrad-LICENSE"],
        "USB redistributed provenance/license differs",
    )?;
    let provenance = json_file(&assets.join("source-provenance.json"))?;
    require(
        provenance["pickle_sha256"] == MODEL_SHA && provenance["runtime_sha256"] == RUNTIME_SHA,
        "USB historical source provenance changed",
    )?;
    let installed = json_file(&output.join("usb-installed.json"))?;
    require(
        installed["model_checkpoint"] == provider["model_checkpoint"],
        "USB installed checkpoint differs from native provider",
    )?;
    let held = held_usb_assets(&assets, &serde_json::to_value(&expected)?)?;
    openpilot_usbgpu::model_delivery::assets::validate(&assets, MODEL_SHA, 776634338)?;
    verify_tree(&assets, &held)?;
    Ok(())
}
pub(super) struct Published {
    pub(super) path: PathBuf,
    pub(super) file: File,
    pub(super) accepted: bool,
}
impl Drop for Published {
    fn drop(&mut self) {
        if !self.accepted {
            if let (Ok(a), Ok(b)) = (self.file.metadata(), fs::symlink_metadata(&self.path)) {
                if b.is_dir()
                    && !b.file_type().is_symlink()
                    && (a.dev(), a.ino()) == (b.dev(), b.ino())
                {
                    let _ = fs::remove_dir_all(&self.path);
                }
            }
        }
    }
}
pub fn stage_usb_assets(options: &UsbOptions) -> Result<Value> {
    real_parents(&options.output)?;
    require(
        fs::symlink_metadata(&options.output)
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "USB preparation output exists or is inaccessible",
    )?;
    crate::disk_space::inspect(&options.output, MAX_TOTAL)?.require()?;
    let release = Input::open(&options.release_archive, 1_155_620_561)?;
    release.expect(1_155_620_561, RELEASE_SHA)?;
    let runtime = Input::open(&options.runtime_archive, 2_731_184)?;
    runtime.expect(2_731_184, RUNTIME_SHA)?;
    let installed = Input::open(&options.installed, 879)?;
    installed.expect(879, INSTALLED_SHA)?;
    let parent = options.output.parent().ok_or("USB output parent missing")?;
    let temp = tempfile::Builder::new()
        .prefix(".model-family-usb-")
        .tempdir_in(parent)?;
    let selected = extract(BufReader::new(release.file.try_clone()?), temp.path())?;
    runtime.copy_to(&temp.path().join("usb-runtime.tar.gz"))?;
    installed.copy_to(&temp.path().join("usb-installed.json"))?;
    verify_selected(temp.path(), &selected)?;
    let report = json!({"version":1,"source_release":"b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a","archive":release.seal,"runtime":runtime.seal,"installed":installed.seal,"assets_files":selected.assets,"licenses":selected.licenses,"model_pickle_executed":false,"release_executables_extracted":false,"scope":"authenticated genuine asset data only; no model/device execution"});
    write_json(&temp.path().join("usb-asset-admission.json"), &report)?;
    let mut receipt = Input::open(
        &temp.path().join("usb-asset-admission.json"),
        2 * 1024 * 1024,
    )?;
    for input in [&release, &runtime, &installed] {
        input.verify()?;
    }
    let directory = File::open(temp.path())?;
    let before = directory.metadata()?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temp.path(),
        rustix::fs::CWD,
        &options.output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _ = temp.keep();
    let mut published = Published {
        path: options.output.clone(),
        file: directory,
        accepted: false,
    };
    let after = fs::symlink_metadata(&options.output)?;
    require(
        after.is_dir()
            && !after.file_type().is_symlink()
            && (after.dev(), after.ino()) == (before.dev(), before.ino()),
        "USB output was replaced during publication",
    )?;
    verify_selected(&options.output, &selected)?;
    for input in [&release, &runtime, &installed] {
        input.verify()?;
    }
    receipt.path = options.output.join("usb-asset-admission.json");
    receipt.verify()?;
    require(
        identity(&published.file.metadata()?) == identity(&fs::symlink_metadata(&options.output)?),
        "USB published directory changed",
    )?;
    published.accepted = true;
    Ok(report)
}
pub fn usb_cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--release-archive", true),
            OptionSpec::value("--runtime-archive", true),
            OptionSpec::value("--installed", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!("Usage: stage-model-family-usb-assets --release-archive FILE --runtime-archive FILE --installed JSON --output DIR");
        return Ok(());
    }
    let path = |name: &str| -> Result<PathBuf> {
        Ok(PathBuf::from(
            parsed
                .values
                .get(name)
                .ok_or("USB preparation option missing")?,
        ))
    };
    let report = stage_usb_assets(&UsbOptions {
        release_archive: path("--release-archive")?,
        runtime_archive: path("--runtime-archive")?,
        installed: path("--installed")?,
        output: path("--output")?,
    })?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::io::Cursor;
    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
        gzip.write_all(bytes).unwrap();
        gzip.finish().unwrap()
    }
    fn archive(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for &(name, bytes, link) in entries {
            let mut header = tar::Header::new_ustar();
            header.set_mode(0o644);
            header.set_size(bytes.len() as u64);
            header.set_entry_type(if link {
                tar::EntryType::Symlink
            } else {
                tar::EntryType::Regular
            });
            if link {
                header.set_link_name("foreign").unwrap();
            }
            // Raw headers let the adversarial test express a path rejected by set_path.
            header.as_mut_bytes()[..100].fill(0);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            tar.append(&header, bytes).unwrap();
        }
        tar.into_inner().unwrap()
    }
    #[test]
    fn selected_archive_rejects_alias_duplicate_escape_and_nonzero_or_compressed_tail() {
        let name = "rust/bin/usbgpu-assets/data";
        let valid = archive(&[(name, b"abc", false)]);
        let root = tempfile::tempdir().unwrap();
        let selected = extract_release(Cursor::new(gzip(&valid)), root.path(), 3).unwrap();
        assert_eq!(
            selected.assets["data"],
            json!({"bytes":3,"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"})
        );
        let mut bad_tar = valid.clone();
        bad_tar.extend_from_slice(b"bad-tail");
        let mut bad_gzip = gzip(&valid);
        bad_gzip.extend_from_slice(b"garbage");
        let mut second_stream = gzip(&valid);
        second_stream.extend_from_slice(&gzip(b"second stream"));
        for (bytes, total) in [
            (gzip(&archive(&[(name, b"", true)])), 0),
            (
                gzip(&archive(&[(name, b"a", false), (name, b"b", false)])),
                2,
            ),
            (
                gzip(&archive(&[(
                    "rust/bin/usbgpu-assets/../escape",
                    b"a",
                    false,
                )])),
                1,
            ),
            (gzip(&bad_tar), 3),
            (bad_gzip, 3),
            (second_stream, 3),
        ] {
            let root = tempfile::tempdir().unwrap();
            assert!(extract_release(Cursor::new(bytes), root.path(), total).is_err());
        }
    }
    #[test]
    fn oversize_header_fails_without_payload_allocation_and_failed_publication_preserves_foreign_root(
    ) {
        let mut header = tar::Header::new_ustar();
        header.set_path("rust/bin/usbgpu-assets/too-big").unwrap();
        header.set_mode(0o644);
        header.set_size(800 * 1024 * 1024 + 1);
        header.set_cksum();
        let mut bytes = header.as_bytes().to_vec();
        bytes.extend_from_slice(&[0; 1024]);
        let root = tempfile::tempdir().unwrap();
        assert!(extract_release(
            Cursor::new(gzip(&bytes)),
            root.path(),
            800 * 1024 * 1024 + 1
        )
        .is_err());
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        fs::create_dir(&output).unwrap();
        let file = File::open(&output).unwrap();
        let guard = Published {
            path: output.clone(),
            file,
            accepted: false,
        };
        fs::rename(&output, root.path().join("owned-away")).unwrap();
        fs::create_dir(&output).unwrap();
        fs::write(output.join("foreign"), b"foreign").unwrap();
        drop(guard);
        assert_eq!(fs::read(output.join("foreign")).unwrap(), b"foreign");
    }
}
