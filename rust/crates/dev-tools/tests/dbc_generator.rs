//! Independent original captures; no generated fixture is a runtime input.
use openpilot_dev_tools::dbc_generator::{self as native, Program};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/dbc-generator")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}
fn write_pairs(root: &Path, pairs: &Value) {
    for pair in pairs.as_array().unwrap() {
        let path = root.join(pair[0].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes(pair[1].as_str().unwrap())).unwrap();
    }
}
fn tree(root: &Path) -> Value {
    let mut files = BTreeMap::new();
    let mut links = BTreeMap::new();
    let mut directories = Vec::new();
    fn visit(
        base: &Path,
        path: &Path,
        files: &mut BTreeMap<String, String>,
        links: &mut BTreeMap<String, String>,
        directories: &mut Vec<String>,
    ) {
        if !path.exists() {
            return;
        }
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = path
                .strip_prefix(base)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                links.insert(
                    name,
                    fs::read_link(path).unwrap().to_str().unwrap().to_owned(),
                );
            } else if kind.is_dir() {
                directories.push(name);
                visit(base, &path, files, links, directories);
            } else {
                files.insert(name, hex(&fs::read(path).unwrap()));
            }
        }
    }
    visit(root, root, &mut files, &mut links, &mut directories);
    directories.sort();
    json!({"files":files,"links":links,"directories":directories})
}
fn error<T>(result: Result<T, native::Error>) -> Value {
    match result {
        Ok(_) => Value::Null,
        Err(error) => json!({"type":error.kind(),"errno":error.raw_os_error()}),
    }
}

#[test]
fn original_composition_bytes_newlines_include_order_and_partial_failures() {
    let cases = fixture("compose-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 15);
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        write_pairs(&source, &case["inputs"]);
        if case["output_exists"].as_bool().unwrap() {
            fs::create_dir(&output).unwrap();
        }
        write_pairs(&output, &case["initial_outputs"]);
        let actual = error(native::create_dbc(
            &source,
            Path::new(case["filename"].as_str().unwrap()),
            &output,
        ));
        assert_eq!(actual, case["error"], "{}", case["name"]);
        assert_eq!(tree(&output)["files"], case["outputs"], "{}", case["name"]);
    }
}

#[test]
fn original_directory_walk_cleanup_filters_and_collision_order() {
    let cases = fixture("create-all-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 4);
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&output).unwrap();
        write_pairs(&source, &case["inputs"]);
        write_pairs(&output, &case["initial_outputs"]);
        for name in case["initial_directories"].as_array().unwrap() {
            fs::create_dir(output.join(name.as_str().unwrap())).unwrap();
        }
        let expected = case["error"].clone();
        #[cfg(target_os = "macos")]
        let expected = if case["name"] == "remove-matching-directory-first" {
            // Keep the Linux capture immutable. Darwin unlink(directory) uses
            // EPERM; CPython maps its actual errno to PermissionError.
            // https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/unlink.2.html
            // https://docs.python.org/3.12/library/exceptions.html#PermissionError
            assert_eq!(expected, json!({"type":"IsADirectoryError","errno":21}));
            json!({"type":"PermissionError","errno":1})
        } else {
            expected
        };
        assert_eq!(
            error(native::create_all(&output, &source, &[])),
            expected,
            "{}",
            case["name"]
        );
        let actual = tree(&output);
        assert_eq!(actual["files"], case["outputs"], "{}", case["name"]);
        assert_eq!(
            actual["directories"], case["directories"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn original_stage_existing_files_links_exclusive_create_and_generation_order() {
    let cases = fixture("stage-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 5);
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path();
        for entry in case["existing"].as_array().unwrap() {
            let path = output.join(entry[0].as_str().unwrap());
            match entry[1].as_str().unwrap() {
                "file" => fs::write(path, entry[2].as_str().unwrap()).unwrap(),
                "directory" => fs::create_dir(path).unwrap(),
                "symlink" => std::os::unix::fs::symlink(entry[2].as_str().unwrap(), path).unwrap(),
                other => panic!("unknown captured fixture kind {other}"),
            }
        }
        let generated: Vec<(String, String)> =
            serde_json::from_value(case["generated"].clone()).unwrap();
        // This is the same explicit generator-result seam used to capture the
        // original stage main; full native generation is tested independently.
        let result = native::stage_with(output, || {
            if case["generation_error"].as_bool().unwrap() {
                Err(native::Error::new(
                    "FileNotFoundError",
                    Some(2),
                    "owned missing input",
                ))
            } else {
                Ok(generated)
            }
        });
        match result {
            Ok(report) => assert_eq!(
                report.json_line(),
                case["stdout"].as_str().unwrap(),
                "{}",
                case["name"]
            ),
            Err(actual) => assert_eq!(
                json!({"type":actual.kind(),"errno":actual.raw_os_error()}),
                case["error"],
                "{}",
                case["name"]
            ),
        }
        let actual = tree(output);
        for key in ["files", "links", "directories"] {
            assert_eq!(actual[key], case[key], "{} {key}", case["name"]);
        }
    }
}

#[test]
fn independent_original_procedural_helper_parameters() {
    let cases = fixture("helpers-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 15);
    for case in cases.as_array().unwrap() {
        let address = case["address"].as_i64().unwrap();
        let actual = match case["kind"].as_str().unwrap() {
            "tesla-point" => native::tesla_point(address, case["name"].as_str().unwrap()),
            "tesla-value" => native::tesla_values(address),
            "corner-180" => native::corner_object(180, address).unwrap(),
            "corner-235" => native::corner_object(235, address).unwrap(),
            "corner-430" => native::corner_object(430, address).unwrap(),
            other => panic!("unknown captured helper {other}"),
        };
        assert_eq!(
            actual,
            case["output"].as_str().unwrap(),
            "{} address {address}",
            case["kind"]
        );
    }
}

#[test]
fn original_chrysler_dynamic_input_and_assertion_partial_output() {
    let common_path =
        root().join("opendbc_repo/opendbc/dbc/generator/chrysler/_stellantis_common.dbc");
    let common = fs::read(common_path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&common)),
        "96573c311bfd988de02a493de644bae473715d190e5f2951b75bc76aaf7423ab"
    );
    let cases = fixture("chrysler-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 4);
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let mut input = if case["input_mode"] == "common-plus" {
            common.clone()
        } else {
            Vec::new()
        };
        input.extend(bytes(case["input_hex"].as_str().unwrap()));
        fs::write(temp.path().join("_stellantis_common.dbc"), input).unwrap();
        let result = native::render_program(Program::ChryslerRam, temp.path());
        assert_eq!(
            result.err().map(|e| e.kind().to_owned()),
            case["error"].as_str().map(str::to_owned),
            "{}",
            case["name"]
        );
        let mut observed = BTreeMap::new();
        for entry in fs::read_dir(temp.path()).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            if !name.ends_with("_generated.dbc") {
                continue;
            }
            let data = fs::read(&path).unwrap();
            let expected = &case["outputs"][name];
            let mut identity =
                json!({"sha256":format!("{:x}", Sha256::digest(&data)),"bytes":data.len()});
            if expected.get("hex").is_some() {
                identity["hex"] = json!(hex(&data));
            }
            observed.insert(name.to_owned(), identity);
        }
        assert_eq!(
            serde_json::to_value(observed).unwrap(),
            case["outputs"],
            "{}",
            case["name"]
        );
    }
}

fn source_snapshot(source: &Path) -> BTreeMap<String, String> {
    fn visit(base: &Path, path: &Path, result: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                visit(base, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(base)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(source, source, &mut result);
    result
}

#[test]
fn full_original_45_names_sizes_hashes_order_and_source_nonmutation() {
    let source = root().join("opendbc_repo/opendbc/dbc/generator");
    let before = source_snapshot(&source);
    let generated = native::generate_all(&source).unwrap();
    assert_eq!(source_snapshot(&source), before);
    assert_eq!(generated.len(), 45);
    assert!(generated.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert_eq!(
        generated.iter().map(|(_, s)| s.len()).sum::<usize>(),
        992_535
    );
    let identities: BTreeMap<_,_> = generated.iter().map(|(name,text)|(name.clone(),json!({"bytes":text.len(),"sha256":format!("{:x}",Sha256::digest(text.as_bytes()))}))).collect();
    assert_eq!(
        serde_json::to_value(identities).unwrap(),
        fixture("generate-all-original.json")["outputs"]
    );
}

#[test]
fn immutable_independent_capture_and_retired_source_provenance() {
    let manifest = fixture("generate-all-original.json");
    let pins = manifest["source_sha256"].as_object().unwrap();
    assert_eq!(pins.len(), 73);
    let retired: Vec<_> = pins
        .keys()
        .filter(|p| p.ends_with(".py"))
        .cloned()
        .collect();
    assert_eq!(retired.len(), 14); // Thirteen generator files plus the stager.
    for (name, expected) in pins {
        let path = root().join(name);
        match fs::symlink_metadata(&path) {
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && retired.contains(name) => {}
            other => {
                other.unwrap();
                assert_eq!(
                    format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
                    expected.as_str().unwrap(),
                    "source {name}"
                );
            }
        }
    }
    // These bind the independently executed originals, not a current native
    // recapture. DBC inputs remain dynamic at runtime; baseline drift here
    // requires a newly reviewed independent original capture.
    for group in [
        "policy",
        "procedural",
        "paths_cli",
        "glob",
        "literal_correction",
        "errors",
    ] {
        for (name, expected) in fixture("provenance.json")[group]["fixture_sha256"]
            .as_object()
            .unwrap()
        {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/dbc-generator")
                .join(name);
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
                expected.as_str().unwrap(),
                "capture {name}"
            );
        }
    }
}

// APFS cannot create invalid UTF-8 filenames; retain the raw-byte corpus on Linux.
// https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html
#[cfg(target_os = "linux")]
#[test]
fn original_raw_os_paths_and_filename_encoding_error_preserve_partial_bytes() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let cases = fixture("paths-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 2);
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join(std::ffi::OsString::from_vec(bytes(
            case["source_hex"].as_str().unwrap(),
        )));
        let output = temp.path().join(std::ffi::OsString::from_vec(bytes(
            case["output_hex"].as_str().unwrap(),
        )));
        let filename = std::ffi::OsString::from_vec(bytes(case["filename_hex"].as_str().unwrap()));
        fs::create_dir(&source).unwrap();
        fs::create_dir(&output).unwrap();
        fs::write(
            source.join(&filename),
            bytes(case["input_hex"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(
            error(native::create_dbc(&source, Path::new(&filename), &output)),
            case["error"],
            "{}",
            case["name"]
        );
        let observed: BTreeMap<_, _> = fs::read_dir(&output)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    hex(e.file_name().as_bytes()),
                    hex(&fs::read(e.path()).unwrap()),
                )
            })
            .collect();
        assert_eq!(
            serde_json::to_value(observed).unwrap(),
            case["outputs"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn explicit_native_admission_rejects_custom_changed_and_dangling_python_after_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let output = temp.path().join("output");
    fs::create_dir_all(source.join("tesla")).unwrap();
    fs::create_dir(&output).unwrap();
    let bad = source.join("tesla/custom.py");
    fs::write(&bad, b"raise RuntimeError('must never execute Python')\n").unwrap();
    fs::write(output.join("old_generated.dbc"), b"STALE").unwrap();
    let error = native::create_all(&output, &source, &[]).unwrap_err();
    assert_eq!(error.kind(), "UnsupportedProgram");
    assert!(
        !output.join("old_generated.dbc").exists(),
        "admission belongs to the procedural phase after old-output removal"
    );
    fs::remove_file(&bad).unwrap();
    let known = source.join("tesla/_radar_common.py");
    fs::write(
        &known,
        b"# changed original must not silently become native policy\n",
    )
    .unwrap();
    assert_eq!(
        native::create_all(&output, &source, &[])
            .unwrap_err()
            .kind(),
        "UnsupportedProgram"
    );
    fs::remove_file(&known).unwrap();
    std::os::unix::fs::symlink("absent", &known).unwrap();
    assert!(
        native::create_all(&output, &source, &[]).is_err(),
        "dangling symlink is present, not retired absence"
    );
    fs::remove_file(known).unwrap();
    native::create_all(&output, &source, &[]).unwrap();
    assert_eq!(native::program_outputs().len(), 10);
    assert!(native::program_outputs()
        .iter()
        .all(|name| !name.starts_with('_') && name.ends_with("_generated.dbc")));
}

#[test]
fn inherited_glob_bug_is_corrected_with_literal_owned_paths_and_no_sibling_cleanup() {
    let cases = fixture("glob-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 4);
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join(case["source"].as_str().unwrap());
        let output = temp.path().join(case["output"].as_str().unwrap());
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&output).unwrap();
        for (name, data) in case["initial"].as_object().unwrap() {
            let path = temp.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes(data.as_str().unwrap())).unwrap();
        }
        if case["name"] == "source-bracket-skips-writer" {
            let brand = source.join("chrysler");
            fs::write(
                brand.join("_stellantis_common.dbc"),
                fs::read(
                    root()
                        .join("opendbc_repo/opendbc/dbc/generator/chrysler/_stellantis_common.dbc"),
                )
                .unwrap(),
            )
            .unwrap();
            // Deliberate #375 correction: select the literal source, unlike the
            // unchanged create_all error retained in glob-original.json. Exact
            // contents come from original direct script/create_dbc operations.
            native::create_all(&output, &source, &[Program::ChryslerRam]).unwrap();
            let oracle = fixture("literal-correction-original.json");
            for (name, identity) in oracle["outputs"].as_object().unwrap() {
                let data = fs::read(temp.path().join(name)).unwrap();
                assert_eq!(
                    json!({"bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(&data))}),
                    *identity,
                    "{name}"
                );
            }
        } else {
            native::create_all(&output, &source, &[]).unwrap();
            assert_eq!(
                hex(&fs::read(output.join("car_generated.dbc")).unwrap()),
                case["files"][format!("{}/car_generated.dbc", case["output"].as_str().unwrap())]
                    .as_str()
                    .unwrap()
            );
            if case["other"].as_bool().unwrap() {
                assert_eq!(
                    fs::read(temp.path().join("out1/stale_generated.dbc")).unwrap(),
                    b"SIBLING"
                );
            }
        }
        assert!(
            !output.join("stale_generated.dbc").exists(),
            "only the literal owned output directory is cleaned"
        );
    }
    // '*' used to expand to unrelated sibling directories too. No current
    // native selection may widen ownership based on filename metacharacters.
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source*");
    let output = temp.path().join("out*");
    let sibling = temp.path().join("out1");
    fs::create_dir_all(source.join("brand")).unwrap();
    fs::create_dir(&output).unwrap();
    fs::create_dir(&sibling).unwrap();
    fs::write(source.join("brand/car.dbc"), b"CORE").unwrap();
    fs::write(output.join("stale_generated.dbc"), b"OLD").unwrap();
    fs::write(sibling.join("stale_generated.dbc"), b"SIBLING").unwrap();
    native::create_all(&output, &source, &[]).unwrap();
    assert!(!output.join("stale_generated.dbc").exists());
    assert_eq!(
        fs::read(sibling.join("stale_generated.dbc")).unwrap(),
        b"SIBLING"
    );
    assert_eq!(
        hex(&fs::read(output.join("car_generated.dbc")).unwrap()),
        cases[0]["files"]["out[1]/car_generated.dbc"]
            .as_str()
            .unwrap()
    );
}

#[test]
fn original_chrysler_decimal_digit_limit_and_partial_output() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dbc-generator/chrysler-digit-limit-original.json");
    let captured = fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&captured)),
        "5dcd8fe2f3d197cddb52c0396b073b5700c406d09dc2228134ec8a8ff0fb5732"
    );
    let fixture: Value = serde_json::from_slice(&captured).unwrap();
    let common_name = "opendbc_repo/opendbc/dbc/generator/chrysler/_stellantis_common.dbc";
    let common = fs::read(root().join(common_name)).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&common)),
        fixture["source_sha256"][common_name].as_str().unwrap()
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let count = usize::try_from(case["digits"].as_u64().unwrap()).unwrap();
        let token = case["sign"].as_str().unwrap().to_owned()
            + &vec![case["digit"].as_str().unwrap(); count]
                .join(case["separator"].as_str().unwrap());
        let mut input = common.clone();
        input.extend_from_slice(format!("\nBO_ {token} AddedControl: 8 XXX\n").as_bytes());
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("_stellantis_common.dbc"), input).unwrap();
        let actual = native::render_program(Program::ChryslerRam, temp.path());
        assert_eq!(
            actual.err().map(|e| e.kind().to_owned()),
            case["error"].as_str().map(str::to_owned),
            "{}",
            case["name"]
        );
        let observed: BTreeMap<_, _> = fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .as_encoded_bytes()
                    .ends_with(b"_generated.dbc")
            })
            .map(|path| {
                let bytes = fs::read(&path).unwrap();
                (
                    path.file_name().unwrap().to_str().unwrap().to_owned(),
                    json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes))}),
                )
            })
            .collect();
        assert_eq!(
            serde_json::to_value(observed).unwrap(),
            case["outputs"],
            "{}",
            case["name"]
        );
    }
}
