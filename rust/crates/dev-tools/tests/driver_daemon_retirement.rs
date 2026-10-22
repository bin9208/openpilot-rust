//! Hosted grouped same-ELF replay; fixture/model/receipt preparation is external.
#![cfg(all(target_os = "linux", feature = "driver-daemon-native"))]
use openpilot_dev_tools::driver_daemon_check::RETIRED;
use openpilot_dev_tools::qa_process::{Captured, CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
    },
    path::{Path, PathBuf},
    time::Duration,
};
fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}
fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn execute(cli: &Path, args: &[OsString], proof: &Path) -> Captured {
    fs::create_dir(proof).unwrap();
    let argv = std::iter::once(cli.as_os_str().to_owned())
        .chain(args.iter().cloned())
        .collect();
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(20 * 60));
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    spec.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    let result = NativeRunner.run_to_files(&spec, &proof.join("stdout"), &proof.join("stderr"));
    let stdout = fs::read(proof.join("stdout")).unwrap();
    let stderr = fs::read(proof.join("stderr")).unwrap();
    fs::write(proof.join("invocation.json"), serde_json::to_vec_pretty(&json!({"argv_bytes":std::iter::once(cli.as_os_str()).chain(args.iter().map(OsString::as_os_str)).map(|a|a.as_bytes().to_vec()).collect::<Vec<_>>(),"status":result.as_ref().ok().map(|r|r.code),"error":result.as_ref().err().map(ToString::to_string),"timeout_seconds":spec.timeout.as_secs(),"stream_limit_bytes":spec.stdout_limit,"stdout_sha256":format!("{:x}",Sha256::digest(&stdout)),"stderr_sha256":format!("{:x}",Sha256::digest(&stderr))})).unwrap()).unwrap();
    Captured {
        code: result.unwrap().code,
        stdout,
        stderr,
    }
}
#[test]
#[ignore = "postdelete complete fixture, current-host export and two source roots required"]
fn same_elf_replays_both_modes_in_both_roots_and_rejects_all_retired_sources() {
    let cli = required("DRIVER_RETIREMENT_CLI");
    let daemon = required("DRIVER_RETIREMENT_BINARY");
    let collector = required("DRIVER_RETIREMENT_COLLECTOR");
    let peer = required("DRIVER_RETIREMENT_PEER");
    let fixture = required("DRIVER_RETIREMENT_FIXTURE");
    let catalog = required("DRIVER_RETIREMENT_CATALOG");
    let receipt = required("DRIVER_RETIREMENT_CATALOG_RECEIPT");
    let proof = required("DRIVER_RETIREMENT_OUTPUT");
    fs::create_dir(&proof).unwrap();
    let held = [&cli, &daemon, &collector, &peer, &receipt].map(|p| (p.clone(), sha(p)));
    let receipt_sha = sha(&receipt);
    let roots = [
        required("DRIVER_RETIREMENT_CHECKOUT_ROOT"),
        required("DRIVER_RETIREMENT_PRIVATE_ROOT"),
    ];
    let mut accepted = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        for (path, _) in RETIRED {
            assert!(fs::symlink_metadata(root.join(path)).is_err());
        }
        let root_proof = proof.join(format!("root-{index}"));
        fs::create_dir(&root_proof).unwrap();
        let alias = root_proof.join(OsString::from_vec(b"raw-\xff".to_vec()));
        symlink(&root_proof, &alias).unwrap();
        for (mode, original, source, manifest) in [
            (
                "collector",
                "collector-current",
                "2ecf3f6813f601ea52e7a61ccf75219952daefa0",
                "532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795",
            ),
            (
                "no-collector",
                "no-collector",
                "481ae6fec39ecc68786c4c2813b1205fb4567141",
                "1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380",
            ),
        ] {
            let output = alias.join(format!("{mode}-output"));
            let mut args: Vec<OsString> = Vec::new();
            for (name, value) in [
                ("--binary", daemon.clone()),
                ("--catalog", catalog.clone()),
                ("--models", fixture.join("no-collector/models")),
                ("--output", output),
                ("--native-peer", peer.clone()),
                (
                    "--native-original-dir",
                    fixture.join(original).join("observations"),
                ),
                ("--native-original-manifest-sha256", manifest.into()),
                ("--native-source-commit", source.into()),
                ("--native-root", root.clone()),
                ("--native-source-policy", "retired-driver".into()),
                ("--native-current-catalog-receipt", receipt.clone()),
                (
                    "--native-current-catalog-receipt-sha256",
                    receipt_sha.clone().into(),
                ),
            ] {
                args.push(name.into());
                args.push(value.into_os_string());
            }
            if mode == "collector" {
                args.push("--collector".into());
                args.push(collector.clone().into_os_string());
            }
            let result = execute(&cli, &args, &root_proof.join(format!("{mode}-cli")));
            assert!(
                result.code == 0,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let report: Value = serde_json::from_slice(
                &fs::read(root_proof.join(format!("{mode}-output/report.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(report["source_policy"], "retired-driver");
            assert_eq!(report["source_commit"], source);
            assert_eq!(report["current_catalog"]["receipt"]["sha256"], receipt_sha);
            let runs = report["runs"].as_array().unwrap();
            assert_eq!(runs.len(), 4);
            for run in runs {
                assert_eq!(run["logging"].is_null(), mode == "no-collector");
                if mode == "collector" {
                    assert_ne!(report["checkout_commit"], report["source_commit"]);
                    assert_eq!(run["logging"]["source_commit"], report["checkout_commit"]);
                }
            }
            assert_eq!(String::from_utf8(result.stdout).unwrap().lines().count(), 4);
            accepted.push(json!({"root":index,"mode":mode,"report":root_proof.join(format!("{mode}-output/report.json")),"report_sha256":sha(&root_proof.join(format!("{mode}-output/report.json")))}));
            if mode == "no-collector" {
                for (n, (name, _)) in RETIRED.iter().enumerate() {
                    let path = root.join(name);
                    for link in [false, true] {
                        if link {
                            symlink(root_proof.join("nonexistent"), &path).unwrap()
                        } else {
                            fs::write(&path, b"retired source reappearance").unwrap()
                        }
                        let bad_output = root_proof.join(format!("negative-{n}-{link}-output"));
                        let mut bad = args.clone();
                        let position = bad.iter().position(|a| a == "--output").unwrap();
                        bad[position + 1] = bad_output.clone().into_os_string();
                        let result =
                            execute(&cli, &bad, &root_proof.join(format!("negative-{n}-{link}")));
                        fs::remove_file(&path).unwrap();
                        assert_ne!(result.code, 0);
                        assert!(!bad_output.exists());
                        assert!(result.stdout.is_empty());
                        assert!(String::from_utf8_lossy(&result.stderr)
                            .contains("retired driver source present"));
                    }
                }
            }
        }
        fs::remove_file(alias).unwrap();
    }
    for (path, digest) in held {
        assert_eq!(sha(&path), digest);
    }
    fs::write(proof.join("accepted.json"),serde_json::to_vec_pretty(&json!({"runs":accepted,"device_validation":false,"same_elf":true,"negative_controls":roots.len()*RETIRED.len()*2,"original_source_restored":false})).unwrap()).unwrap();
}
