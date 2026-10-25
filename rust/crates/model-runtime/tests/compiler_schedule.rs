use openpilot_model_runtime::compiler_schedule::{pipeline_entrypoints, PipelineStage};
use serde_json::{json, Value};

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

// Execute only in Actions. Extract the unchanged boundary statements from the
// actual Python sources; do not substitute candidate-derived expected ranges.
// No tensor, compiler, model, GPU or constant-preparation execution is involved.
const ORIGINAL_BOUNDARIES: &str = r#"
import ast, dataclasses, hashlib, json, pathlib, sys, types
root = pathlib.Path(sys.argv[1])
schema_path = root / 'rust/tools/model_export/schema.py'
schedule_path = root / 'rust/tools/model_export/offline_schedule.py'
schema_bytes, schedule_bytes = schema_path.read_bytes(), schedule_path.read_bytes()
schema = ast.parse(schema_bytes, filename=str(schema_path))
schedule = ast.parse(schedule_bytes, filename=str(schedule_path))
scope = {'__name__': '__main__', 'dataclass': dataclasses.dataclass, 'ExportError': RuntimeError}
for name in ('Entrypoint', 'entrypoint_version'):
    node = next(node for node in schema.body if getattr(node, 'name', None) == name)
    exec(compile(ast.Module(body=[node], type_ignores=[]), str(schema_path), 'exec'), scope)
def function(name):
    return next(node for node in schedule.body if getattr(node, 'name', None) == name)
def assigned(node, name):
    return isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == name for target in node.targets)
body = function('export_pipeline').body
first = next(i for i, node in enumerate(body) if assigned(node, 'retained'))
last = next(i for i, node in enumerate(body) if assigned(node, 'model'))
retained_code = compile(ast.Module(body=body[first:last], type_ignores=[]), str(schedule_path), 'exec')
body = function('emit_schedule').body
first = next(i for i, node in enumerate(body) if assigned(node, 'entries'))
last = next(i for i, node in enumerate(body) if assigned(node, 'manifest'))
emitted_code = compile(ast.Module(body=body[first:last], type_ignores=[]), str(schedule_path), 'exec')
manifest = body[last].value
assert isinstance(manifest, ast.Call) and isinstance(manifest.func, ast.Name) and manifest.func.id == 'Manifest'
version_code = compile(ast.Expression(body=manifest.args[0]), str(schedule_path), 'eval')
results = []
for case in json.load(sys.stdin):
    local = dict(scope, pipeline=types.SimpleNamespace(stages=[types.SimpleNamespace(
        name=stage['name'], linear=types.SimpleNamespace(src=stage['calls'])) for stage in case['stages']]),
        prepared=types.SimpleNamespace(linear=types.SimpleNamespace(src=case['retained'])))
    try:
        exec(retained_code, local)
        local.update(schedule=types.SimpleNamespace(stage_ends=local['ends']),
                     emitted_by_source=case['prefix'], calls=range(case['prefix'][-1]))
        exec(emitted_code, local)
        version = eval(version_code, local)
        results.append({'version': version, 'entrypoints': [dataclasses.asdict(entry) for entry in local['entries']]})
    except (RuntimeError, IndexError):
        results.append({'error': True})
print(json.dumps({'source_sha256': {'schema.py': hashlib.sha256(schema_bytes).hexdigest(),
                                  'offline_schedule.py': hashlib.sha256(schedule_bytes).hexdigest()}, 'results': results}))
"#;

#[test]
#[ignore = "GHA-only original Python boundary comparison; no model/compiled execution"]
fn original_python_pipeline_boundaries_match_native() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut child = Command::new("python3")
        .args(["-I", "-c", ORIGINAL_BOUNDARIES])
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("GHA must provide python3");
    // Close stdin after writing so the original json.load can complete.
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let original: Value = serde_json::from_slice(&output.stdout).unwrap();
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
