#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::llvm,
    compiler_onnx::{Attribute, Model},
};
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
    let raw = match config.library_sha256.as_str() {
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0" => {
            include_str!("fixtures/compiler/original-gelu-layout18.json")
        }
        "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a" => {
            include_str!("fixtures/compiler/original-gelu-layout20.json")
        }
        _ => unreachable!("provider() validates identity"),
    };
    let value: Value = serde_json::from_str(raw).unwrap();
    assert_eq!(value["provider_sha256"], config.library_sha256);
    assert_eq!(value["cache_disabled"], true);
    value
}
#[test]
fn masked_depthwise_rejects_incompletely_unrolled_reduction_axes() {
    use openpilot_model_runtime::compiler_backend_cpu::convolution::MaskedDepthwise;
    // heuristic.py unrolls the last reduction axis up to 32, then only a
    // second axis when both it and the last axis are <=3. Total area alone
    // therefore cannot prove this primitive's fully expanded reduction DAG.
    for (kernel, pads) in [
        ([3, 5], [1, 2, 1, 2]),
        ([5, 3], [2, 1, 2, 1]),
        ([2, 4], [1, 1, 0, 2]),
        ([4, 2], [1, 1, 2, 0]),
    ] {
        assert!(
            MaskedDepthwise::new([4, 4, 7], kernel, pads).is_err(),
            "both reduction axes need the retained source unroll proof: {kernel:?}"
        );
    }
}
#[test]
fn original_masked_depthwise_primitive_preserves_every_output_bit() {
    use openpilot_model_runtime::compiler_backend_cpu::convolution::MaskedDepthwise;
    use std::{path::Path, process::Command, time::Duration};
    let fixture = fixture();
    let config = provider();
    #[repr(C, align(32))]
    #[derive(Clone)]
    struct Block([u8; 32]);
    let aligned = |bytes: &[u8]| {
        bytes
            .chunks(32)
            .map(|chunk| {
                let mut block = Block([0; 32]);
                block.0[..chunk.len()].copy_from_slice(chunk);
                block
            })
            .collect::<Vec<_>>()
    };
    let mut compiled = std::collections::BTreeMap::new();
    let mut exact = 0;
    let mut failures = Vec::new();
    let mut selected = 0;
    for case in fixture["cases"].as_array().unwrap() {
        if case["held"] != true {
            continue;
        }
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let Some(conv) = model.graph.nodes.iter().find(|n| n.op == "Conv") else {
            continue;
        };
        let shape = case["shape"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as usize)
            .collect::<Vec<_>>();
        let weights = &model.graph.initializers[&conv.inputs[1]];
        if conv.attributes.get("group") != Some(&Attribute::Integer(shape[1] as i64))
            || weights.shape[1] != 1
            || weights.shape[2..].iter().product::<i64>() == 1
        {
            continue;
        }
        let Attribute::Integers(pads) = &conv.attributes["pads"] else {
            panic!("fixture explicit padding");
        };
        let bias = &model.graph.initializers[&conv.inputs[2]];
        assert_eq!((weights.dtype, bias.dtype), (10, 10));
        let layout = MaskedDepthwise::new(
            [shape[1], shape[2], shape[3]],
            [weights.shape[2] as usize, weights.shape[3] as usize],
            pads.iter()
                .map(|&v| usize::try_from(v).unwrap())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        let artifact = layout.artifact("conv", "op_kernel_0").unwrap();
        // Reject malformed storage before any pointer enters the kernel ABI.
        assert!(layout.pack_input(&[0; 3]).is_err());
        assert!(layout.pack_weights(&[0; 3]).is_err());
        assert!(layout.artifact("bad-name", "op_kernel_0").is_err());
        let key = format!("{:x}", Sha256::digest(artifact.source.as_bytes()));
        let temp = compiled.entry(key).or_insert_with(|| {
            let temp = tempfile::tempdir().unwrap();
            let result = llvm::compile_isolated(
                Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                &config,
                &artifact.source,
                Duration::from_secs(30),
            )
            .unwrap();
            fs::write(temp.path().join("kernel.ll"), &artifact.source).unwrap();
            fs::write(temp.path().join("kernel.o"), result.object).unwrap();
            let linked = Command::new("cc")
                .arg("-shared")
                .arg("-Wl,-z,defs")
                .arg(temp.path().join("kernel.o"))
                .arg("-o")
                .arg(temp.path().join("kernel.so"))
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{}",
                String::from_utf8_lossy(&linked.stderr)
            );
            temp
        });
        let expected = decode(case["outputs"]["conv"].as_str().unwrap());
        let packed_input = layout
            .pack_input(&decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
        let packed_weights = layout.pack_weights(&weights.data).unwrap();
        assert_eq!(
            artifact.bytes,
            [
                expected.len(),
                packed_input.len(),
                packed_weights.len(),
                bias.data.len()
            ]
        );
        assert_eq!((artifact.workers, artifact.scalars), (1, 0));
        let mut output = aligned(&vec![0; expected.len()]);
        let input = aligned(&packed_input);
        let weight = aligned(&packed_weights);
        let bias = aligned(&bias.data);
        // SAFETY: locally generated trusted source-bound kernel, exact typed
        // ABI byte sizes, distinct aligned live buffers, synchronous call.
        unsafe {
            let library = libloading::Library::new(temp.path().join("kernel.so")).unwrap();
            let call: libloading::Symbol<
                unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32),
            > = library.get(b"op_kernel_0").unwrap();
            let pointers = [
                output.as_mut_ptr().cast(),
                input.as_ptr().cast_mut().cast(),
                weight.as_ptr().cast_mut().cast(),
                bias.as_ptr().cast_mut().cast(),
            ];
            call(pointers.as_ptr(), std::ptr::null());
        }
        let actual = output
            .iter()
            .flat_map(|b| b.0)
            .take(expected.len())
            .collect::<Vec<_>>();
        selected += 1;
        if actual == expected {
            exact += 1;
        } else {
            let differences = actual
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| {
                    (
                        i,
                        u32::from_le_bytes(a.try_into().unwrap()),
                        u32::from_le_bytes(b.try_into().unwrap()),
                    )
                })
                .collect::<Vec<_>>();
            failures.push(format!(
                "{} {} differences first{:x?}",
                case["name"],
                differences.len(),
                &differences[..differences.len().min(4)]
            ));
        }
    }
    assert_eq!(
        selected, 9,
        "all three structural depthwise profiles and modes required"
    );
    eprintln!("masked depthwise primitive exact{exact}/9");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
