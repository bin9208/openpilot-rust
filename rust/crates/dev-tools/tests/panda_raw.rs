use openpilot_dev_tools::{
    panda_oracle_build,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::Digest;
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
    },
    path::{Path, PathBuf},
};
fn decode(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap())
        .collect()
}
fn replace(bytes: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut rest = bytes;
    let mut out = vec![];
    while let Some(at) = rest.windows(old.len()).position(|s| s == old) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(new);
        rest = &rest[at + old.len()..];
    }
    out.extend_from_slice(rest);
    out
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn canonical(value: &JsonValue) -> String {
    match value.view() {
        JsonView::Object(fields) => {
            let mut fields = fields
                .into_iter()
                .map(|(k, v)| (k.to_vec(), v))
                .collect::<Vec<_>>();
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            format!(
                "{{{}}}",
                fields
                    .into_iter()
                    .map(|(k, v)| format!(
                        "{}:{}",
                        JsonValue::codepoints(k).unwrap().to_json().unwrap(),
                        canonical(&v)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        JsonView::Array(items) => format!(
            "[{}]",
            items.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => value.to_json().unwrap(),
    }
}
struct Peer<'a> {
    base: &'a Path,
    source: &'a Path,
    commands: Vec<Vec<Vec<u8>>>,
    fail: usize,
    text: Option<(usize, String, i32)>,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let argv = spec.effective_argv();
        self.commands.push(
            argv.iter()
                .map(|a| {
                    replace(
                        &replace(a.as_bytes(), self.base.as_os_str().as_bytes(), b"BASE"),
                        self.source.as_os_str().as_bytes(),
                        b"ROOT",
                    )
                })
                .collect(),
        );
        let targeted = self
            .text
            .as_ref()
            .filter(|(stage, _, _)| *stage == self.commands.len());
        let code =
            targeted
                .map(|(_, _, code)| *code)
                .unwrap_or(if self.commands.len() == self.fail {
                    9
                } else {
                    0
                });
        if code == 0 || self.text.is_some() {
            if argv[0] == "capnp" {
                let dir = argv
                    .iter()
                    .find_map(|arg| arg.as_bytes().strip_prefix(b"-oc++:"))
                    .unwrap();
                let dir = PathBuf::from(OsString::from_vec(dir.to_vec()));
                for name in ["car", "custom", "deprecated", "log"] {
                    fs::write(
                        dir.join(format!("{name}.capnp.c++")),
                        format!("controlled generated {name}\n"),
                    )?;
                }
            } else {
                let at = argv.iter().position(|v| v == "-o").unwrap();
                fs::write(Path::new(&argv[at + 1]), b"controlled compiler product\n")?;
            }
        }
        let (stdout, stderr) = if let Some((_, fault, _)) = targeted {
            (
                if fault == "invalid-stdout" {
                    b"out-\xff".to_vec()
                } else {
                    "out-\r\nx\ry\n한😀".as_bytes().to_vec()
                },
                if fault == "invalid-stderr" {
                    b"err-\xff".to_vec()
                } else {
                    "err-\r\nx\ry\n한😀".as_bytes().to_vec()
                },
            )
        } else {
            (
                b"controlled stdout\n".to_vec(),
                b"controlled stderr\n".to_vec(),
            )
        };
        Ok(Captured {
            code,
            stdout,
            stderr,
        })
    }
}
#[test]
fn twenty_nine_original_raw_builds_preserve_all_command_bytes_receipts_and_failures() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/panda-raw/original.json")).unwrap();
    verify(&fixture);
}
fn verify(fixture: &serde_json::Value) {
    let source = root();
    const HISTORICAL_DEVICE_ORACLE: &str =
        "dbf7b2de42a945047f3c0d8f5450f3e89e16452cb454c4b922ee81b799347468";
    const CURRENT_DEVICE_ORACLE: &str =
        "96a8b74bf789bf96a91067bf4a174aaca3bbd455133eb4c48bdd8a6598a09197";
    assert_eq!(
        format!(
            "{:x}",
            sha2::Sha256::digest(
                fs::read(source.join("rust/tools/pandad_device_source.cc")).unwrap()
            )
        ),
        CURRENT_DEVICE_ORACLE
    );
    let cases = fixture["cases"].as_array().unwrap();
    for (case_index, row) in cases.iter().enumerate() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let raw = |name: &str| base.join(OsString::from_vec([name.as_bytes(), b"-\xff"].concat()));
        let buildroot = if row["rawroot"] == true {
            let root = raw("root");
            fs::create_dir_all(root.join("rust/tools")).unwrap();
            for top in ["openpilot", "msgq_repo", "panda", "opendbc_repo"] {
                symlink(source.join(top), root.join(top)).unwrap();
            }
            for entry in fs::read_dir(source.join("rust/tools")).unwrap() {
                let p = entry.unwrap().path();
                if p.extension().is_some_and(|e| e == "cc" || e == "h") {
                    symlink(&p, root.join("rust/tools").join(p.file_name().unwrap())).unwrap();
                }
            }
            root
        } else {
            source.clone()
        };
        let usb = raw("usb").join("libusb-1.0/libusb.h");
        fs::create_dir_all(usb.parent().unwrap()).unwrap();
        fs::write(usb, b"controlled header\n").unwrap();
        let args = row["args_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                OsString::from_vec(replace(
                    &decode(a.as_str().unwrap()),
                    b"BASE",
                    base.as_os_str().as_bytes(),
                ))
            })
            .collect::<Vec<_>>();
        let fail = row["status"]
            .as_str()
            .unwrap()
            .strip_prefix("failure-")
            .map(|n| n.parse().unwrap())
            .unwrap_or(0);
        let mut peer = Peer {
            base,
            source: &source,
            commands: vec![],
            fail,
            text: row.get("fault").map(|fault| {
                (
                    row["stage"].as_u64().unwrap() as usize,
                    fault.as_str().unwrap().to_owned(),
                    row["code"].as_i64().unwrap() as i32,
                )
            }),
        };
        let result = panda_oracle_build::run_os(
            &buildroot,
            row["family"].as_str().unwrap(),
            &args,
            &mut peer,
            40 * 1024u64.pow(3),
        );
        assert_eq!(result.is_ok(), row["error"].is_null(), "{row}: {result:?}");
        let wanted = row["commands_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                c.as_array()
                    .unwrap()
                    .iter()
                    .map(|a| decode(a.as_str().unwrap()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(peer.commands, wanted, "{row}");
        let output = raw("out");
        let actual = fs::read_dir(&output)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "json" || e == "inc"))
            .collect::<Vec<_>>();
        assert_eq!(
            actual.len(),
            row["files"].as_object().unwrap().len(),
            "{row}"
        );
        if let Some(expected) = row.get("artifacts") {
            fn walk(
                base: &Path,
                dir: &Path,
                files: &mut serde_json::Map<String, serde_json::Value>,
            ) {
                use sha2::{Digest, Sha256};
                for entry in fs::read_dir(dir).unwrap() {
                    let p = entry.unwrap().path();
                    if p.is_dir() {
                        walk(base, &p, files);
                    } else if !p.extension().is_some_and(|e| e == "json" || e == "inc") {
                        files.insert(
                            p.strip_prefix(base).unwrap().to_str().unwrap().to_owned(),
                            serde_json::Value::String(format!(
                                "{:x}",
                                Sha256::digest(fs::read(p).unwrap())
                            )),
                        );
                    }
                }
            }
            let mut files = serde_json::Map::new();
            walk(&output, &output, &mut files);
            assert_eq!(&serde_json::Value::Object(files), expected, "{row}");
        }
        for (name, expected) in row["files"].as_object().unwrap() {
            let text = fs::read_to_string(output.join(name))
                .unwrap()
                .replace(base.to_str().unwrap(), "BASE")
                .replace(source.to_str().unwrap(), "ROOT");
            if name.ends_with(".json") {
                let historical = expected.as_str().unwrap();
                let expected = if historical.contains(HISTORICAL_DEVICE_ORACLE) {
                    assert_eq!(cases.len(), 29, "unexpected historical fixture family");
                    assert!(matches!(case_index, 4 | 5), "unexpected historical case");
                    assert_eq!(name, "manifest.json");
                    assert_eq!(historical.matches(HISTORICAL_DEVICE_ORACLE).count(), 1);
                    historical.replace(HISTORICAL_DEVICE_ORACLE, CURRENT_DEVICE_ORACLE)
                } else {
                    historical.to_owned()
                };
                assert_eq!(
                    canonical(&JsonValue::parse(&text).unwrap()),
                    canonical(&JsonValue::parse(&expected).unwrap()),
                    "{name} {row}"
                );
            } else {
                assert_eq!(text, expected.as_str().unwrap(), "{name} {row}");
            }
        }
    }
}

#[test]
fn forty_two_original_strict_text_and_universal_newline_command_stages() {
    let fixture =
        serde_json::from_str(include_str!("fixtures/panda-raw/text-original.json")).unwrap();
    verify(&fixture);
}
