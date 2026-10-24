#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_backend_cpu::llvm::{compile_isolated, Config};
use openpilot_model_runtime::compiler_backend_cpu::matrix::MatrixLayout;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command, time::Duration};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/compiler/original-batched-matmul.json"
    ))
    .unwrap()
}
fn dims(v: &Value) -> Vec<usize> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as usize)
        .collect()
}
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_image_dot_physical_shapes_and_packing() {
    let fixture = fixture();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (path, expected) in fixture["provenance"]["source_sha256"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root.join(path)).unwrap())),
            expected.as_str().unwrap(),
            "original source drift: {path}"
        );
    }
    for case in fixture["layouts"].as_array().unwrap() {
        let layout =
            MatrixLayout::new(&dims(&case["input_shape"]), &dims(&case["weight_shape"])).unwrap();
        let boundaries = case["boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["op"] == "CONTIGUOUS")
            .collect::<Vec<_>>();
        assert_eq!(layout.input_shape, dims(&boundaries[0]["shape"]));
        assert_eq!(layout.weight_shape, dims(&boundaries[1]["shape"]));
    }
    for case in fixture["cases"].as_array().unwrap() {
        let layout =
            MatrixLayout::new(&dims(&case["input_shape"]), &dims(&case["weight_shape"])).unwrap();
        assert_eq!(
            layout.output_elements * 4,
            case["original_output_bytes"].as_u64().unwrap() as usize
        );
        for kind in ["input", "weights"] {
            let logical = if kind == "input" {
                decode(case["input_hex"].as_str().unwrap())
            } else {
                decode(case["weight_pattern_hex"].as_str().unwrap())
                    .into_iter()
                    .cycle()
                    .take(layout.input_channels * layout.output_channels * 2)
                    .collect()
            };
            if kind == "weights" {
                assert_eq!(
                    format!("{:x}", Sha256::digest(&logical)),
                    case["logical_weights_sha256"].as_str().unwrap()
                );
            }
            let bytes = case[format!("packed_{kind}_bytes")].as_u64().unwrap() as usize;
            let mut packed = vec![0; bytes];
            for chunk in packed.chunks_exact_mut(2) {
                chunk.copy_from_slice(&[0x55, 0xa3]);
            }
            if kind == "input" {
                layout.pack_input_into(&logical, &mut packed)
            } else {
                layout.pack_weights_into(&logical, &mut packed)
            }
            .unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&packed)),
                case[format!("packed_{kind}_sha256")].as_str().unwrap(),
                "{}:{kind}",
                case["name"]
            );
            let before = packed.clone();
            for short_input in [true, false] {
                let input = if short_input {
                    &logical[..logical.len() - 1]
                } else {
                    &logical
                };
                let output = if short_input {
                    packed.as_mut_slice()
                } else {
                    &mut packed[..bytes - 1]
                };
                let error = if kind == "input" {
                    layout.pack_input_into(input, output)
                } else {
                    layout.pack_weights_into(input, output)
                };
                assert!(error.is_err(), "{}:{kind}: invalid size", case["name"]);
            }
            assert_eq!(packed, before, "invalid size must not partially write");
        }
    }
}

#[test]
fn invalid_matrix_dimensions_are_rejected_without_allocating() {
    for (input, weights) in [
        (vec![], vec![8, 12]),
        (vec![8], vec![8, 12]),
        (vec![1, 0, 8], vec![8, 12]),
        (vec![1, 3, 8], vec![8, 12, 1]),
        (vec![1, 3, 8], vec![12, 12]),
        (vec![1, 3, 7], vec![7, 12]),
        (vec![1, 3, 8], vec![8, 11]),
        (vec![usize::MAX, 2, 3, 8], vec![8, 12]),
        (vec![usize::MAX, 3, 8], vec![8, 12]),
        (vec![1, 3, 8], vec![8, usize::MAX - 3]),
        (vec![1, 1 << 24, 8], vec![8, 12]),
    ] {
        assert!(
            MatrixLayout::new(&input, &weights).is_err(),
            "invalid matrix dimensions accepted: {input:?} {weights:?}"
        );
    }
}

#[test]
fn original_padded_output_view_undo() {
    for case in fixture()["cases"].as_array().unwrap() {
        let layout =
            MatrixLayout::new(&dims(&case["input_shape"]), &dims(&case["weight_shape"])).unwrap();
        let physical = decode(case["physical_output_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&physical)),
            case["physical_output_sha256"].as_str().unwrap()
        );
        let expected = decode(case["output_hex"].as_str().unwrap());
        let mut logical = vec![0x55; expected.len()];
        layout.unpack_output_into(&physical, &mut logical).unwrap();
        assert_eq!(logical, expected, "{}", case["name"]);
        let before = logical.clone();
        assert!(layout
            .unpack_output_into(&physical[..physical.len() - 1], &mut logical)
            .is_err());
        assert_eq!(logical, before, "invalid size must not partially write");
        assert!(layout
            .unpack_output_into(&physical, &mut logical[..expected.len() - 1])
            .is_err());
        assert_eq!(logical, before, "invalid size must not partially write");
    }
}

#[test]
fn original_parameterized_matrix_schedules() {
    for case in fixture()["original_scheduling"].as_array().unwrap() {
        let layout =
            MatrixLayout::new(&dims(&case["input_shape"]), &dims(&case["weight_shape"])).unwrap();
        let workers = case["max_workers"].as_u64().unwrap_or(16) as usize;
        let schedule = layout.schedule(workers).unwrap();
        let sizes = |field: &str| {
            case[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().parse::<usize>().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(schedule.before, sizes("before"), "{case}");
        assert_eq!(schedule.after, sizes("after"), "{case}");
        assert_eq!(
            schedule.opts,
            case["opts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{case}"
        );
    }
}

#[test]
fn original_half_weight_materialization_preserves_nan_and_zero_bits() {
    let fixture = fixture();
    let case = &fixture["weight_half_materialization"];
    let layout =
        MatrixLayout::new(&dims(&case["input_shape"]), &dims(&case["weight_shape"])).unwrap();
    let logical = decode(case["logical_hex"].as_str().unwrap());
    let expected = decode(case["packed_hex"].as_str().unwrap());
    let mut packed = [0x55, 0xa3]
        .into_iter()
        .cycle()
        .take(expected.len())
        .collect::<Vec<_>>();
    layout.pack_weights_into(&logical, &mut packed).unwrap();
    assert_eq!(packed, expected);
}

#[repr(C, align(32))]
#[derive(Clone)]
struct Block([u8; 32]);
fn aligned(bytes: &[u8]) -> Vec<Block> {
    bytes
        .chunks(32)
        .map(|chunk| {
            let mut value = Block([0; 32]);
            value.0[..chunk.len()].copy_from_slice(chunk);
            value
        })
        .collect()
}

#[test]
fn original_matrix_kernel_matches_exact_physical_bytes() {
    let fixture = fixture();
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    let pin = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    assert!(fixture["provenance"]["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some(&pin)));
    let config = Config {
        library,
        library_sha256: pin,
        triple: "x86_64-none-unknown-elf".into(),
        cpu: "x86-64".into(),
        features: String::new(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    let mut products = BTreeMap::<String, tempfile::TempDir>::new();
    for case in fixture["cases"].as_array().unwrap() {
        let layout =
            MatrixLayout::new(&dims(&case["input_shape"]), &dims(&case["weight_shape"])).unwrap();
        let kernel = layout.artifact("matrix", "op_kernel_0", 16).unwrap();
        assert_eq!(
            kernel.bytes,
            [
                layout.output_elements * 4,
                layout.input_elements * 2,
                layout.weight_elements * 2
            ]
        );
        assert_eq!(kernel.scalars, 1);
        let expected = decode(case["physical_output_hex"].as_str().unwrap());
        let mut packed_input = vec![0xa3; kernel.bytes[1]];
        let mut packed_weights = vec![0xa3; kernel.bytes[2]];
        layout
            .pack_input_into(
                &decode(case["input_hex"].as_str().unwrap()),
                &mut packed_input,
            )
            .unwrap();
        let weights = decode(case["weight_pattern_hex"].as_str().unwrap())
            .into_iter()
            .cycle()
            .take(layout.input_channels * layout.output_channels * 2)
            .collect::<Vec<_>>();
        layout
            .pack_weights_into(&weights, &mut packed_weights)
            .unwrap();
        if let std::collections::btree_map::Entry::Vacant(entry) =
            products.entry(kernel.source.clone())
        {
            let compiled = compile_isolated(
                std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                &config,
                &kernel.source,
                Duration::from_secs(30),
            )
            .unwrap();
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("kernel.o"), compiled.object).unwrap();
            let linked = Command::new("cc")
                .arg("-shared")
                .arg("-Wl,-z,defs")
                .arg(dir.path().join("kernel.o"))
                .arg("-o")
                .arg(dir.path().join("kernel.so"))
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{}",
                String::from_utf8_lossy(&linked.stderr)
            );
            if let Some(proof) = std::env::var_os("MATRIX_PROOF_DIR") {
                let proof = PathBuf::from(proof);
                fs::create_dir_all(&proof).unwrap();
                let name = case["name"].as_str().unwrap();
                fs::write(proof.join(format!("{name}.ll")), &kernel.source).unwrap();
                fs::write(proof.join(format!("{name}.opt.ll")), compiled.optimized_ir).unwrap();
            }
            entry.insert(dir);
        }
        let input = aligned(&packed_input);
        let weights = aligned(&packed_weights);
        let mut output = aligned(&vec![0x55; expected.len()]);
        let pointers = [
            output.as_mut_ptr().cast::<std::ffi::c_void>(),
            input.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
            weights.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
        ];
        // SAFETY: this locally emitted synchronous wrapper accepts three
        // disjoint32-byte-aligned buffers of its declared sizes and one core ID.
        unsafe {
            let module =
                libloading::Library::new(products[&kernel.source].path().join("kernel.so"))
                    .unwrap();
            let run = module
                .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(
                    b"op_kernel_0\0",
                )
                .unwrap();
            for worker in 0..kernel.workers {
                run(pointers.as_ptr(), &(worker as i32));
            }
        }
        let actual = output
            .iter()
            .flat_map(|v| v.0)
            .take(expected.len())
            .collect::<Vec<_>>();
        let mismatch = actual.iter().zip(&expected).position(|(a, b)| a != b);
        assert!(
            mismatch.is_none(),
            "{}: first physical byte mismatch {:?}; actual SHA{:x}, expected SHA{:x}",
            case["name"],
            mismatch,
            Sha256::digest(&actual),
            Sha256::digest(&expected)
        );
    }
}

#[test]
fn original_matrix_input_and_output_boundary_abi() {
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    let config = Config {
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
    };
    let mut products = BTreeMap::<String, tempfile::TempDir>::new();
    for case in fixture()["cases"].as_array().unwrap() {
        let layout =
            MatrixLayout::new(&dims(&case["input_shape"]), &dims(&case["weight_shape"])).unwrap();
        for (entry, boundary, input, expected_bytes, expected_sha) in [
            (
                "pack",
                layout.input_artifact("pack").unwrap(),
                decode(case["input_hex"].as_str().unwrap()),
                layout.input_elements * 2,
                case["packed_input_sha256"].as_str().unwrap().to_owned(),
            ),
            (
                "undo",
                layout.output_artifact("undo").unwrap(),
                decode(case["physical_output_hex"].as_str().unwrap()),
                layout.batches * layout.rows * layout.output_channels * 4,
                format!(
                    "{:x}",
                    Sha256::digest(decode(case["output_hex"].as_str().unwrap()))
                ),
            ),
        ] {
            assert_eq!(boundary.bytes, [expected_bytes, input.len()]);
            assert_eq!(boundary.scalars, 0);
            assert_eq!(boundary.workers, 1);
            if let std::collections::btree_map::Entry::Vacant(item) =
                products.entry(boundary.source.clone())
            {
                let compiled = compile_isolated(
                    std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                    &config,
                    &boundary.source,
                    Duration::from_secs(30),
                )
                .unwrap();
                let dir = tempfile::tempdir().unwrap();
                fs::write(dir.path().join("boundary.o"), compiled.object).unwrap();
                let linked = Command::new("cc")
                    .arg("-shared")
                    .arg("-Wl,-z,defs")
                    .arg(dir.path().join("boundary.o"))
                    .arg("-o")
                    .arg(dir.path().join("boundary.so"))
                    .output()
                    .unwrap();
                assert!(
                    linked.status.success(),
                    "{}",
                    String::from_utf8_lossy(&linked.stderr)
                );
                item.insert(dir);
            }
            // Nonzero offset views exercise the same32-byte guarantee as the
            // memory planner, with untouched prefix/suffix canaries.
            let mut physical = aligned(&vec![0x55; expected_bytes + 64]);
            let mut source = aligned(&vec![0; input.len() + 64]);
            for (target, chunk) in source[1..].iter_mut().zip(input.chunks(32)) {
                target.0[..chunk.len()].copy_from_slice(chunk);
            }
            if entry == "pack" {
                for chunk in physical[1..]
                    .iter_mut()
                    .flat_map(|b| b.0.chunks_exact_mut(2))
                {
                    chunk.copy_from_slice(&[0x55, 0xa3]);
                }
            }
            let before = physical.clone();
            let pointers = [
                physical[1..].as_mut_ptr().cast::<std::ffi::c_void>(),
                source[1..].as_mut_ptr().cast::<std::ffi::c_void>(),
            ];
            // SAFETY: the local synchronous two-buffer ABI sees disjoint
            // aligned offset views of exactly its declared lengths; no scalars.
            unsafe {
                let module =
                    libloading::Library::new(products[&boundary.source].path().join("boundary.so"))
                        .unwrap();
                let run = module
                    .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(
                        format!("{entry}\0").as_bytes(),
                    )
                    .unwrap();
                run(pointers.as_ptr(), std::ptr::null());
            }
            let actual = physical
                .iter()
                .flat_map(|b| b.0)
                .skip(32)
                .take(expected_bytes)
                .collect::<Vec<_>>();
            assert_eq!(
                format!("{:x}", Sha256::digest(&actual)),
                expected_sha,
                "{}:{entry}",
                case["name"]
            );
            assert_eq!(physical[0].0, before[0].0);
            let after = physical
                .iter()
                .flat_map(|b| b.0)
                .skip(32 + expected_bytes)
                .collect::<Vec<_>>();
            let before = before
                .iter()
                .flat_map(|b| b.0)
                .skip(32 + expected_bytes)
                .collect::<Vec<_>>();
            assert_eq!(after, before, "output view canary");
        }
    }
}
