#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{
        image::{BufferKind, Image1x1},
        llvm::{compile_isolated, Compiled, Config},
    },
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command, time::Duration};

#[test]
fn original_logical_nchw_materialization_preserves_padding() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-image-materialization.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let channels = case["channels"].as_u64().unwrap() as usize;
        let image = Image1x1 {
            channels_in: channels,
            channels_out: channels,
            width: case["width"].as_u64().unwrap() as usize,
        };
        for kind in ["input", "weights"] {
            let expected = decode(case[format!("packed_{kind}_hex")].as_str().unwrap());
            let logical = decode(case[format!("logical_{kind}_hex")].as_str().unwrap());
            let fill = decode(case["fill_hex"].as_str().unwrap());
            let mut actual = fill
                .iter()
                .copied()
                .cycle()
                .take(expected.len())
                .collect::<Vec<_>>();
            let result = if kind == "input" {
                image.pack_input_into(&logical, &mut actual)
            } else {
                image.pack_weights_into(&logical, &mut actual)
            };
            result.unwrap_or_else(|e| panic!("{}:{kind}: {e}", case["name"]));
            assert_eq!(actual, expected, "{}:{kind}", case["name"]);
            let before = actual.clone();
            let error = if kind == "input" {
                image.pack_input_into(&logical[..logical.len() - 1], &mut actual)
            } else {
                image.pack_weights_into(&logical[..logical.len() - 1], &mut actual)
            };
            assert!(error.is_err());
            assert_eq!(actual, before, "invalid size must not partially write");
        }
    }
}

#[test]
fn logical_input_runtime_staging_matches_original_image_kernel() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-image-logical-artifact.json"
    ))
    .unwrap();
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    let pin = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    let provider = fixture["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["library_sha256"] == pin)
        .unwrap();
    let config = Config {
        library,
        library_sha256: pin,
        triple: provider["triple"].as_str().unwrap().into(),
        cpu: provider["cpu"].as_str().unwrap().into(),
        features: provider["features"].as_str().unwrap().into(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    let mut products = BTreeMap::<String, tempfile::TempDir>::new();
    for case in provider["cases"].as_array().unwrap() {
        let channels = case["channels"].as_u64().unwrap() as usize;
        let image = Image1x1 {
            channels_in: channels,
            channels_out: channels,
            width: case["width"].as_u64().unwrap() as usize,
        };
        let kernel = image.logical_artifact("packed", "op_kernel_0").unwrap();
        assert_eq!(
            kernel.buffers,
            [
                BufferKind::F32,
                BufferKind::F32,
                BufferKind::U16,
                BufferKind::U16
            ]
        );
        assert_eq!(kernel.scalars, 0);
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
            let staged = compile_isolated(
                std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                &config,
                &kernel.staging_source,
                Duration::from_secs(30),
            )
            .unwrap();
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("kernel.o"), compiled.object).unwrap();
            fs::write(dir.path().join("staging.o"), staged.object).unwrap();
            let linked = Command::new("cc")
                .arg("-shared")
                .arg("-Wl,-z,defs")
                .arg(dir.path().join("kernel.o"))
                .arg(dir.path().join("staging.o"))
                .arg("-o")
                .arg(dir.path().join("kernel.so"))
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{}",
                String::from_utf8_lossy(&linked.stderr)
            );
            entry.insert(dir);
        }
        let input = aligned(&decode(case["logical_input_hex"].as_str().unwrap()));
        let weights = aligned(&decode(case["weights_hex"].as_str().unwrap()));
        let mut scratch = aligned(&vec![0xa3; kernel.bytes[2]]);
        let mut output = aligned(&vec![0; kernel.bytes[0]]);
        let pointers = [
            output.as_mut_ptr().cast::<std::ffi::c_void>(),
            input.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
            scratch.as_mut_ptr().cast::<std::ffi::c_void>(),
            weights.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
        ];
        // SAFETY: the generated typed wrapper is synchronous and receives four
        // disjoint 32-byte-aligned buffers of exactly its declared byte lengths.
        unsafe {
            let module =
                libloading::Library::new(products[&kernel.source].path().join("kernel.so"))
                    .unwrap();
            let run = module
                .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(
                    b"op_kernel_0\0",
                )
                .unwrap();
            run(pointers.as_ptr(), std::ptr::null());
        }
        let actual = output
            .iter()
            .flat_map(|b| b.0)
            .take(kernel.bytes[0])
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            decode(case["output_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
        let mut expected_packed = vec![0xa3; kernel.bytes[2]];
        image
            .pack_input_into(
                &decode(case["logical_input_hex"].as_str().unwrap()),
                &mut expected_packed,
            )
            .unwrap();
        assert_eq!(
            scratch
                .iter()
                .flat_map(|b| b.0)
                .take(kernel.bytes[2])
                .collect::<Vec<_>>(),
            expected_packed,
            "{}:scratch",
            case["name"]
        );
    }
}

#[test]
fn packed_llvm_object_runs_through_cpu_artifact_abi_and_offset_views() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-image-packed.json")).unwrap();
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    let pin = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    let provider = fixture["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["library_sha256"] == pin)
        .unwrap();
    let config = Config {
        library,
        library_sha256: pin,
        triple: provider["triple"].as_str().unwrap().into(),
        cpu: provider["cpu"].as_str().unwrap().into(),
        features: provider["features"].as_str().unwrap().into(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    let cases = provider["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["channels"] == 8 && c["width"] == 3)
        .collect::<Vec<_>>();
    let image = Image1x1 {
        channels_in: 8,
        channels_out: 8,
        width: 3,
    };
    let kernel = image.artifact("packed", "op_kernel_0").unwrap();
    assert_eq!(
        kernel.buffers,
        [BufferKind::F32, BufferKind::U16, BufferKind::U16]
    );
    assert_eq!(kernel.scalars, 0);
    let compiled = compile_isolated(
        std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
        &config,
        &kernel.source,
        Duration::from_secs(30),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("kernel.o"), &compiled.object).unwrap();
    let linked = Command::new("cc")
        .arg("-shared")
        .arg("-Wl,-z,defs")
        .arg(directory.path().join("kernel.o"))
        .arg("-o")
        .arg(directory.path().join("kernels.so"))
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let mut weights = vec![0x5a; 32];
    weights.extend(decode(cases[0]["weights_hex"].as_str().unwrap()));
    weights.extend([0x5a; 32]);
    fs::write(directory.path().join("weights.bin"), &weights).unwrap();
    let allocations = kernel.bytes.iter().enumerate().map(|(i, bytes)| serde_json::json!({"bytes":bytes+64,"weight_offset":if i==2 {Some(0)} else {None}})).collect::<Vec<_>>();
    let mut views = kernel
        .bytes
        .iter()
        .enumerate()
        .map(|(i, bytes)| serde_json::json!({"allocation":i,"offset":32,"bytes":bytes}))
        .collect::<Vec<_>>();
    views.push(serde_json::json!({"allocation":0,"offset":0,"bytes":kernel.bytes[0]+64}));
    let graph = serde_json::json!({"version":1,"backend":"cpu-llvm","arch":std::env::consts::ARCH,"weights_sha256":format!("{:x}",Sha256::digest(&weights)),"library_sha256":format!("{:x}",Sha256::digest(fs::read(directory.path().join("kernels.so")).unwrap())),"allocations":allocations,"views":views,"inputs":[{"name":"input","view":1},{"name":"output_storage","view":3}],"outputs":[{"name":"output","view":0},{"name":"output_storage","view":3}],"kernels":[{"buffers":3,"scalars":0}],"calls":[{"kernel":0,"views":[0,1,2],"scalars":[],"workers":1,"core_id":null}]});
    fs::write(
        directory.path().join("graph.json"),
        serde_json::to_vec(&graph).unwrap(),
    )
    .unwrap();
    // SAFETY: this test builds the typed synchronous wrapper and all view ranges
    // from its descriptor; the artifact remains owned/immutable until model drop.
    let mut model = unsafe { CpuModel::load(directory.path()) }.unwrap();
    for case in cases {
        model
            .write_input("output_storage", &vec![0x5a; kernel.bytes[0] + 64])
            .unwrap();
        model
            .write_input("input", &decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
        model.run();
        let mut actual = vec![0; kernel.bytes[0]];
        model.read_output("output", &mut actual).unwrap();
        assert_eq!(
            actual,
            decode(case["output_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
        let mut storage = vec![0; kernel.bytes[0] + 64];
        model.read_output("output_storage", &mut storage).unwrap();
        assert_eq!(&storage[..32], &[0x5a; 32]);
        assert_eq!(&storage[storage.len() - 32..], &[0x5a; 32]);
    }
}

fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}

#[repr(C, align(32))]
#[derive(Clone)]
struct Block([u8; 32]);
fn aligned(data: &[u8]) -> Vec<Block> {
    let mut blocks = vec![Block([0; 32]); data.len().div_ceil(32)];
    for (block, source) in blocks.iter_mut().zip(data.chunks(32)) {
        block.0[..source.len()].copy_from_slice(source);
    }
    blocks
}

#[test]
fn materialized_image_original_bytes() {
    check_fixture(include_str!("fixtures/compiler/original-image-packed.json"));
}

#[test]
fn varied_weights_and_looped_reductions_original_bytes() {
    check_fixture(include_str!(
        "fixtures/compiler/original-image-packed-controls.json"
    ));
}

fn check_fixture(json: &str) {
    let fixture: Value = serde_json::from_str(json).unwrap();
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    let pin = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    let provider = fixture["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["library_sha256"] == pin)
        .expect("LLVM provider has no independent original packed IMAGE capture");
    let config = Config {
        library,
        library_sha256: pin,
        triple: provider["triple"].as_str().unwrap().into(),
        cpu: provider["cpu"].as_str().unwrap().into(),
        features: provider["features"].as_str().unwrap().into(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (path, expected) in fixture["provenance"]["source_contributors"]
        .as_object()
        .unwrap()
    {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root.join(path)).unwrap())),
            expected.as_str().unwrap(),
            "original source drift: {path}"
        );
    }
    let mut products = BTreeMap::<String, (tempfile::TempDir, Compiled)>::new();
    for case in provider["cases"].as_array().unwrap() {
        let channels = case["channels"].as_u64().unwrap() as usize;
        let width = case["width"].as_u64().unwrap() as usize;
        let source = Image1x1 {
            channels_in: channels,
            channels_out: channels,
            width,
        }
        .render("packed")
        .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
        let layout = Image1x1 {
            channels_in: channels,
            channels_out: channels,
            width,
        }
        .layout()
        .unwrap();
        assert_eq!(
            decode(case["input_hex"].as_str().unwrap()).len(),
            layout.input_elements * 2
        );
        assert_eq!(
            decode(case["weights_hex"].as_str().unwrap()).len(),
            layout.weight_elements * 2
        );
        if let std::collections::btree_map::Entry::Vacant(entry) = products.entry(source.clone()) {
            let compiled = compile_isolated(
                std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                &config,
                &source,
                Duration::from_secs(30),
            )
            .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("kernel.o"), &compiled.object).unwrap();
            let output = Command::new("cc")
                .arg("-shared")
                .arg("-Wl,-z,defs")
                .arg(dir.path().join("kernel.o"))
                .arg("-o")
                .arg(dir.path().join("kernel.so"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            entry.insert((dir, compiled));
        }
        let (dir, compiled) = &products[&source];
        let input = aligned(&decode(case["input_hex"].as_str().unwrap()));
        let weights = aligned(&decode(case["weights_hex"].as_str().unwrap()));
        let expected = decode(case["output_hex"].as_str().unwrap());
        let mut output = aligned(&vec![0; expected.len()]);
        // SAFETY: the locally emitted kernel has precisely this three-pointer ABI.
        // Each disjoint allocation meets the original 32-byte alignment promise,
        // and the preserved materialized fixture provides every accessed element.
        unsafe {
            let module = libloading::Library::new(dir.path().join("kernel.so")).unwrap();
            let run = module
                .get::<unsafe extern "C" fn(*mut u8, *const u8, *const u8)>(b"packed\0")
                .unwrap();
            run(
                output.as_mut_ptr().cast(),
                input.as_ptr().cast(),
                weights.as_ptr().cast(),
            );
        }
        let actual = output
            .iter()
            .flat_map(|v| v.0)
            .take(expected.len())
            .collect::<Vec<_>>();
        if let Some(path) = std::env::var_os("IMAGE_PROOF_DIR") {
            let path = PathBuf::from(path);
            fs::create_dir_all(&path).unwrap();
            let name = case["name"].as_str().unwrap();
            fs::write(path.join(format!("{name}.ll")), &source).unwrap();
            fs::write(path.join(format!("{name}.opt.ll")), &compiled.optimized_ir).unwrap();
        }
        assert_eq!(actual, expected, "{}", case["name"]);
    }
}

#[test]
fn original_image_reduction_axes() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-image-packed.json")).unwrap();
    for case in fixture["source_schedule_observations"].as_array().unwrap() {
        let parts = case["name"]
            .as_str()
            .unwrap()
            .split('-')
            .collect::<Vec<_>>();
        let channels = parts[2].parse().unwrap();
        let image = Image1x1 {
            channels_in: channels,
            channels_out: channels,
            width: parts[3].parse().unwrap(),
        };
        let layout = image.layout().unwrap();
        let before = &case["before"];
        let shape = before["shape"].as_array().unwrap();
        let axes = before["axis_types"].as_array().unwrap();
        let expected = shape
            .iter()
            .zip(axes)
            .filter(|(_, axis)| *axis == "AxisType.REDUCE")
            .map(|(n, _)| n.as_str().unwrap().parse::<usize>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(layout.reduction_axes, expected, "{}", case["name"]);
        assert_eq!(
            layout.unroll,
            case["after"]["upcast_size"].as_u64().unwrap() as usize
        );
    }
}
