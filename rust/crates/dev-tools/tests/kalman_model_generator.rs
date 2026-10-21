#![cfg(feature = "kalman-model-generator-native")]

use openpilot_dev_tools::kalman_model_generator::{
    evaluate_all, function_names, generate, output_argument, Model,
};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::Duration,
};

fn admit_original_root(root: &Path) -> BTreeMap<String, Vec<u8>> {
    use sha2::{Digest, Sha256};
    let directory = fs::symlink_metadata(root).unwrap();
    assert!(directory.is_dir() && !directory.file_type().is_symlink());
    let read = |path: &Path| {
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert!(metadata.len() <= 1024 * 1024);
        fs::read(path).unwrap()
    };
    let body = read(&root.join("manifest.json"));
    assert_eq!(
        format!("{:x}", Sha256::digest(&body)),
        "005f7421cd11cbc2de77675b65d5ad1d1d8644f030c09e48c07e642548bdbc9b"
    );
    let manifest: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        manifest["source_commit"],
        "f913de3aa3c0f2c261f4ae4b87e41f3eb317fb82"
    );
    assert_eq!(manifest["native_port_generated_expected"], false);
    let files = manifest["files"].as_object().unwrap();
    assert_eq!(files.len(), 30);
    let mut snapshot = BTreeMap::from([("manifest.json".to_owned(), body)]);
    for (name, seal) in files {
        assert!(Path::new(name)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_))));
        let bytes = read(&root.join(name));
        assert_eq!(seal["bytes"], bytes.len());
        assert_eq!(
            seal["sha256"],
            format!("{:x}", Sha256::digest(&bytes)),
            "{name}"
        );
        snapshot.insert(name.clone(), bytes);
    }
    snapshot
}

#[test]
fn committed_original_generator_data_retains_every_fixed_byte_seal() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kalman-generators-original");
    assert_eq!(admit_original_root(&root).len(), 31);
}

#[test]
fn live_model_definition_changes_require_explicit_native_equation_review() {
    use sha2::{Digest, Sha256};
    let package = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = package.ancestors().nth(3).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(package.join("tests/fixtures/kalman-generators-original/manifest.json")).unwrap(),
    )
    .unwrap();
    for name in [
        "openpilot/selfdrive/locationd/models/pose_kf.py",
        "openpilot/selfdrive/locationd/models/car_kf.py",
        "openpilot/selfdrive/locationd/models/constants.py",
        "openpilot/common/constants.py",
        "rednose_repo/rednose/helpers/sympy_helpers.py",
    ] {
        let bytes = fs::read(root.join(name)).unwrap();
        assert_eq!(manifest["source_closure"][name]["bytes"], bytes.len());
        assert_eq!(
            manifest["source_closure"][name]["sha256"],
            format!("{:x}", Sha256::digest(&bytes)),
            "live original equations changed: {name}"
        );
    }
}

#[derive(Deserialize)]
struct Sample {
    state: Vec<f64>,
    globals: Option<Vec<f64>>,
    dt: f64,
    functions: BTreeMap<String, Vec<f64>>,
}

fn corpus(model: Model) -> Vec<Sample> {
    let source = match model {
        Model::Pose => include_bytes!("../../locationd/tests/data/model.json").as_slice(),
        Model::Car => include_bytes!("../../paramsd/tests/data/model.json").as_slice(),
    };
    serde_json::from_slice(source).unwrap()
}

#[test]
fn symbolic_equations_and_row_major_jacobians_match_independent_source_corpora() {
    for (model, count, width) in [(Model::Pose, 16, 18), (Model::Car, 24, 9)] {
        let samples = corpus(model);
        assert_eq!(samples.len(), count);
        for sample in samples {
            assert_eq!(sample.state.len(), width);
            let globals = sample.globals.as_deref().unwrap_or(&[]);
            let actual = evaluate_all(model, &sample.state, globals, sample.dt).unwrap();
            assert_eq!(actual.len(), function_names(model).len());
            assert_eq!(sample.functions.len(), actual.len());
            for (generated_name, values) in actual {
                let original_name = match generated_name {
                    "transition" => "f_fun".to_owned(),
                    "transition_jacobian" => "F_fun".to_owned(),
                    name if name.starts_with("observe_") => name.replacen("observe_", "h_", 1),
                    name if name.starts_with("jacobian_") => name.replacen("jacobian_", "H_", 1),
                    _ => panic!("unexpected generated function {generated_name}"),
                };
                let expected = &sample.functions[&original_name];
                assert_eq!(values.len(), expected.len(), "{original_name} width");
                for (index, (value, oracle)) in values.iter().zip(expected).enumerate() {
                    assert!(
                        value.is_finite() && oracle.is_finite(),
                        "{original_name}[{index}] nonfinite: {value} / {oracle}"
                    );
                    assert!(
                        (value - oracle).abs() <= 1e-9 + 1e-9 * oracle.abs(),
                        "{original_name}[{index}]: {value} != {oracle}"
                    );
                }
            }
        }
    }
}

#[test]
fn native_regeneration_is_deterministic_and_emits_complete_original_abi() {
    let directory = tempfile::tempdir().unwrap();
    for (model, state_width, observations) in [
        (Model::Pose, 18, [4, 10, 13, 14].as_slice()),
        (Model::Car, 9, [24, 25, 26, 27, 28, 29, 30, 31].as_slice()),
    ] {
        let first = directory.path().join(format!("{state_width}-first"));
        let second = directory.path().join(format!("{state_width}-second"));
        generate(model, &first).unwrap();
        generate(model, &second).unwrap();
        let mut names = function_names(model)
            .iter()
            .map(|name| format!("{name}.rs"))
            .collect::<Vec<_>>();
        names.push("mod.rs".to_owned());
        names.sort();
        let mut listed = fs::read_dir(&first)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        listed.sort();
        assert_eq!(listed, names);
        for name in names {
            let bytes = fs::read(first.join(&name)).unwrap();
            assert_eq!(bytes, fs::read(second.join(&name)).unwrap(), "{name}");
            assert!(!bytes.is_empty(), "{name}");
        }
        let modules = fs::read_to_string(first.join("mod.rs")).unwrap();
        assert!(modules.contains(&format!("pub const INITIAL_X: [f64; {state_width}]")));
        for kind in observations {
            assert!(modules.contains(&format!("mod observe_{kind};")));
            assert!(modules.contains(&format!("mod jacobian_{kind};")));
        }
    }
}

#[test]
fn source_equations_are_sensitive_to_state_and_car_globals() {
    let pose = corpus(Model::Pose).remove(1);
    let original = evaluate_all(Model::Pose, &pose.state, &[], pose.dt).unwrap();
    let mut changed = pose.state;
    changed[12] += 0.25;
    let modified = evaluate_all(Model::Pose, &changed, &[], pose.dt).unwrap();
    assert_ne!(original["observe_10"], modified["observe_10"]);

    let car = corpus(Model::Car).remove(2);
    let mut globals = car.globals.unwrap();
    let original = evaluate_all(Model::Car, &car.state, &globals, car.dt).unwrap();
    globals[4] *= 1.5;
    let modified = evaluate_all(Model::Car, &car.state, &globals, car.dt).unwrap();
    assert_ne!(original["transition"], modified["transition"]);
    assert_eq!(original["observe_24"], modified["observe_24"]);
}

#[test]
fn pose_asin_jacobian_tracks_a_nonsingular_pitch_perturbation() {
    let mut state = [0.0; 18];
    state[0] = 0.2;
    state[1] = 1.2;
    state[2] = -0.4;
    state[6] = 0.04;
    state[7] = -0.03;
    state[8] = 0.02;
    let dt = 0.05;
    let derivative = evaluate_all(Model::Pose, &state, &[], dt).unwrap()["transition_jacobian"][19];
    let step = 1e-6;
    state[1] += step;
    let high = evaluate_all(Model::Pose, &state, &[], dt).unwrap()["transition"][1];
    state[1] -= 2.0 * step;
    let low = evaluate_all(Model::Pose, &state, &[], dt).unwrap()["transition"][1];
    let difference = (high - low) / (2.0 * step);
    assert!(derivative.is_finite() && difference.is_finite());
    assert!(
        (derivative - difference).abs() < 1e-6,
        "{derivative} != {difference}"
    );
}

#[test]
fn car_zero_speed_jacobian_retains_frozen_original_infinity() {
    let sample = corpus(Model::Car).remove(1);
    let mut state = sample.state;
    let globals = sample.globals.unwrap();
    state[4] = 0.0;
    let [mass, _, center_to_front, center_to_rear, stiffness_front, stiffness_rear] =
        <[f64; 6]>::try_from(globals.as_slice()).unwrap();
    // F_fun[45] from the independently frozen original generated module.
    let original = sample.dt
        * (stiffness_front * (-state[2] - state[3] + state[7]) / (mass * state[1])
            + (-stiffness_front - stiffness_rear) * state[5] / (mass * state[4])
            + (-center_to_front * stiffness_front + center_to_rear * stiffness_rear) * state[6]
                / (mass * state[4]));
    assert!(original.is_infinite() && original.is_sign_negative());
    let actual =
        evaluate_all(Model::Car, &state, &globals, sample.dt).unwrap()["transition_jacobian"][45];
    assert!(
        actual.is_infinite() && actual.is_sign_negative(),
        "frozen original {original}, native {actual}"
    );
}

#[test]
fn output_option_preserves_raw_path_and_last_value() {
    let raw_path = OsString::from_vec(b"/tmp/model-\xff".to_vec());
    let selected = output_argument(
        [
            OsString::from("--o=old"),
            OsString::from("--output"),
            raw_path.clone(),
        ],
        "default",
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.as_os_str(), raw_path.as_os_str());
    assert_eq!(
        output_argument(
            [OsString::from("--output"), OsString::from("-1")],
            "default"
        )
        .unwrap()
        .unwrap(),
        std::path::PathBuf::from("-1")
    );
    assert_eq!(
        output_argument([OsString::from("--help")], "default").unwrap(),
        None
    );
    assert!(output_argument([OsString::from("--output")], "default").is_err());
    assert!(output_argument(
        [OsString::from("--output"), OsString::from("--help")],
        "default"
    )
    .is_err());
    assert!(output_argument([OsString::from("--bogus")], "default").is_err());
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}

fn run_recorded(label: &str, evidence: &Path, mut spec: CommandSpec) {
    spec.stdout_limit = 2 * 1024 * 1024;
    spec.stderr_limit = 2 * 1024 * 1024;
    spec.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    let argv = spec
        .effective_argv()
        .iter()
        .map(|arg| arg.as_os_str().as_bytes().to_vec())
        .collect::<Vec<_>>();
    fs::write(
        evidence.join(format!("{label}.argv.json")),
        serde_json::to_vec_pretty(&argv).unwrap(),
    )
    .unwrap();
    let stdout = evidence.join(format!("{label}.stdout"));
    let stderr = evidence.join(format!("{label}.stderr"));
    let result = NativeRunner.run_to_files(&spec, &stdout, &stderr);
    match result {
        Ok(captured) => {
            fs::write(
                evidence.join(format!("{label}.status")),
                format!("{}\n", captured.code),
            )
            .unwrap();
            assert_eq!(
                captured.code,
                0,
                "{label} failed; see {}",
                evidence.display()
            );
        }
        Err(error) => {
            fs::write(
                evidence.join(format!("{label}.status")),
                format!("runner error: {error}\n"),
            )
            .unwrap();
            panic!("{label} failed; see {}: {error}", evidence.display());
        }
    }
}

fn complete_roster(directory: &Path, model: Model) -> BTreeMap<String, Vec<u8>> {
    let mut expected = function_names(model)
        .iter()
        .map(|name| format!("{name}.rs"))
        .collect::<Vec<_>>();
    expected.push("mod.rs".to_owned());
    expected.sort();
    let mut found = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    found.sort();
    assert_eq!(found, expected, "{} roster", directory.display());
    expected
        .into_iter()
        .map(|name| {
            let bytes = fs::read(directory.join(&name)).unwrap();
            assert!(!bytes.is_empty(), "{} {name} empty", directory.display());
            (name, bytes)
        })
        .collect()
}

fn copy_original_roster(
    source: &Path,
    destination: &Path,
    model: Model,
) -> BTreeMap<String, Vec<u8>> {
    let snapshot = complete_roster(source, model);
    fs::create_dir(destination).unwrap();
    for (name, bytes) in &snapshot {
        fs::write(destination.join(name), bytes).unwrap();
    }
    snapshot
}

fn bits(value: f64) -> String {
    format!("f64::from_bits(0x{:016x})", value.to_bits())
}

fn array(values: &[f64]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| bits(*value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn boundary_inputs(model: Model) -> Vec<(String, Vec<f64>, Vec<f64>, f64)> {
    let mut inputs = Vec::new();
    match model {
        Model::Pose => {
            inputs.push(("zero-dt".into(), vec![0.0; 18], vec![], 0.0));
            for pitch in [
                std::f64::consts::FRAC_PI_2 - 1e-6,
                -std::f64::consts::FRAC_PI_2 + 1e-6,
            ] {
                let mut state = vec![0.0; 18];
                state[0] = 0.2;
                state[1] = pitch;
                state[2] = -0.4;
                state[6..9].copy_from_slice(&[0.1, -0.2, 0.3]);
                inputs.push((format!("pitch-{pitch}"), state, vec![], 0.05));
            }
            let mut state = vec![0.0; 18];
            state[0] = f64::NAN;
            state[12] = f64::INFINITY;
            inputs.push(("nonfinite-state".into(), state, vec![], 0.05));
        }
        Model::Car => {
            let first = corpus(Model::Car).remove(1);
            let base = first.state;
            let globals = first.globals.unwrap();
            for (name, state_index, global_index, value) in [
                ("zero-speed", Some(4), None, 0.0),
                ("negative-speed", Some(4), None, -5.0),
                ("zero-steer-ratio", Some(1), None, 0.0),
                ("zero-mass", None, Some(0), 0.0),
                ("zero-inertia", None, Some(1), 0.0),
                ("nan-lateral", Some(5), None, f64::NAN),
                ("infinite-mass", None, Some(0), f64::INFINITY),
            ] {
                let mut state = base.clone();
                let mut parameters = globals.clone();
                if let Some(index) = state_index {
                    state[index] = value;
                }
                if let Some(index) = global_index {
                    parameters[index] = value;
                }
                inputs.push((name.into(), state, parameters, first.dt));
            }
        }
    }
    inputs
}

fn harness_source(model: Model) -> String {
    let mut text = String::from(
        r#"#[path = "original/mod.rs"] mod original;
#[path = "native-a/mod.rs"] mod native;
fn compare(label: &str, source: &[f64], candidate: &[f64]) {
    assert_eq!(source.len(), candidate.len(), "{label} width");
    for (index, (&a, &b)) in source.iter().zip(candidate).enumerate() {
        if a.is_nan() || b.is_nan() {
            assert!(a.is_nan() && b.is_nan(), "{label}[{index}]: {a} / {b}");
        } else if a.is_infinite() || b.is_infinite() {
            assert!(a.is_infinite() && b.is_infinite() && a.is_sign_positive() == b.is_sign_positive(),
                "{label}[{index}]: {a} / {b}");
        } else {
            assert!((a-b).abs() <= 1e-9 + 1e-9*a.abs(), "{label}[{index}]: {a} / {b}");
        }
    }
}
fn bits_equal(label: &str, source: &[f64], candidate: &[f64]) {
    assert_eq!(source.len(), candidate.len(), "{label} width");
    for (index, (&a, &b)) in source.iter().zip(candidate).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "{label}[{index}]");
    }
}
fn main() {
"#,
    );
    for name in ["INITIAL_X", "INITIAL_P", "PROCESS_NOISE"] {
        text.push_str(&format!(
            "bits_equal(\"{name}\", &original::{name}, &native::{name});\n"
        ));
    }
    match model {
        Model::Pose => {
            for kind in [4, 10, 13, 14] {
                let name = format!("NOISE_{kind}");
                text.push_str(&format!(
                    "bits_equal(\"{name}\", &original::{name}, &native::{name});\n"
                ));
            }
        }
        Model::Car => {
            for kind in [26, 27, 31, 29, 28, 30] {
                let name = format!("NOISE_{kind}");
                text.push_str(&format!(
                    "assert_eq!(original::{name}.to_bits(), native::{name}.to_bits(), \"{name}\");\n"
                ));
            }
        }
    }
    let mut inputs = corpus(model)
        .into_iter()
        .enumerate()
        .map(|(index, sample)| {
            (
                format!("corpus-{index}"),
                sample.state,
                sample.globals.unwrap_or_default(),
                sample.dt,
            )
        })
        .collect::<Vec<_>>();
    inputs.extend(boundary_inputs(model));
    for (label, state, globals, dt) in inputs {
        text.push_str(&format!(
            "let state = {}; let dt = {};\n",
            array(&state),
            bits(dt)
        ));
        if model == Model::Car {
            text.push_str(&format!("let globals = {};\n", array(&globals)));
        }
        for name in function_names(model) {
            let argument = if name.starts_with("transition") {
                match model {
                    Model::Pose => "&state, dt",
                    Model::Car => "&state, &globals, dt",
                }
            } else {
                "&state"
            };
            text.push_str(&format!(
                "compare(\"{label}/{name}\", &original::{name}({argument}), &native::{name}({argument}));\n"
            ));
        }
    }
    text.push_str("}\n");
    text
}

#[test]
#[ignore = "hosted fresh original generator capture and rustc acceptance only"]
fn fresh_native_files_compile_and_match_original_functions_and_constant_bits() {
    let original_root = required_path("KALMAN_ORIGINAL_ROOT");
    let original_snapshot = admit_original_root(&original_root);
    let retired_root = required_path("KALMAN_RETIRED_ROOT");
    let assert_absent = |root: &Path| {
        for name in ["generate_pose_model.py", "generate_car_model.py"] {
            let result = fs::symlink_metadata(root.join("rust/tools").join(name));
            assert!(
                result.is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "retired Python generator present: {name}"
            );
        }
    };
    assert_absent(&retired_root);
    let evidence_root = required_path("KALMAN_GENERATOR_EVIDENCE");
    let rustc = required_path("KALMAN_RUSTC");
    fs::create_dir_all(&evidence_root).unwrap();
    let evidence = evidence_root.join("native-model-pair");
    fs::create_dir(&evidence).unwrap();
    for (model, name, binary_name) in [
        (Model::Pose, "pose", "KALMAN_POSE_BINARY"),
        (Model::Car, "car", "KALMAN_CAR_BINARY"),
    ] {
        let original = original_root.join(format!("{name}-original"));
        let directory = evidence.join(name);
        fs::create_dir(&directory).unwrap();
        let snapshot = copy_original_roster(&original, &directory.join("original"), model);
        let binary = required_path(binary_name);
        let private = directory.join("source-absent-working-directory");
        fs::create_dir(&private).unwrap();
        assert_absent(&private);
        for pass in ["native-a", "native-b"] {
            let output = directory.join(pass);
            let mut spec = CommandSpec::new_os(
                vec![
                    binary.as_os_str().to_owned(),
                    "--output".into(),
                    output.into_os_string(),
                ],
                Duration::from_secs(300),
            );
            spec.cwd = Some(if pass == "native-a" {
                retired_root.clone()
            } else {
                private.clone()
            });
            spec.env.insert("PATH".into(), "/nonexistent".into());
            run_recorded(pass, &directory, spec);
        }
        let first = complete_roster(&directory.join("native-a"), model);
        let second = complete_roster(&directory.join("native-b"), model);
        assert_eq!(first, second, "{name} nondeterministic native emission");
        assert_eq!(
            complete_roster(&original, model),
            snapshot,
            "{name} original changed"
        );
        let source = directory.join("harness.rs");
        fs::write(&source, harness_source(model)).unwrap();
        let executable = directory.join("comparison");
        let rustc_spec = CommandSpec::new_os(
            vec![
                rustc.as_os_str().to_owned(),
                "--edition=2021".into(),
                "--crate-name".into(),
                format!("kalman_{name}_pair").into(),
                source.into_os_string(),
                "-o".into(),
                executable.as_os_str().to_owned(),
            ],
            Duration::from_secs(600),
        );
        run_recorded("rustc-pair", &directory, rustc_spec);
        let mut comparison =
            CommandSpec::new_os(vec![executable.into_os_string()], Duration::from_secs(120));
        comparison.env.insert("PATH".into(), "/nonexistent".into());
        run_recorded("compare-all", &directory, comparison);
        assert_eq!(
            complete_roster(&original, model),
            snapshot,
            "{name} original changed during compilation/comparison"
        );
        assert_eq!(
            complete_roster(&directory.join("native-a"), model),
            first,
            "{name} native source changed during compilation/comparison"
        );
    }
    assert_eq!(admit_original_root(&original_root), original_snapshot);
    assert_absent(&retired_root);
}
