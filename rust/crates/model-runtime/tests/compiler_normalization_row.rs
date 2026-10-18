#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_backend_cpu::llvm;
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
    let config = provider();
    let bytes = match config.library_sha256.as_str() {
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0" => {
            include_str!("fixtures/compiler/original-normalization-row18.json")
        }
        "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a" => {
            include_str!("fixtures/compiler/original-normalization-row20.json")
        }
        _ => unreachable!(),
    };
    let mut fixture: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(fixture["cache_disabled"], true);
    assert_eq!(fixture["cpu_count"], 16);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 48);
    assert_eq!(fixture["library_sha256"], config.library_sha256);
    assert_eq!(
        fixture["distinct_scalar_fused_cases"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // The optional explicit gate appends actual tensors only from an ignored
    // cold original receipt. The mandatory public48 remain in every run; private
    // intermediate tensor bytes are never embedded in the tracked fixture.
    if let Some(path) = std::env::var_os("MODEL_NORM_ACTUAL_RECEIPT") {
        let actual: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(actual["library_sha256"], config.library_sha256);
        assert_eq!(actual["cache_disabled"], true);
        assert_eq!(
            actual["source_model_sha256"],
            "f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5"
        );
        assert_eq!(
            actual["output_sha256"],
            "ab8ec2368b047a59f166197be451274052a8db4205fa167e0318a86c56f71d0a"
        );
        let boundaries = actual["normalization_boundaries"].as_array().unwrap();
        assert_eq!(boundaries.len(), 2);
        for boundary in boundaries {
            assert_eq!(boundary["buffers"][0]["bytes"], 4);
            assert_eq!(boundary["buffers"][1]["bytes"], 2048);
            let key = boundary["program_source_sha256"].as_str().unwrap();
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(boundary["source"].as_str().unwrap().as_bytes())
                ),
                key
            );
            assert_eq!(
                actual["provider_real_object_sha256"][key],
                boundary["object_sha256"]
            );
            fixture["cases"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({
                    "name": format!("actual274-immediate-norm-call{}", boundary["call_index"]),
                    "elements": 512,
                    "input_hex": boundary["buffers_hex"][1],
                    "input_sha256": boundary["buffers_sha256"][1],
                    "output_hex": boundary["buffers_hex"][0],
                    "output_sha256": boundary["buffers_sha256"][0],
                }));
        }
        assert_eq!(fixture["cases"].as_array().unwrap().len(), 50);
    }
    fixture
}
#[test]
fn row_norm_zero_clamp_preserves_all_original_physical_bytes() {
    use openpilot_model_runtime::compiler_backend_cpu::normalization::RowNorm;
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
        let elements = case["elements"].as_u64().unwrap() as usize;
        let artifact = RowNorm::new(elements)
            .unwrap()
            .artifact("row_norm", "op_kernel_0")
            .unwrap();
        let raw_input = decode(case["input_hex"].as_str().unwrap());
        let expected = decode(case["output_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw_input)),
            case["input_sha256"]
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&expected)),
            case["output_sha256"]
        );
        assert_eq!(artifact.bytes, [4, elements * 4]);
        assert_eq!(artifact.scalars, 0);
        assert_eq!(artifact.workers, 1);
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
        let input = aligned(&raw_input);
        // SAFETY: locally generated trusted kernel, source-derived exact typed
        // capacities, disjoint32-bytealignedbuffers and synchronous worker calls.
        unsafe {
            let library = libloading::Library::new(directory.path().join("kernel.so")).unwrap();
            let run: libloading::Symbol<
                unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32),
            > = library.get(b"op_kernel_0").unwrap();
            let pointers = [output.as_mut_ptr().cast(), input.as_ptr().cast_mut().cast()];
            for worker in 0..artifact.workers {
                assert_eq!(worker, 0);
                run(pointers.as_ptr(), std::ptr::null());
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
    assert_eq!(exact, fixture["cases"].as_array().unwrap().len());
}
