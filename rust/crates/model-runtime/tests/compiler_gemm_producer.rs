#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_backend_cpu::{
    llvm::{compile_isolated, Config},
    matrix::MatrixLayout,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, process::Command, time::Duration};
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn provider() -> Config {
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    Config {
        library_sha256: format!("{:x}", Sha256::digest(fs::read(&library).unwrap())),
        library,
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
#[repr(C, align(32))]
#[derive(Clone)]
struct Block([u8; 32]);
fn aligned(bytes: &[u8]) -> Vec<Block> {
    bytes
        .chunks(32)
        .map(|raw| {
            let mut b = Block([0; 32]);
            b.0[..raw.len()].copy_from_slice(raw);
            b
        })
        .collect()
}
fn kernel(config: &Config, source: &str, temp: &std::path::Path) {
    let built = compile_isolated(
        std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
        config,
        source,
        Duration::from_secs(30),
    )
    .unwrap();
    fs::write(temp.join("kernel.o"), built.object).unwrap();
    assert!(Command::new("cc")
        .arg("-shared")
        .arg("-Wl,-z,defs")
        .arg(temp.join("kernel.o"))
        .arg("-o")
        .arg(temp.join("kernel.so"))
        .status()
        .unwrap()
        .success());
}
fn run(temp: &std::path::Path, buffers: &mut [Vec<Block>], workers: usize) {
    let ptrs = buffers
        .iter_mut()
        .map(|b| b.as_mut_ptr().cast::<std::ffi::c_void>())
        .collect::<Vec<_>>();
    // SAFETY: exact locally generated immutable artifact and declared disjoint,
    // aligned buffers. Calls are synchronous and the library outlives them.
    unsafe {
        let library = libloading::Library::new(temp.join("kernel.so")).unwrap();
        let invoke = library
            .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(b"op_kernel_0\0")
            .unwrap();
        for core in 0..workers {
            invoke(ptrs.as_ptr(), &(core as i32));
        }
    }
}
#[test]
fn original_gemm_biased_relu_producer_physical_bytes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-gemm-residual.json"
    ))
    .unwrap();
    let config = provider();
    let receipts = &fixture["provider_cases"][&config.library_sha256];
    assert!(
        receipts.is_object(),
        "missing independently executed provider receipt"
    );
    for (n, k) in [(4, 8), (8, 4), (32, 64), (512, 1024)] {
        eprintln!("Gemm producer N={n} K={k}");
        let layout = MatrixLayout::new(&[1, n], &[n, k]).unwrap();
        let artifact = layout
            .biased_relu_artifact("gemm_producer", "op_kernel_0", 16)
            .unwrap();
        if let Some(path) = std::env::var_os("MODEL_GEMM_DEBUG_DIR") {
            let path = PathBuf::from(path);
            fs::create_dir_all(&path).unwrap();
            fs::write(
                path.join(format!("native-producer-N{n}-K{k}.ll")),
                &artifact.source,
            )
            .unwrap();
        }
        let temp = tempfile::tempdir().unwrap();
        kernel(&config, &artifact.source, temp.path());
        for case in fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["input_channels"] == k && c["output_channels"] == n)
        {
            let receipt = &receipts[case["name"].as_str().unwrap()];
            assert_eq!(
                artifact.bytes.to_vec(),
                receipt["first_kernel"]["bytes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                artifact.workers,
                receipt["first_kernel"]["workers"][0].as_u64().unwrap() as usize
            );
            assert_eq!(artifact.scalars, 1);
            let mut root = vec![0xa3; artifact.bytes[1]];
            let raw = decode(receipt["root_valid_packed_hex"].as_str().unwrap());
            root[..raw.len()].copy_from_slice(&raw);
            let weight = decode(case["weight1_pattern_hex"].as_str().unwrap())
                .into_iter()
                .cycle()
                .take(n * k * 2)
                .collect::<Vec<_>>();
            assert_eq!(
                format!("{:x}", Sha256::digest(&weight)),
                case["weight1_sha256"].as_str().unwrap()
            );
            let mut logical = vec![0; weight.len()];
            for i in 0..n {
                for o in 0..k {
                    logical[(i * k + o) * 2..(i * k + o + 1) * 2]
                        .copy_from_slice(&weight[(o * n + i) * 2..(o * n + i + 1) * 2]);
                }
            }
            assert_eq!(
                format!("{:x}", Sha256::digest(&logical)),
                receipt["first_weight_materialized_sha256"]
                    .as_str()
                    .unwrap()
            );
            let mut packed = vec![0xa3; artifact.bytes[2]];
            layout.pack_weights_into(&logical, &mut packed).unwrap();
            let bias = decode(case["bias1_hex"].as_str().unwrap());
            assert_eq!(bias.len(), artifact.bytes[3]);
            let mut buffers = vec![
                aligned(&vec![0x55; artifact.bytes[0]]),
                aligned(&root),
                aligned(&packed),
                aligned(&bias),
            ];
            run(temp.path(), &mut buffers, artifact.workers);
            let expected = decode(receipt["hidden_valid_packed_hex"].as_str().unwrap());
            let output = buffers[0]
                .iter()
                .flat_map(|b| b.0)
                .take(expected.len())
                .collect::<Vec<_>>();
            assert!(
                output == expected,
                "{} firstbyte={:?} actual={:x} original={:x}",
                case["name"],
                output.iter().zip(&expected).position(|(a, b)| a != b),
                Sha256::digest(&output),
                Sha256::digest(&expected)
            );
        }
    }
}
