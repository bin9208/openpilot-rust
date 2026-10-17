#[allow(dead_code)]
#[path = "../src/compiler_onnx/mod.rs"]
mod compiler_onnx;
use compiler_onnx::memory::{plan_memory, plan_memory_enabled, BufferSpec, Call};
#[test]
fn reuses_closed_lifetimes_but_keeps_overlapping_buffers_separate() {
    let buffers = (0..3)
        .map(|_| BufferSpec {
            device: "QCOM".into(),
            bytes: 256,
            held: false,
            supports_offset: false,
        })
        .collect::<Vec<_>>();
    let calls = vec![
        Call {
            copy: false,
            buffers: vec![0],
        },
        Call {
            copy: false,
            buffers: vec![0, 1],
        },
        Call {
            copy: false,
            buffers: vec![1],
        },
        Call {
            copy: false,
            buffers: vec![2],
        },
    ];
    let plan = plan_memory(&buffers, &calls).unwrap();
    assert_eq!(plan.arenas[0].bytes, 512);
    assert_eq!(plan.views[&0].offset, 0);
    assert_eq!(plan.views[&1].offset, 256);
    assert_eq!(plan.views[&2].offset, 0);
}

#[test]
fn independent_original_memory_plans_preserve_tlsf_offsets_lanes_and_arenas() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-memory.json")).unwrap();
    let mut cases = fixture["cases"].as_array().unwrap().clone();
    assert_eq!(cases.len(), 109);
    for json in [
        include_str!("fixtures/compiler/dmonitoring_model-memory.json"),
        include_str!("fixtures/compiler/driving_supercombo-memory.json"),
    ] {
        let actual_model: serde_json::Value = serde_json::from_str(json).unwrap();
        cases.push(actual_model["case"].clone());
    }
    assert_eq!(cases.len(), 111);
    for case in cases {
        let buffers = case["buffers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| BufferSpec {
                device: v["device"].as_str().unwrap().into(),
                bytes: v["bytes"].as_u64().unwrap() as usize,
                held: v["held"].as_bool().unwrap(),
                supports_offset: v["supports_offset"].as_bool().unwrap(),
            })
            .collect::<Vec<_>>();
        let calls = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| Call {
                copy: v["copy"].as_bool().unwrap(),
                buffers: v["buffers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as usize)
                    .collect(),
            })
            .collect::<Vec<_>>();
        let native =
            plan_memory_enabled(&buffers, &calls, case["disabled"].as_bool() != Some(true));
        if case.get("expected_error").is_some() {
            assert!(native.is_err(), "{}", case["name"]);
            continue;
        }
        let native = native.unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
        let actual = serde_json::json!({"arenas":native.arenas.iter().map(|a|serde_json::json!({"device":a.device,"lane":a.lane,"bytes":a.bytes})).collect::<Vec<_>>(),"views":native.views.iter().map(|(i,v)|(i.to_string(),serde_json::json!({"arena":v.arena,"offset":v.offset,"bytes":v.bytes}))).collect::<serde_json::Map<_,_>>()});
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
}

#[test]
fn typed_schedule_preserves_dependency_order_prunes_dead_plans_and_holds_bindings() {
    use compiler_onnx::{
        lower::{IndexExpr, LoweredGraph, Plan, ScalarExpr, TensorType},
        scalar::Binary,
    };
    let mut graph = LoweredGraph::default();
    let ty = TensorType {
        dtype: 1,
        shape: vec![4],
    };
    graph.plans.push(Plan {
        ty: ty.clone(),
        expression: ScalarExpr::Input("input".into()),
    });
    graph.plans.push(Plan {
        ty: ty.clone(),
        expression: ScalarExpr::Constant(vec![0; 16]),
    });
    for (kind, a) in [(Binary::Add, 0), (Binary::Sub, 2)] {
        graph.plans.push(Plan {
            ty: ty.clone(),
            expression: ScalarExpr::Binary {
                kind,
                dtype: 1,
                left: Box::new(ScalarExpr::Load {
                    plan: a,
                    index: IndexExpr::Index,
                }),
                right: Box::new(ScalarExpr::Load {
                    plan: 1,
                    index: IndexExpr::Index,
                }),
            },
        });
    }
    graph.plans.push(Plan {
        ty,
        expression: ScalarExpr::FloatLiteral {
            dtype: 1,
            value: 99.,
        },
    });
    graph.outputs.insert("output".into(), 3);
    let schedule = compiler_onnx::schedule::schedule_plans(&graph, "QCOM", false).unwrap();
    assert_eq!(
        schedule
            .calls
            .iter()
            .map(|v| v.buffers.clone())
            .collect::<Vec<_>>(),
        vec![vec![2, 0, 1], vec![3, 2, 1]]
    );
    assert!(schedule.buffers[0].held && schedule.buffers[1].held && schedule.buffers[3].held);
    assert!(!schedule.buffers[2].held);
    assert!(!schedule.live.contains(&4));
    let memory = plan_memory(&schedule.buffers, &schedule.calls).unwrap();
    assert_eq!(memory.views.len(), 1);
    assert_eq!(memory.views[&2].bytes, 16);
}
