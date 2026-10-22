//! Private, failure-only integration diagnostic; never a retirement acceptance gate.
use super::support::{bytes_sha, pin, read_json, required, run, write_json, Held};
use openpilot_dev_tools::driver_daemon_check::OriginalCapture;
use serde_json::{json, Value};
use std::{collections::BTreeMap, error::Error, ffi::OsString, fs, io::Cursor, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const SOURCE: &str = "2ecf3f6813f601ea52e7a61ccf75219952daefa0";
const MANIFEST: &str = "532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795";

fn hold_tree(path: &Path, held: &mut Vec<Held>) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err("diagnostic catalog contains a symlink".into());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            hold_tree(&entry?.path(), held)?;
        }
    } else if metadata.is_file() {
        held.push(Held::open(path));
    } else {
        return Err("diagnostic input is not a regular file".into());
    }
    Ok(())
}

fn raw_predictions(packet: &[u8]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(packet);
    let message =
        capnp::serialize::read_message(&mut cursor, capnp::message::ReaderOptions::new())?;
    if cursor.position() != packet.len() as u64 {
        return Err("driver packet contains trailing bytes".into());
    }
    let event = message.get_root::<openpilot_cereal::log_capnp::event::Reader>()?;
    let openpilot_cereal::log_capnp::event::Which::DriverStateV2(state) = event.which()? else {
        return Err("diagnostic packet is not driverStateV2".into());
    };
    let bytes = state?.get_raw_predictions()?.to_vec();
    if bytes.len() != 2212 {
        return Err("diagnostic requires the complete 2212-byte raw prediction".into());
    }
    Ok(bytes)
}

fn comparison(left: &[u8], right: &[u8]) -> Value {
    let mut maximum = 0.0_f64;
    let mut nonfinite_pairs = 0;
    let mut finite_nonzero_differences = 0;
    let mut signed_zero_differences = 0;
    let mut nonfinite_differences = 0;
    let mut first_differences = Vec::new();
    for (index, (left, right)) in left.chunks_exact(4).zip(right.chunks_exact(4)).enumerate() {
        let a = f32::from_le_bytes(left.try_into().unwrap());
        let b = f32::from_le_bytes(right.try_into().unwrap());
        if a.is_finite() && b.is_finite() {
            maximum = maximum.max((f64::from(a) - f64::from(b)).abs());
        } else {
            nonfinite_pairs += 1;
        }
        if a.to_bits() != b.to_bits() {
            let kind = if !a.is_finite() || !b.is_finite() {
                nonfinite_differences += 1;
                "nonfinite"
            } else if a == 0.0 && b == 0.0 {
                signed_zero_differences += 1;
                "signed_zero"
            } else {
                finite_nonzero_differences += 1;
                "finite_nonzero"
            };
            // Raw bit strings preserve NaN payloads and signed zeros in JSON.
            // Bound samples, but count every differing f32 in the prediction.
            if first_differences.len() < 8 {
                first_differences.push(json!({"index":index,
                    "left_bits":format!("{:08x}",a.to_bits()),
                    "right_bits":format!("{:08x}",b.to_bits()),"kind":kind}));
            }
        }
    }
    let differing_f32_count =
        finite_nonzero_differences + signed_zero_differences + nonfinite_differences;
    json!({"left_bytes":left.len(),"right_bytes":right.len(),
        "left_sha256":bytes_sha(left),"right_sha256":bytes_sha(right),
        "byte_equal":left == right,"max_finite_f32_absolute_difference":maximum,
        "nonfinite_pairs":nonfinite_pairs,"float_count":left.len()/4,
        "differing_f32_count":differing_f32_count,
        "finite_nonzero_differences":finite_nonzero_differences,
        "signed_zero_differences":signed_zero_differences,
        "nonfinite_differences":nonfinite_differences,
        "difference_samples_truncated":differing_f32_count > first_differences.len(),
        "first_differences":first_differences})
}

pub fn run_first_driver_frame_diagnostic() -> Result<()> {
    let preparation = required("MODEL_FAMILY_GATE_PREPARATION");
    run_driver_frame_diagnostic_from(
        &preparation.join("grouped-native/root-0/driver-collector-ordinary-output/1344-raw-1"),
        &preparation.join("driver-first-frame-diagnostic"),
    )
}

pub fn run_driver_frame_diagnostic_from(
    failed: &std::path::Path,
    output: &std::path::Path,
) -> Result<()> {
    let root = required("MODEL_FAMILY_GATE_ROOT");
    let preparation = required("MODEL_FAMILY_GATE_PREPARATION");
    let original_family = required("MODEL_FAMILY_GATE_ORIGINAL");
    if !original_family.is_dir() {
        return Err("original family fixture directory missing".into());
    }
    fs::create_dir(output)?;
    let original = OriginalCapture::open(
        &preparation.join("driver-fixture/collector-current/observations"),
        MANIFEST,
        SOURCE,
    )?;
    let catalog = preparation.join("driver-catalog/catalog");
    let receipt = preparation.join("driver-catalog/current-catalog-receipt.json");
    let target = preparation.join("driver-catalog/compiler-target.json");
    let executable = root.join("rust/target/debug/model-run");
    let mut held = vec![
        Held::open(&executable),
        Held::open(&receipt),
        Held::open(&target),
    ];
    let catalog_start = held.len();
    hold_tree(&catalog, &mut held)?;
    let catalog_before = held[catalog_start..]
        .iter()
        .map(|input| {
            let evidence = input.evidence();
            (evidence["path"].as_str().unwrap().to_owned(), evidence)
        })
        .collect::<BTreeMap<_, _>>();
    let current = read_json(&catalog.join("current.json"));
    let generation = current["generation"]
        .as_str()
        .ok_or("catalog generation missing")?;
    if generation.len() != 64 || !generation.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("catalog generation is not a SHA256".into());
    }
    let bundle = catalog.join(generation).join("driver-1344x760");
    let mut sequence = Vec::new();
    let mut frames = Vec::new();
    for (index, (label, input, packet)) in [
        ("warmup", "warmup.bin", "warmup-message.bin"),
        ("frame-0", "frame-0.bin", "message-0.bin"),
    ]
    .into_iter()
    .enumerate()
    {
        let input_path = failed.join(input);
        let packet_path = failed.join(packet);
        held.push(Held::open(&input_path));
        held.push(Held::open(&packet_path));
        let bytes = fs::read(&input_path)?;
        let expected_input = original.read(&format!("1344-raw-1/{input}"))?;
        if bytes != expected_input {
            return Err(
                format!("failed daemon input differs from sealed original: {input}").into(),
            );
        }
        fs::write(output.join(input), &bytes)?;
        let expected_packet =
            original.read(&format!("1344-raw-1/reference/expected-{index}.bin"))?;
        let expected = raw_predictions(&expected_packet)?;
        let actual = raw_predictions(&fs::read(&packet_path)?)?;
        fs::write(output.join(format!("{label}-original.raw")), &expected)?;
        fs::write(output.join(format!("{label}-daemon.raw")), &actual)?;
        let transform: Vec<u8> = [0.75_f32, 0.0, 132.0, 0.0, 0.75, 113.0, 0.0, 0.0, 1.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        fs::write(output.join(format!("{label}-transform.bin")), transform)?;
        fs::write(output.join(format!("{label}-calib.bin")), [0_u8; 12])?;
        sequence.push(json!({"entrypoint":"prepare","inputs":{"frame":input,
            "transform":format!("{label}-transform.bin")},"outputs":{"warped":format!("{label}-warped.bin")}}));
        sequence.push(json!({"entrypoint":"model","inputs":{"calib":format!("{label}-calib.bin")},
            "outputs":{"model":format!("{label}-model.bin"),"warped":format!("{label}-model-warped.bin")}}));
        frames.push((label, expected, actual));
    }
    let sequence_path = output.join("sequence.json");
    write_json(&sequence_path, &json!(sequence));
    // Hold every actual input to model-run, including the authored sequence.
    hold_tree(output, &mut held)?;
    let mut args = vec![
        OsString::from("--trusted-bundle"),
        bundle.clone().into_os_string(),
        sequence_path.into_os_string(),
    ];
    let current_receipt = read_json(&receipt);
    if !current_receipt["cached_code"].is_null() {
        let admission = bundle.join("cached-admission.json");
        held.push(Held::open(&admission));
        args.extend([
            "--cached-admission".into(),
            admission.into_os_string(),
            "--cached-admission-sha256".into(),
            current_receipt["cached_code"]["admission_sha256"]
                .as_str()
                .ok_or("cached admission SHA missing")?
                .into(),
        ]);
    }
    let result = run(&executable, &args, &output.join("invocation"), None, true); // diagnostic's bound is 120 seconds
    let mut comparisons = Vec::new();
    if result.code == 0 {
        for (label, expected, daemon) in frames {
            let model = fs::read(output.join(format!("{label}-model.bin")))?;
            if model.len() != 2212 {
                return Err("model-run raw prediction width differs".into());
            }
            comparisons.push(json!({"frame":label,"model_vs_original":comparison(&model,&expected),
                "daemon_vs_original":comparison(&daemon,&expected),"model_vs_daemon":comparison(&model,&daemon),
                "classification":if model == expected && daemon == expected {"both_match_original"}
                    else if model == expected {"model_matches_original_daemon_differs"}
                    else if model == daemon {"model_and_daemon_agree_but_differ_from_original"}
                    else {"model_daemon_original_differ"},
                "warped":pin(&output.join(format!("{label}-warped.bin"))),
                "model_warped":pin(&output.join(format!("{label}-model-warped.bin")))}));
        }
    }
    original.verify()?;
    for input in &held {
        input.verify();
    }
    let mut catalog_after = Vec::new();
    hold_tree(&catalog, &mut catalog_after)?;
    let catalog_after = catalog_after
        .iter()
        .map(|input| {
            let evidence = input.evidence();
            (evidence["path"].as_str().unwrap().to_owned(), evidence)
        })
        .collect::<BTreeMap<_, _>>();
    if catalog_before != catalog_after {
        return Err("diagnostic catalog roster or held identities changed".into());
    }
    write_json(
        &output.join("diagnostic.json"),
        &json!({"diagnostic_only":true,
        "retirement_acceptance":false,"original_source":SOURCE,"original_manifest_sha256":MANIFEST,
        "original_family_directory":original_family,"compiler_target":read_json(&target),
        "current_catalog_receipt":read_json(&receipt),"model_run_exit":result.code,
        "comparisons":comparisons,"held_inputs":held.iter().map(Held::evidence).collect::<Vec<_>>(),
        "interpreter_execution":"not_measured"}),
    );
    Ok(())
}

#[cfg(test)]
mod drift_tests {
    use super::*;

    fn words(bits: &[u32]) -> Vec<u8> {
        bits.iter().flat_map(|bits| bits.to_le_bytes()).collect()
    }

    #[test]
    fn reports_finite_zero_and_nonfinite_bit_differences_separately() {
        let left = words(&[0xbda52f1a, 0, 0x7fc00001, 0x7f800000, 0x7fc00003]);
        let right = words(&[0xbda62ade, 0x80000000, 0x7fc00002, 0xff800000, 0x7fc00003]);
        let report = comparison(&left, &right);
        assert_eq!(report["differing_f32_count"], 4);
        assert_eq!(report["finite_nonzero_differences"], 1);
        assert_eq!(report["signed_zero_differences"], 1);
        assert_eq!(report["nonfinite_differences"], 2);
        // The existing field counts all nonfinite pairs, including identical NaNs.
        assert_eq!(report["nonfinite_pairs"], 3);
        assert_eq!(
            report["first_differences"][0],
            json!({
                "index":0,"left_bits":"bda52f1a","right_bits":"bda62ade","kind":"finite_nonzero"
            })
        );
        assert_eq!(report["first_differences"][1]["kind"], "signed_zero");
        assert_eq!(report["first_differences"][2]["kind"], "nonfinite");
        assert_eq!(report["difference_samples_truncated"], false);
        assert_eq!(
            report["max_finite_f32_absolute_difference"],
            0.00048020482063293457
        );
    }

    #[test]
    fn keeps_complete_counts_when_difference_samples_are_bounded() {
        let left = words(&[1.0_f32.to_bits(); 553]);
        let right = words(&[2.0_f32.to_bits(); 553]);
        let report = comparison(&left, &right);
        assert_eq!(report["differing_f32_count"], 553);
        assert_eq!(report["finite_nonzero_differences"], 553);
        assert_eq!(report["first_differences"].as_array().unwrap().len(), 8);
        assert_eq!(report["first_differences"][7]["index"], 7);
        assert_eq!(report["difference_samples_truncated"], true);
        assert_eq!(report["max_finite_f32_absolute_difference"], 1.0);
        let equal = comparison(&right, &right);
        assert_eq!(equal["differing_f32_count"], 0);
        assert_eq!(equal["first_differences"], json!([]));
        assert_eq!(equal["byte_equal"], true);
    }

    #[test]
    #[ignore = "requires independently hashed raw predictions from job119931046645; no model execution"]
    fn saved_job119931046645_has_nonzero_driver_drift() {
        let root = required("DRIVER_DRIFT_SAVED_PREDICTIONS");
        let expected = "99b8b2fe7aa17098a366d1bb5e40a2b5a40561f0186ab4dcd33fb29a6dc358fc";
        let observed = "58456ffe906d7d9aa9dbfe7e530dc55b0251b141511018018eebf78e95800b64";
        let warmup = "ace8ee6c6ff7cf8aa6f1881dd9eae6dc9cba15bb75bcc7bd70a5af03ddef500e";
        let mut held = Vec::new();
        let mut load = |frame: &str, kind: &str, expected_sha: &str| {
            let path = root.join(format!("{frame}-{kind}.raw"));
            held.push(Held::open(&path));
            let bytes = fs::read(path).unwrap();
            assert_eq!(bytes.len(), 2212);
            assert_eq!(bytes_sha(&bytes), expected_sha);
            bytes
        };
        let original = load("frame-0", "original", expected);
        let native = load("frame-0", "model", observed);
        let daemon = load("frame-0", "daemon", observed);
        assert_eq!(native, daemon);
        let warm_original = load("warmup", "original", warmup);
        let warm_native = load("warmup", "model", warmup);
        let warm_daemon = load("warmup", "daemon", warmup);
        assert_eq!(warm_original, warm_native);
        assert_eq!(warm_original, warm_daemon);
        let report = comparison(&native, &original);
        assert_eq!(report["differing_f32_count"], 439);
        assert_eq!(report["finite_nonzero_differences"], 439);
        assert_eq!(report["signed_zero_differences"], 0);
        assert_eq!(report["nonfinite_differences"], 0);
        assert_eq!(report["max_finite_f32_absolute_difference"], 0.01171875);
        assert_eq!(
            report["first_differences"][0],
            json!({
                "index":0,"left_bits":"bda52f1a","right_bits":"bda62ade","kind":"finite_nonzero"
            })
        );
        assert_eq!(
            comparison(&warm_native, &warm_original)["differing_f32_count"],
            0
        );
        for input in &held {
            input.verify();
        }
        println!("saved-driver-drift-report={report}");
    }
}
