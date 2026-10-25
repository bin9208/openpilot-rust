#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_onnx::{lower::LoweredGraph, Model, Value};
use serde_json::Value as Json;
fn fixture() -> Json {
    let fixture: Json =
        serde_json::from_str(include_str!("fixtures/compiler/original-gemm-chain.json")).unwrap();
    assert_eq!(fixture["cache_disabled"], true);
    assert_eq!(fixture["compile_to_obj_calls"], 26);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 12);
    fixture
}
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn closed_chain_owns_both_readers_and_terminal_biased_gemm() {
    use openpilot_model_runtime::compiler_backend_cpu::matrix::chain::closures;
    let fixture = fixture();
    for case in fixture["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let selected = closures(&graph, &model).unwrap();
        assert_eq!(selected.len(), 1, "{}", case["name"]);
        let chain = &selected[0];
        assert_eq!(chain.blocks.len(), 2);
        assert_eq!(chain.root, graph.names["x"]);
        assert_eq!(chain.output, graph.names["output"]);
        assert!(!chain.owned.contains(&chain.root));
        for name in [
            "h1",
            "r1",
            "m1",
            "add1",
            "activated1",
            "h2",
            "r2",
            "m2",
            "add2",
            "activated2",
            "output",
        ] {
            assert!(chain.owned.contains(&graph.names[name]), "owned{name}");
        }
        // A held intermediate changes the original materialization. Whole-chain
        // ownership must reject it; raw held root remains represented above.
        for name in [
            "h1",
            "r1",
            "m1",
            "add1",
            "activated1",
            "h2",
            "r2",
            "m2",
            "add2",
            "activated2",
        ] {
            let mut held = graph.clone();
            held.outputs
                .insert(format!("held_{name}"), held.names[name]);
            assert!(closures(&held, &model).unwrap().is_empty(), "held{name}");
        }
        for source in ["x", "r1", "activated1", "activated2"] {
            let mut shared = model.clone();
            let mut reader = shared
                .graph
                .nodes
                .iter()
                .find(|n| n.op == "Relu")
                .unwrap()
                .clone();
            reader.inputs = vec![source.into()];
            reader.outputs = vec!["outside".into()];
            shared.graph.nodes.push(reader);
            let mut output: Value = shared.graph.outputs[0].clone();
            output.name = "outside".into();
            output.shape = graph.plans[graph.names[source]]
                .ty
                .shape
                .iter()
                .map(|&d| openpilot_model_runtime::compiler_onnx::ShapeDimension::Fixed(d as i64))
                .collect();
            shared.graph.outputs.push(output);
            let lowered = LoweredGraph::lower_offline(&shared).unwrap();
            assert!(
                closures(&lowered, &shared).unwrap().is_empty(),
                "outside reader of{source}"
            );
        }
    }
}

#[test]
fn staged_chain_preserves_packed_edges_until_both_readers_finish() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::chain::{closures, stage},
        compiler_onnx::{memory::plan_memory_enabled, schedule::schedule_plans},
    };
    use std::collections::{BTreeMap, BTreeSet};
    for case in fixture()["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let selected = closures(&graph, &model).unwrap();
        let chain = &selected[0];
        let mut schedule = schedule_plans(&graph, "CPU", true).unwrap();
        let mut initialized = BTreeMap::new();
        let mut scratch = BTreeSet::new();
        let kernels = stage(
            &selected,
            &graph,
            &mut schedule,
            &mut initialized,
            &mut scratch,
        )
        .unwrap();
        let calls = kernels
            .iter()
            .map(|(&index, kernel)| {
                let call = &schedule.calls[index];
                assert_eq!(kernel.bytes.len(), call.buffers.len());
                for (&buffer, &bytes) in call.buffers.iter().zip(&kernel.bytes) {
                    assert_eq!(schedule.buffers[buffer].bytes, bytes);
                }
                assert!(kernel.workers > 0);
                call
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2 + chain.blocks.len() * 2);
        assert_eq!(calls[0].buffers[1], chain.root);
        let mut packed_root = calls[0].buffers[0];
        for (block_index, block) in chain.blocks.iter().enumerate() {
            let producer = calls[1 + block_index * 2];
            let residual = calls[2 + block_index * 2];
            assert_eq!(producer.buffers.len(), 4);
            assert_eq!(residual.buffers.len(), 5);
            assert_eq!(producer.buffers[1], packed_root);
            assert_eq!(residual.buffers[1], packed_root);
            assert_eq!(residual.buffers[2], producer.buffers[0]);
            assert_ne!(residual.buffers[0], packed_root);
            assert_ne!(residual.buffers[0], producer.buffers[0]);
            assert_eq!(producer.buffers[3], block.biases[0]);
            assert_eq!(residual.buffers[4], block.biases[1]);
            assert!(initialized.contains_key(&producer.buffers[2]));
            assert!(initialized.contains_key(&residual.buffers[3]));
            assert!(scratch.contains(&packed_root));
            assert!(scratch.contains(&producer.buffers[0]));
            packed_root = residual.buffers[0];
        }
        let terminal = calls.last().unwrap();
        assert_eq!(terminal.buffers.len(), 4);
        assert_eq!(terminal.buffers[0], chain.output);
        assert_eq!(terminal.buffers[1], packed_root);
        assert_eq!(terminal.buffers[3], chain.terminal_bias);
        assert!(initialized.contains_key(&terminal.buffers[2]));
        assert_eq!(
            kernels.values().filter(|k| k.scalars == 1).count(),
            chain.blocks.len() * 2 + 1
        );
        assert!(schedule
            .calls
            .iter()
            .all(|c| { !chain.owned.contains(&c.buffers[0]) || c.buffers[0] == chain.output }));
        // Verify complete overlapping lifetimes, including the packed root's
        // first-Gemm reader through its later residual reader. Checking only
        // distinct IDs would miss physical arena reuse across those calls.
        let mut intervals = BTreeMap::<usize, (usize, usize)>::new();
        for (time, call) in schedule.calls.iter().enumerate() {
            for &id in &call.buffers {
                intervals
                    .entry(id)
                    .and_modify(|v| v.1 = time)
                    .or_insert((time, time));
            }
        }
        for enabled in [false, true] {
            let memory = plan_memory_enabled(&schedule.buffers, &schedule.calls, enabled).unwrap();
            if !enabled {
                assert!(memory.views.is_empty());
                continue;
            }
            for (&a, va) in &memory.views {
                for (&b, vb) in memory.views.range((a + 1)..) {
                    let (af, al) = intervals[&a];
                    let (bf, bl) = intervals[&b];
                    if af <= bl && bf <= al && va.arena == vb.arena {
                        assert!(
                            va.offset + va.bytes <= vb.offset || vb.offset + vb.bytes <= va.offset,
                            "{}: live storage overlap {a}/{b}",
                            case["name"]
                        );
                    }
                }
            }
        }
    }
}
