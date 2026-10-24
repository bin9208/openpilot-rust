#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{compiler_backend_cpu::llvm, compiler_onnx::Model};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
fn decode(raw: &str) -> Vec<u8> {
    raw.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn provider() -> openpilot_model_runtime::compiler_backend_cpu::llvm::Config {
    use openpilot_model_runtime::compiler_backend_cpu::llvm::Config;
    let library = std::path::PathBuf::from(
        std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"),
    );
    let library_sha256 = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    assert!(
        matches!(
            library_sha256.as_str(),
            "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
                | "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a"
        ),
        "unsupported LLVM provider identity"
    );
    Config {
        library,
        library_sha256,
        triple: "x86_64-none-unknown-elf".into(),
        cpu: "x86-64".into(),
        features: String::new(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    }
}
fn fixture() -> Value {
    let bytes = match provider().library_sha256.as_str() {
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0" => {
            include_str!("fixtures/compiler/original-gemm-chain.json")
        }
        "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a" => {
            include_str!("fixtures/compiler/original-gemm-chain20.json")
        }
        _ => unreachable!(),
    };
    let fixture: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(fixture["cache_disabled"], true);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 12);
    assert_eq!(fixture["library_sha256"], provider().library_sha256);
    fixture
}
#[test]
fn terminal_biased_gemm_preserves_all_original_physical_bytes() {
    use openpilot_model_runtime::compiler_backend_cpu::matrix::MatrixLayout;
    use std::{path::Path, process::Command, time::Duration};
    #[repr(C, align(32))]
    struct Block([u8; 32]);
    let aligned = |raw: &[u8]| {
        raw.chunks(32)
            .map(|chunk| {
                let mut block = Block([0; 32]);
                block.0[..chunk.len()].copy_from_slice(chunk);
                block
            })
            .collect::<Vec<_>>()
    };
    let fixture = fixture();
    let config = provider();
    let mut compiled = std::collections::BTreeMap::new();
    let mut exact = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let n = case["input_shape"][1].as_u64().unwrap() as usize;
        let layout = MatrixLayout::new(&[1, n], &[n, n]).unwrap();
        let artifact = layout
            .biased_artifact("terminal", "op_kernel_0", 16)
            .unwrap();
        let checkpoints = case["checkpoints"].as_array().unwrap();
        let terminal = checkpoints
            .iter()
            .rev()
            .find(|call| {
                call["source_sha256"].as_str().is_some_and(|key| {
                    fixture["programs"][key]["source"]
                        .as_str()
                        .unwrap()
                        .starts_with("define void @r_")
                })
            })
            .unwrap();
        let input = &terminal["arguments"][1];
        let bytes = input["bytes"].as_u64().unwrap() as usize;
        let offset = input["offset"].as_u64().unwrap() as usize;
        let snapshot = checkpoints
            .iter()
            .take_while(|call| call["call"] != terminal["call"])
            .flat_map(|call| call["snapshots"].as_array().unwrap())
            .filter(|s| {
                s["base"] == input["base"]
                    && s["offset"].as_u64().unwrap() as usize <= offset
                    && (s["offset"].as_u64().unwrap() as usize
                        + s["bytes"].as_u64().unwrap() as usize)
                        >= offset + bytes
            })
            .last()
            .unwrap();
        let raw = decode(snapshot["hex"].as_str().unwrap());
        let start = offset - snapshot["offset"].as_u64().unwrap() as usize;
        let raw_input = &raw[start..start + bytes];
        let expected = decode(terminal["snapshots"][0]["hex"].as_str().unwrap());
        assert_eq!(
            expected,
            decode(case["outputs"]["output"]["hex"].as_str().unwrap())
        );
        let mut logical_weight = vec![0; n * n * 2];
        let source_weight = &model.graph.initializers["w5"].data;
        for i in 0..n {
            for o in 0..n {
                logical_weight[(i * n + o) * 2..(i * n + o + 1) * 2]
                    .copy_from_slice(&source_weight[(o * n + i) * 2..(o * n + i + 1) * 2]);
            }
        }
        let mut packed_weight = vec![0; layout.weight_elements * 2];
        layout
            .pack_weights_into(&logical_weight, &mut packed_weight)
            .unwrap();
        let bias = &model.graph.initializers["b5"].data;
        assert_eq!(
            artifact.bytes,
            [
                expected.len(),
                raw_input.len(),
                packed_weight.len(),
                bias.len()
            ]
        );
        assert_eq!(artifact.scalars, 1);
        let key = format!("{:x}", Sha256::digest(artifact.source.as_bytes()));
        let directory = compiled.entry(key).or_insert_with(|| {
            let directory = tempfile::tempdir().unwrap();
            let compiled = llvm::compile_isolated(
                Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                &config,
                &artifact.source,
                Duration::from_secs(30),
            )
            .unwrap();
            fs::write(directory.path().join("kernel.o"), compiled.object).unwrap();
            let linked = Command::new("cc")
                .arg("-shared")
                .arg("-Wl,-z,defs")
                .arg(directory.path().join("kernel.o"))
                .arg("-o")
                .arg(directory.path().join("kernel.so"))
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{}",
                String::from_utf8_lossy(&linked.stderr)
            );
            directory
        });
        let mut output = aligned(&vec![0; expected.len()]);
        let input = aligned(raw_input);
        let weights = aligned(&packed_weight);
        let bias = aligned(bias);
        // SAFETY: locally generated trusted kernel, source-derived exact typed
        // capacities, disjoint32-bytealignedbuffers and synchronous worker calls.
        unsafe {
            let library = libloading::Library::new(directory.path().join("kernel.so")).unwrap();
            let run: libloading::Symbol<
                unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32),
            > = library.get(b"op_kernel_0").unwrap();
            let pointers = [
                output.as_mut_ptr().cast(),
                input.as_ptr().cast_mut().cast(),
                weights.as_ptr().cast_mut().cast(),
                bias.as_ptr().cast_mut().cast(),
            ];
            for worker in 0..artifact.workers {
                run(pointers.as_ptr(), &(worker as i32));
            }
        }
        let actual = output
            .iter()
            .flat_map(|b| b.0)
            .take(expected.len())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{}", case["name"]);
        exact += 1;
    }
    assert_eq!(exact, 12);
}
