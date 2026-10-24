#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    CpuModel,
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
fn baseline_original_gelu_layout_controls_preserve_all_bits() {
    let fixture = fixture();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 72);
    assert!(fixture["real_compiles"].as_u64().unwrap() > 0);
    assert_eq!(fixture["cpu"], "x86-64");
    assert_eq!(fixture["provider_sha256"], provider().library_sha256);
    let mut failures = Vec::new();
    let mut exact = 0usize;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let destination = temp.path().join("bundle");
        let compilation = match compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: true,
                llvm: Some((
                    provider(),
                    std::path::PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                )),
                ..CompileOptions::default()
            },
            &destination,
        ) {
            Ok(compilation) => compilation,
            Err(error) => {
                failures.push(format!("{name}: admission {error}"));
                continue;
            }
        };
        if name.starts_with("direct-") {
            assert_eq!(
                compilation.graph.backend, "cpu-llvm",
                "{name} must select configured LLVM"
            );
        }
        if let Some(root) = std::env::var_os("MODEL_GELU_DIAGNOSTICS") {
            let directory = std::path::PathBuf::from(root).join(name);
            assert!(!directory.exists(), "preserve previous diagnostic bundles");
            fs::create_dir_all(&directory).unwrap();
            for entry in fs::read_dir(&destination).unwrap() {
                let entry = entry.unwrap();
                fs::copy(entry.path(), directory.join(entry.file_name())).unwrap();
            }
        }
        // SAFETY: locally generated source-matched immutable trusted artifact.
        let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
        model
            .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
        model.run();
        let expected = if case["outputs"].is_object() {
            case["outputs"].as_object().unwrap().clone()
        } else {
            serde_json::Map::from_iter([("output".into(), case["output_hex"].clone())])
        };
        let mut passed = true;
        for (output, expected) in expected {
            let mut actual = vec![0; model.output_size(&output).unwrap()];
            model.read_output(&output, &mut actual).unwrap();
            let expected = decode(expected.as_str().unwrap());
            assert_eq!(actual.len(), expected.len());
            let differences = actual
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| {
                    (
                        i,
                        format!("{:08x}", u32::from_le_bytes(a.try_into().unwrap())),
                        format!("{:08x}", u32::from_le_bytes(b.try_into().unwrap())),
                    )
                })
                .collect::<Vec<_>>();
            if !differences.is_empty() {
                passed = false;
                failures.push(format!(
                    "{name} {output}: {} differences, first {:?}",
                    differences.len(),
                    &differences[..differences.len().min(4)]
                ));
            }
        }
        if passed {
            exact += 1;
        }
    }
    eprintln!(
        "GELU controls exact {exact}/72; failures {}",
        failures.len()
    );
    for failure in &failures {
        eprintln!("{failure}");
    }
    assert!(
        failures.is_empty(),
        "baseline original GELU controls must preserve exact bits"
    );
}

#[test]
fn configured_llvm_gelu_primitive_matches_direct_original_layouts() {
    use openpilot_model_runtime::compiler_backend_cpu::{
        activation::GeluTanh, llvm::compile_isolated,
    };
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
                let mut b = Block([0; 32]);
                b.0[..chunk.len()].copy_from_slice(chunk);
                b
            })
            .collect::<Vec<_>>()
    };
    let mut exact = 0;
    let mut differences = Vec::new();
    let mut compiled = std::collections::BTreeMap::new();
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with("direct-"))
    {
        let shape = case["shape"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d.as_u64().unwrap() as usize)
            .collect::<Vec<_>>();
        let artifact = GeluTanh::new(&shape)
            .unwrap()
            .artifact("gelu", "op_kernel_0")
            .unwrap();
        let key = format!("{:x}", Sha256::digest(artifact.source.as_bytes()));
        let temp = compiled.entry(key).or_insert_with(|| {
            let temp = tempfile::tempdir().unwrap();
            let result = compile_isolated(
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
        let expected = decode(case["output_hex"].as_str().unwrap());
        let mut output = aligned(&vec![0; expected.len()]);
        let input = aligned(&decode(case["input_hex"].as_str().unwrap()));
        assert_eq!(artifact.bytes, [expected.len(); 2]);
        assert_eq!((artifact.workers, artifact.scalars), (1, 0));
        // SAFETY: configured local trusted artifact, aligned nonoverlapping live
        // buffers of its exact ABI sizes, synchronous call, immutable library.
        unsafe {
            let lib = libloading::Library::new(temp.path().join("kernel.so")).unwrap();
            let call: libloading::Symbol<
                unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32),
            > = lib.get(b"op_kernel_0").unwrap();
            let args = [output.as_mut_ptr().cast(), input.as_ptr().cast_mut().cast()];
            call(args.as_ptr(), std::ptr::null());
        }
        let actual = output
            .iter()
            .flat_map(|b| b.0)
            .take(expected.len())
            .collect::<Vec<_>>();
        if actual == expected {
            exact += 1;
        } else {
            differences.push(format!(
                "{} native{:x} original{:x}",
                case["name"],
                Sha256::digest(actual),
                Sha256::digest(expected)
            ));
        }
    }
    eprintln!("configured LLVM GELU direct exact {exact}/36");
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
