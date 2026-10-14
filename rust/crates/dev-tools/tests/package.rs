use openpilot_dev_tools::{
    package::{package, validate_assets},
    seal::digest,
};
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    process::Command,
};
#[path = "common/release.rs"]
mod release;
use release::{elf, git, Fixture};
#[test]
fn deterministic_release_and_native_cli() {
    let f = Fixture::new();
    let v = f.run().unwrap();
    let name = v["archive"]["name"].as_str().unwrap();
    let repeat = f.output.with_file_name("repeat");
    package(&f.root, &f.installer, &f.receipt, &repeat).unwrap();
    for e in fs::read_dir(&f.output).unwrap() {
        let p = e.unwrap().path();
        assert_eq!(
            fs::read(&p).unwrap(),
            fs::read(repeat.join(p.file_name().unwrap())).unwrap()
        );
    }
    validate_assets(&f.root, &f.output).unwrap();
    let mut a = tar::Archive::new(flate2::read::GzDecoder::new(
        fs::File::open(f.output.join(name)).unwrap(),
    ));
    let mut alias = false;
    for e in a.entries().unwrap() {
        let e = e.unwrap();
        assert_eq!(e.header().uid().unwrap(), 0);
        assert_eq!(e.header().gid().unwrap(), 0);
        if e.path().unwrap() == Path::new("rust/bin/kernel-alias") {
            assert_eq!(
                e.link_name().unwrap().unwrap(),
                Path::new("usbgpu-assets/kernel")
            );
            alias = true;
        }
    }
    assert!(alias);
    let o = Command::new(env!("CARGO_BIN_EXE_package-runtime-release"))
        .args(["--root"])
        .arg(&f.root)
        .arg("--output")
        .arg(&f.output)
        .arg("--verify-only")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(package(&f.root, &f.installer, &f.receipt, &f.output).is_err());
}
#[test]
fn invalid_inputs_fail_without_publishing() {
    for fault in [
        "dirty",
        "commit",
        "hash",
        "mode",
        "unexpected",
        "escape",
        "duplicate",
        "boot",
        "wrong-repo",
        "installer-arch",
        "installer-interpreter",
        "installer-needed",
        "receipt-commit",
        "receipt-hash",
        "oversized",
        "directory-alias",
        "json-duplicate",
        "unsafe-mode",
        "bad-static-header",
    ] {
        let f = Fixture::new();
        let binary = f.root.join("rust/bin/openpilot-launch");
        match fault {
            "dirty" => fs::write(f.root.join("source.txt"), "dirty").unwrap(),
            "commit" => f.manifest(|v| v["source_commit"] = "f".repeat(40).into()),
            "hash" => fs::write(&binary, [elf(183, 0), b"tamper".to_vec()].concat()).unwrap(),
            "mode" => fs::set_permissions(&binary, fs::Permissions::from_mode(0o644)).unwrap(),
            "unsafe-mode" => {
                fs::set_permissions(&binary, fs::Permissions::from_mode(0o4755)).unwrap()
            }
            "unexpected" => fs::write(f.root.join("rust/bin/unsealed"), "extra").unwrap(),
            "escape" => {
                fs::remove_file(&binary).unwrap();
                symlink("/etc/passwd", &binary).unwrap();
            }
            "duplicate" => f.manifest(|v| {
                let row = v["files"][0].clone();
                v["files"].as_array_mut().unwrap().push(row);
            }),
            "boot" => {
                f.manifest(|v| {
                    v["files"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|r| r["path"] != "rust/bin/openpilot-launch")
                });
                fs::remove_file(&binary).unwrap();
            }
            "wrong-repo" => {
                git(
                    &f.root,
                    &[
                        "remote",
                        "set-url",
                        "origin",
                        "https://github.com/bin9208/openpilot.git",
                    ],
                );
            }
            "installer-arch" => fs::write(&f.installer, elf(62, 0)).unwrap(),
            "installer-interpreter" => fs::write(&f.installer, elf(183, 3)).unwrap(),
            "installer-needed" => fs::write(&f.installer, elf(183, 2)).unwrap(),
            "receipt-commit" => f.edit(&f.receipt, |v| v["source_commit"] = "f".repeat(40).into()),
            "receipt-hash" => f.edit(&f.receipt, |v| {
                v["installer_sha256"] = "f".repeat(64).into()
            }),
            "oversized" => fs::File::create(&binary)
                .unwrap()
                .set_len(2 * 1024 * 1024 * 1024)
                .unwrap(),
            "directory-alias" => {
                fs::rename(
                    f.root.join("rust/bin/usbgpu-assets"),
                    f.root.join("rust/bin/assets-owned"),
                )
                .unwrap();
                symlink("assets-owned", f.root.join("rust/bin/usbgpu-assets")).unwrap();
            }
            "json-duplicate" => {
                let p = f.root.join("rust/runtime-manifest.json");
                let s = fs::read_to_string(&p).unwrap();
                fs::write(p, s.replacen('{', "{\"schema\":1,", 1)).unwrap();
            }
            "bad-static-header" => {
                let mut b = elf(183, 0);
                b[54] = 0;
                fs::write(&f.installer, b).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(f.run().is_err(), "accepted {fault}");
        assert!(!f.output.exists(), "published {fault}");
    }
}
#[test]
fn verifier_rejects_tampered_release() {
    for fault in [
        "descriptor",
        "archive",
        "helper",
        "sidecar",
        "extra",
        "archive-mode",
        "archive-escape",
        "archive-duplicate",
        "archive-footer",
        "archive-hidden",
        "archive-pax",
        "archive-padding",
    ] {
        let f = Fixture::new();
        let v = f.run().unwrap();
        let archive = f.output.join(v["archive"]["name"].as_str().unwrap());
        match fault {
            "descriptor" => f.edit(&f.output.join("runtime-release.json"), |v| {
                v["archive"]["unpacked_bytes"] = 0.into()
            }),
            "archive" | "archive-footer" => {
                let mut b = fs::read(&archive).unwrap();
                b.truncate(b.len() - 4);
                fs::write(&archive, b).unwrap();
                if fault == "archive-footer" {
                    f.refresh_archive(&archive);
                }
            }
            "helper" => fs::write(
                f.output.join("openpilot-runtime-install-aarch64"),
                b"changed",
            )
            .unwrap(),
            "sidecar" => fs::write(
                archive.with_file_name(format!(
                    "{}.sha256",
                    archive.file_name().unwrap().to_str().unwrap()
                )),
                "bad",
            )
            .unwrap(),
            "extra" => fs::write(f.output.join("extra"), "extra").unwrap(),
            "archive-hidden" => {
                let mut c =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                c.write_all(b"hidden unmanifested bytes").unwrap();
                fs::OpenOptions::new()
                    .append(true)
                    .open(&archive)
                    .unwrap()
                    .write_all(&c.finish().unwrap())
                    .unwrap();
                f.refresh_archive(&archive);
            }
            _ => {
                let mut bytes = Vec::new();
                flate2::read::GzDecoder::new(fs::File::open(&archive).unwrap())
                    .read_to_end(&mut bytes)
                    .unwrap();
                if fault == "archive-mode" {
                    bytes[100..108].copy_from_slice(b"0000600\0");
                    checksum(&mut bytes[..512]);
                } else if fault == "archive-padding" {
                    bytes.extend(vec![0; 11000]);
                } else {
                    let mut offset = 0;
                    while bytes[offset..offset + 512].iter().any(|b| *b != 0) {
                        let size = octal(&bytes[offset + 124..offset + 136]);
                        offset += 512 + size.div_ceil(512) * 512;
                    }
                    let mut h = tar::Header::new_ustar();
                    h.set_path(if fault == "archive-escape" {
                        "escape"
                    } else {
                        "rust/runtime-manifest.json"
                    })
                    .unwrap();
                    if fault == "archive-escape" {
                        h.as_mut_bytes()[..9].copy_from_slice(b"../escape");
                    }
                    h.set_size(0);
                    h.set_mode(0o644);
                    h.set_mtime(
                        git(&f.root, &["show", "-s", "--format=%ct", "HEAD"])
                            .parse()
                            .unwrap(),
                    );
                    if fault == "archive-pax" {
                        h.set_entry_type(tar::EntryType::XHeader);
                    }
                    h.set_cksum();
                    bytes.splice(offset..offset, h.as_bytes().iter().copied());
                }
                let mut header = [0; 10];
                header.copy_from_slice(&fs::read(&archive).unwrap()[..10]);
                let time = u32::from_le_bytes(header[4..8].try_into().unwrap());
                let mut c = flate2::GzBuilder::new()
                    .mtime(time)
                    .operating_system(255)
                    .write(Vec::new(), flate2::Compression::new(6));
                c.write_all(&bytes).unwrap();
                fs::write(&archive, c.finish().unwrap()).unwrap();
                f.refresh_archive(&archive);
            }
        }
        assert!(
            validate_assets(&f.root, &f.output).is_err(),
            "accepted {fault}"
        );
        assert!(archive.exists());
    }
}
fn octal(b: &[u8]) -> usize {
    usize::from_str_radix(std::str::from_utf8(b).unwrap().trim_matches(['\0', ' ']), 8).unwrap()
}
fn checksum(b: &mut [u8]) {
    b[148..156].fill(b' ');
    let sum: u32 = b.iter().map(|b| u32::from(*b)).sum();
    b[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
}

#[test]
fn source_asset_activation_reserve_matches_installer_limit() {
    for (runtime, second_mib, accepted) in [(false, 65, false), (false, 64, true), (true, 65, true)]
    {
        let f = Fixture::new();
        openpilot_dev_tools::package::disk_guard(&f.root, 400 * 1024 * 1024).unwrap();
        let names = if runtime {
            [
                "rust/bin/usbgpu-assets/large-owned-first",
                "rust/bin/usbgpu-assets/large-owned-second",
            ]
        } else {
            [
                "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf",
                "openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx",
            ]
        };
        let ignored = f.root.join(".gitignore");
        let mut text = fs::read_to_string(&ignored).unwrap();
        for name in names {
            text.push_str(name);
            text.push('\n');
        }
        fs::write(&ignored, text).unwrap();
        git(&f.root, &["add", ".gitignore"]);
        git(
            &f.root,
            &["commit", "-qm", "ignore owned sparse reserve fixtures"],
        );
        let commit = git(&f.root, &["rev-parse", "HEAD"]);
        let mut rows = Vec::new();
        for (name, mib) in names.into_iter().zip([64, second_mib]) {
            let p = f.root.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            let mut file = fs::File::create(&p).unwrap();
            file.write_all(b"owned sparse activation-reserve fixture\n")
                .unwrap();
            file.set_len(mib * 1024 * 1024).unwrap();
            rows.push(json!({"path":name,"sha256":digest(&p).unwrap(),"executable":false}));
        }
        f.manifest(|v| {
            v["source_commit"] = commit.clone().into();
            v["files"].as_array_mut().unwrap().extend(rows);
        });
        f.edit(&f.receipt, |v| v["source_commit"] = commit.into());
        let result = f.run();
        if accepted {
            result.unwrap();
            validate_assets(&f.root, &f.output).unwrap();
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("source assets exceed 128 MiB"));
            assert!(!f.output.exists());
        }
    }
}

#[test]
fn packaged_installer_must_match_the_verified_standalone_installer() {
    for dynamic in [false, true] {
        let f = Fixture::new();
        let helper = f.root.join("rust/bin/openpilot-runtime-install");
        let bytes = if dynamic {
            elf(183, 2)
        } else {
            [elf(183, 0), b"different native helper".to_vec()].concat()
        };
        fs::write(&helper, bytes).unwrap();
        f.manifest(|manifest| {
            let row = manifest["files"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|row| row["path"] == "rust/bin/openpilot-runtime-install")
                .unwrap();
            row["sha256"] = digest(&helper).unwrap().into();
        });
        let result = f.run();
        assert!(
            result.is_err(),
            "accepted different hash-sealed packaged helper (dynamic={dynamic})"
        );
        assert!(!f.output.exists());
    }
}

#[test]
fn verifier_rejects_self_consistent_standalone_replacement_that_differs_from_packaged_helper() {
    let f = Fixture::new();
    let descriptor = f.run().unwrap();
    let archive = f
        .output
        .join(descriptor["archive"]["name"].as_str().unwrap());
    let original_archive = fs::read(&archive).unwrap();
    let standalone = f.output.join("openpilot-runtime-install-aarch64");
    fs::write(
        &standalone,
        [elf(183, 0), b"different static installer".to_vec()].concat(),
    )
    .unwrap();
    f.edit(&f.output.join("runtime-installer-build.json"), |receipt| {
        receipt["installer_sha256"] = digest(&standalone).unwrap().into()
    });
    fs::write(
        f.output.join("openpilot-runtime-install-aarch64.sha256"),
        format!(
            "{}  openpilot-runtime-install-aarch64\n",
            digest(&standalone).unwrap()
        ),
    )
    .unwrap();
    assert!(
        validate_assets(&f.root, &f.output).is_err(),
        "accepted self-consistent standalone installer that differs from the runtime helper"
    );
    assert_eq!(fs::read(archive).unwrap(), original_archive);
}
