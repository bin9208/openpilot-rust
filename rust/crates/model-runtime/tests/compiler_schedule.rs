use openpilot_model_runtime::compiler_schedule::{pipeline_entrypoints, PipelineStage};
use serde_json::json;
#[cfg(feature = "native-skip-miri")]
use serde_json::Value;

fn stages(values: &[(&str, &[u64])]) -> Vec<PipelineStage> {
    values
        .iter()
        .map(|(name, calls)| PipelineStage {
            name: (*name).into(),
            calls: calls.to_vec(),
        })
        .collect()
}

#[test]
fn pruned_constants_and_empty_stages_preserve_the_emitted_partition() {
    let stages = stages(&[
        ("prepare", &[10, 11]),
        ("empty", &[]),
        ("policy", &[12, 13, 14]),
        ("removed", &[15]),
    ]);
    let result = pipeline_entrypoints(&stages, &[11, 12, 14], &[0, 1, 2, 3]).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"version":2, "entrypoints":[
            {"name":"prepare","start":0,"end":1},
            {"name":"empty","start":1,"end":1},
            {"name":"policy","start":1,"end":3},
            {"name":"removed","start":3,"end":3}
        ]})
    );
}

#[test]
fn uses_emitted_positions_instead_of_source_or_unique_kernel_counts() {
    // The helper consumes emission accounting rather than guessing a one-to-one
    // mapping: zero/expanded emissions must also retain the stage boundaries.
    let result = pipeline_entrypoints(
        &stages(&[("prepare", &[1]), ("policy", &[2, 3])]),
        &[1, 2, 3],
        &[0, 2, 2, 5],
    )
    .unwrap();
    assert_eq!(result.entrypoints[0].range(), 0..2);
    assert_eq!(result.entrypoints[1].range(), 2..5);
}

#[test]
fn repeated_source_identities_are_counted_at_every_occurrence() {
    let result = pipeline_entrypoints(
        &stages(&[("prepare", &[7, 8]), ("policy", &[7, 9])]),
        &[7, 7, 9],
        &[0, 1, 2, 3],
    )
    .unwrap();
    assert_eq!(result.entrypoints[0].range(), 0..1);
    assert_eq!(result.entrypoints[1].range(), 1..3);
}

#[test]
fn no_stages_selects_v1_and_all_removed_stages_still_select_v2() {
    for (retained, prefix) in [(vec![], vec![0]), (vec![1], vec![0, 1])] {
        let result = pipeline_entrypoints(&[], &retained, &prefix).unwrap();
        assert_eq!(result.version, 1);
        assert!(result.entrypoints.is_empty());
    }
    let result =
        pipeline_entrypoints(&stages(&[("prepare", &[1]), ("policy", &[2])]), &[], &[0]).unwrap();
    assert_eq!(result.version, 2);
    assert!(result
        .entrypoints
        .iter()
        .all(|entry| entry.range() == (0..0)));
}

#[test]
fn rejects_invalid_names_incomplete_coverage_and_stage_overrun() {
    for stages in [
        stages(&[("", &[1])]),
        stages(&[("same", &[]), ("same", &[1])]),
        stages(&[("incomplete", &[])]),
        stages(&[("overrun", &[1, 1])]),
    ] {
        assert!(pipeline_entrypoints(&stages, &[1], &[0, 1]).is_err());
    }
}

#[test]
fn rejects_malformed_emission_prefixes_before_indexing() {
    for prefix in [vec![], vec![1], vec![0, 1], vec![0, 3, 2], vec![0, 1, 2, 3]] {
        assert!(pipeline_entrypoints(&[], &[1, 2], &prefix).is_err());
    }
}

#[test]
fn accepts_exactly_65536_named_empty_stages_and_rejects_one_more() {
    let mut stages = (0..65536)
        .map(|index| PipelineStage {
            name: format!("stage-{index}"),
            calls: vec![],
        })
        .collect::<Vec<_>>();
    let result = pipeline_entrypoints(&stages, &[], &[0]).unwrap();
    assert_eq!(result.version, 2);
    assert_eq!(result.entrypoints.len(), 65536);
    stages.push(PipelineStage {
        name: "one-too-many".into(),
        calls: vec![],
    });
    assert!(pipeline_entrypoints(&stages, &[], &[0]).is_err());
}

// source7a928 actual original stage73 observation. Data-only reader: no Python,
// subprocess, source AST lookup or current original installation is required.
#[cfg(feature = "native-skip-miri")]
mod frozen_stage {
    use super::{json, Value};
    use sha2::{Digest, Sha256};

    const RECORD_LIMIT: usize = 16 * 1024 * 1024;
    const TOTAL_LIMIT: usize = 64 * 1024 * 1024;
    const SOURCE: &str = "7a928cefe22e3c81bcb941f6ed20eaa80eeabc59";
    const ROOT: &str = "/home/actions/actions-runner/_work/openpilot-rust/openpilot-rust/.diag296-source-40236805936-1.iO4Nek/source";
    const JOB: &str = "/tmp/openpilot-ci.oTOw0T";
    const PYTHON: &str = "/usr/bin/python3.12";
    const PYTHON_SHA: &str = "e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f";
    const RECORDS: &[(&str, usize, &str, &[u8])] = &[
        (
            "observation.json",
            19411,
            "e128237987e14b4ed20d3514fb25f2f243736815d8e784b0ff99fd4fd5e2d39c",
            include_bytes!("fixtures/compiler-stage-original/observation.json"),
        ),
        (
            "oracle.txt",
            2870,
            "f47b29f458a1f143926fd16e00d5861cfd92c97bce0c3bb1d3536b9c4e992d5f",
            include_bytes!("fixtures/compiler-stage-original/oracle.txt"),
        ),
        (
            "python.sha256",
            86,
            "74a2bf284a5116d43270762ab1766fe74421da338fa0889fcd2c188202eae773",
            include_bytes!("fixtures/compiler-stage-original/python.sha256"),
        ),
        (
            "python.version",
            14,
            "5b3e43dc38ca01e3f5a7854ba50d4330864b0c1e0f646650b2c78cf072acc366",
            include_bytes!("fixtures/compiler-stage-original/python.version"),
        ),
        (
            "request.json",
            4444067,
            "e00ad79756d2e51bc63eb87b13b12832177faf7bdab4cda92526e1c044e5bf85",
            include_bytes!("fixtures/compiler-stage-original/request.json"),
        ),
        (
            "stderr.bin",
            0,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            include_bytes!("fixtures/compiler-stage-original/stderr.bin"),
        ),
        (
            "stdout.bin",
            3079930,
            "67596fc5e6cde23b1df9b08d3d93dc932e17aefa512a4aa2805f9c21731ede32",
            include_bytes!("fixtures/compiler-stage-original/stdout.bin"),
        ),
    ];
    const SOURCE_FILES: &[(&str, &str, usize, &str)] = &[
        (
            "schema.py",
            "rust/tools/model_export/schema.py",
            2611,
            "720abcc396852a395689c4d37f9487822f951166a36b5efe8586dce5f656c6a9",
        ),
        (
            "offline_schedule.py",
            "rust/tools/model_export/offline_schedule.py",
            16463,
            "bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144",
        ),
        (
            "native-schedule",
            "rust/crates/model-runtime/src/compiler_schedule.rs",
            2686,
            "0447c3a4213dc278b62505e4875ab24ce53fbf2c596aa712490a9e69c6aeafb3",
        ),
        (
            "native-entrypoint",
            "rust/crates/model-runtime/src/entrypoint.rs",
            1408,
            "ab651a93d6fa2425c3a7f2725c20fa0fb7df39c308f13a6f910d07f4046d3712",
        ),
        (
            "native-lib",
            "rust/crates/model-runtime/src/lib.rs",
            1118,
            "9c18a2434644ea72ebbd5d2d23c706084391775995e0567c6a0d1809f8f1d741",
        ),
        (
            "native-test",
            "rust/crates/model-runtime/tests/compiler_schedule.rs",
            24499,
            "a065d00bab2accb1aae19ec89d5293ee321829b85dc0cbd654a84b0535d26f6a",
        ),
        (
            "native-manifest",
            "rust/crates/model-runtime/Cargo.toml",
            1605,
            "53d766c5fa51dab5e4be7c7b793e82d591a0bf7970784b68638c9b5159e56866",
        ),
        (
            "native-lock",
            "rust/Cargo.lock",
            159864,
            "9586516b4adda7abc3f739f2c3360a6cd2370e8f2d785a6b36a4adfcf54f6b9a",
        ),
    ];

    fn sha256(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn bytes(name: &str) -> &'static [u8] {
        RECORDS.iter().find(|record| record.0 == name).unwrap().3
    }

    fn witness(value: &Value, path: &str, length: usize, digest: &str) {
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert_eq!(value["path"], path);
        assert_eq!(value["sha256"], digest);
        let identity = &value["identity"];
        assert_eq!(identity["bytes"], length);
        assert_eq!(identity["links"], 1);
        assert_eq!(identity["mode"].as_u64().unwrap() & 0o170000, 0o100000);
    }

    pub fn reference(request: &[u8]) -> Value {
        let mut total = 0;
        for &(name, length, digest, raw) in RECORDS {
            assert_eq!(raw.len(), length, "frozen bytes: {name}");
            assert_eq!(sha256(raw), digest, "frozen SHA256: {name}");
            assert!(raw.len() <= RECORD_LIMIT);
            total += raw.len();
        }
        assert_eq!(total, 7_546_378);
        assert!(total <= TOTAL_LIMIT);
        // Pin original raw bytes above. Compare the request structurally so
        // serde_json preserve_order changes object key order, not the73 inputs.
        let input: Value = serde_json::from_slice(bytes("request.json")).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(request).unwrap(), input);
        assert_eq!(input.as_array().unwrap().len(), 73);
        let observation: Value = serde_json::from_slice(bytes("observation.json")).unwrap();
        assert_eq!(observation["schema_version"], 1);
        assert_eq!(observation["kind"], "independent-python-stage-observation");
        assert_eq!(observation["github_sha"], SOURCE);
        assert_eq!(observation["source_root"], ROOT);
        assert_eq!(observation["case_count"], 73);
        for name in [
            "binding_unchanged",
            "original_source_sha_bound",
            "collector_only",
        ] {
            assert_eq!(observation[name], true);
        }
        for name in ["stdout_limit_reached", "stderr_limit_reached"] {
            assert_eq!(observation[name], false);
        }
        assert_eq!(observation["retirement_credit"], 0);
        assert_eq!(observation["record_limit_bytes"], RECORD_LIMIT);
        assert_eq!(observation["stream_limit_bytes"], RECORD_LIMIT);
        assert_eq!(observation["total_limit_bytes"], TOTAL_LIMIT);
        assert_eq!(
            observation["request"],
            json!({
                "record":"request.json","bytes":bytes("request.json").len(),
                "sha256":sha256(bytes("request.json")),"record_unchanged":true
            })
        );
        assert_eq!(
            observation["original"],
            json!({
                "exit_code":0,"returncode":0,"signal":null,"file_size_limit_signal":false,"input_error":null,
                "stdout":{"record":"stdout.bin","bytes":bytes("stdout.bin").len(),"sha256":sha256(bytes("stdout.bin"))},
                "stderr":{"record":"stderr.bin","bytes":0,"sha256":sha256(bytes("stderr.bin"))}
            })
        );
        assert!(bytes("stderr.bin").is_empty());
        assert_eq!(
            observation["argv"],
            json!([
                PYTHON,
                "-I",
                "-c",
                std::str::from_utf8(bytes("oracle.txt")).unwrap(),
                format!("{ROOT}/rust/crates/model-runtime/../../..")
            ])
        );
        assert_eq!(
            bytes("python.sha256"),
            format!("{PYTHON_SHA}  {PYTHON}\n").as_bytes()
        );
        assert_eq!(bytes("python.version"), b"Python 3.12.3\n");

        let before = &observation["witnesses_before"];
        assert_eq!(before, &observation["witnesses_after"]);
        assert_eq!(before.as_object().unwrap().len(), 13);
        for &(alias, relative, length, digest) in SOURCE_FILES {
            witness(
                &before[alias],
                &format!("{ROOT}/{relative}"),
                length,
                digest,
            );
        }
        witness(&before["admitted-python"], PYTHON, 8_020_928, PYTHON_SHA);
        assert_ne!(
            before["admitted-python"]["identity"]["mode"]
                .as_u64()
                .unwrap()
                & 0o111,
            0
        );
        for (name, raw) in [
            ("SOURCE_COMMIT", format!("{SOURCE}\n").into_bytes()),
            (
                "stage-oracle-python.sha256",
                bytes("python.sha256").to_vec(),
            ),
            (
                "stage-oracle-python.version",
                bytes("python.version").to_vec(),
            ),
        ] {
            witness(
                &before[name],
                &format!("{JOB}/proof/{name}"),
                raw.len(),
                &sha256(&raw),
            );
        }
        let owner = b"40236805936:1:native-units";
        witness(
            &before["job-owner"],
            &format!("{JOB}/owner"),
            owner.len(),
            &sha256(owner),
        );

        let original: Value = serde_json::from_slice(bytes("stdout.bin")).unwrap();
        assert_eq!(original.as_object().unwrap().len(), 2);
        let hashes = original["source_sha256"].as_object().unwrap();
        assert_eq!(hashes.len(), 2);
        for name in ["schema.py", "offline_schedule.py"] {
            assert_eq!(hashes[name], before[name]["sha256"]);
        }
        let results = original["results"].as_array().unwrap();
        assert_eq!(results.len(), 73);
        let errors = results
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.get("error").is_some().then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(errors, [66, 67, 68, 69, 72]);
        for index in errors {
            assert_eq!(results[index], json!({"error":true}));
        }
        assert_eq!(input[71]["stages"].as_array().unwrap().len(), 65536);
        assert_eq!(input[72]["stages"].as_array().unwrap().len(), 65537);
        assert_eq!(results[71]["version"], 2);
        assert_eq!(results[71]["entrypoints"].as_array().unwrap().len(), 65536);
        assert_eq!(results[72], json!({"error":true}));
        original
    }
}

#[cfg(feature = "native-skip-miri")]
#[test]
fn frozen_original_pipeline_boundaries_match_native() {
    let mut cases = Vec::new();
    // Exhaust every removal subset of four source identities across three
    // stages, with a repeated identity straddling the stage boundary.
    for mask in 0..16u64 {
        for repeated in [false, true] {
            let stage_calls: Vec<Vec<u64>> = if repeated {
                vec![vec![0, 1], vec![0, 2], vec![3]]
            } else {
                vec![vec![0, 1], vec![], vec![2, 3]]
            };
            let retained = stage_calls
                .iter()
                .flatten()
                .copied()
                .filter(|id| mask & (1u64 << *id) != 0)
                .collect::<Vec<_>>();
            for expanded in [false, true] {
                let mut prefix = vec![0usize];
                for index in 0..retained.len() {
                    prefix.push(prefix.last().unwrap() + if expanded { index % 3 } else { 1 });
                }
                cases.push(
                    json!({"stages":stage_calls.iter().enumerate().map(|(index, calls)|
                    json!({"name":format!("stage-{index}"),"calls":calls})).collect::<Vec<_>>(),
                    "retained":retained,"prefix":prefix}),
                );
            }
        }
    }
    cases.extend([
        json!({"stages":[],"retained":[],"prefix":[0]}),
        json!({"stages":[],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"","calls":[0]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"x","calls":[]},{"name":"x","calls":[0]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"missing","calls":[]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"overrun","calls":[0,0]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"한글 stage","calls":[0]}],"retained":[0],"prefix":[0,1]}),
    ]);
    for count in [65536, 65537] {
        cases.push(json!({"stages":(0..count).map(|index| json!({"name":format!("stage-{index}"),"calls":[]})).collect::<Vec<_>>(),
            "retained":[],"prefix":[0]}));
    }
    assert_eq!(cases.len(), 73);
    let original = frozen_stage::reference(&serde_json::to_vec(&cases).unwrap());
    eprintln!("original boundary sources: {}", original["source_sha256"]);
    assert_eq!(original["results"].as_array().unwrap().len(), cases.len());
    for (index, (case, expected)) in cases
        .iter()
        .zip(original["results"].as_array().unwrap())
        .enumerate()
    {
        let stages: Vec<PipelineStage> = serde_json::from_value(case["stages"].clone()).unwrap();
        let retained: Vec<u64> = serde_json::from_value(case["retained"].clone()).unwrap();
        let prefix: Vec<usize> = serde_json::from_value(case["prefix"].clone()).unwrap();
        let actual = pipeline_entrypoints(&stages, &retained, &prefix);
        if expected.get("error").is_some() {
            assert!(actual.is_err(), "original rejected case {index}");
        } else {
            assert_eq!(
                serde_json::to_value(actual.unwrap()).unwrap(),
                *expected,
                "case {index}"
            );
        }
    }
}
