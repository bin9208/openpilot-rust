//! #482 실제 역사 object와 frozen BINARY의 비실행 재구성 검사.
#![cfg(all(target_os = "linux", feature = "native-skip-miri"))]
use openpilot_model_runtime::cached_elf;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const OBJECT_SHA: &str = "bf56a30c37f0128497b626329df7239dd7a1c4b5dd9ec728d501f2c11deebea1";
const IMAGE_SHA: &str = "324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767";
const IR_SHA: &str = "e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0";
type Identity = (u64, u64, u32, u32, u32, u64, i64, i64, i64, i64);
fn identity(m: &fs::Metadata) -> Identity {
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.uid(),
        m.gid(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Held {
    path: PathBuf,
    file: File,
    before: Identity,
    bytes: Vec<u8>,
}
impl Held {
    fn open(path: PathBuf, limit: u64) -> Self {
        assert!(path.is_absolute());
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        let metadata = file.metadata().unwrap();
        assert!(metadata.is_file() && metadata.len() <= limit);
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        let before = identity(&metadata);
        assert_eq!(before, identity(&fs::symlink_metadata(&path).unwrap()));
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes.len() as u64, metadata.len());
        let held = Self {
            path,
            file,
            before,
            bytes,
        };
        held.verify_identity();
        held
    }
    fn verify_identity(&self) {
        assert_eq!(self.before, identity(&self.file.metadata().unwrap()));
        assert_eq!(
            self.before,
            identity(&fs::symlink_metadata(&self.path).unwrap())
        );
    }
    fn verify_after(&mut self) {
        self.verify_identity();
        self.file.seek(SeekFrom::Start(0)).unwrap();
        let mut after = Vec::new();
        Read::by_ref(&mut self.file)
            .take(self.bytes.len() as u64 + 1)
            .read_to_end(&mut after)
            .unwrap();
        assert_eq!(self.bytes, after, "held input/executable changed");
        self.verify_identity();
    }
}
#[test]
#[ignore = "등록 Actions의 소유·원문 hash 고정 역사 object/BINARY/IR가 필요; 실행·컴파일 oracle 아님"]
fn historical_object_reconstructs_the_entire_frozen_binary_without_execution() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").unwrap(), "true");
    assert_eq!(
        std::env::var("GITHUB_REPOSITORY").unwrap(),
        "bin9208/openpilot-rust"
    );
    assert_eq!(
        std::env::var("RUNNER_ENVIRONMENT").unwrap(),
        "github-hosted"
    );
    assert_eq!(std::env::var("RUNNER_OS").unwrap(), "Linux");
    assert_eq!(std::env::var("RUNNER_ARCH").unwrap(), "X64");
    assert_eq!(std::env::consts::OS, "linux");
    assert_eq!(std::env::consts::ARCH, "x86_64");
    let proof = PathBuf::from(std::env::var_os("HISTORICAL_IMAGE_PROOF").unwrap());
    assert!(proof.is_absolute() && proof.canonicalize().unwrap() == proof);
    let metadata = fs::symlink_metadata(&proof).unwrap();
    assert!(metadata.is_dir() && metadata.mode() & 0o077 == 0);
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    let mut object = Held::open(proof.join("history92.o"), 1024 * 1024);
    let mut image = Held::open(proof.join("history92.frozen.bin"), 1024 * 1024);
    let mut ir = Held::open(proof.join("history92.ll"), 1024 * 1024);
    let mut executable = Held::open(std::env::current_exe().unwrap(), 128 * 1024 * 1024);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let mut source = Held::open(
        root.join("rust/crates/model-runtime/src/cached_elf.rs"),
        1024 * 1024,
    );
    assert_eq!(object.bytes.len(), 15928);
    assert_eq!(digest(&object.bytes), OBJECT_SHA);
    assert_eq!(image.bytes.len(), 14442);
    assert_eq!(digest(&image.bytes), IMAGE_SHA);
    assert_eq!(ir.bytes.len(), 166049);
    assert_eq!(digest(&ir.bytes), IR_SHA);
    // 기존 native parser만 사용한다. compiler/JIT/dlopen/실행 mmap은 호출하지 않는다.
    let reconstructed = cached_elf::reconstruct(&object.bytes).unwrap();
    let mismatch = reconstructed
        .image
        .iter()
        .zip(&image.bytes)
        .position(|(a, b)| a != b)
        .or_else(|| {
            (reconstructed.image.len() != image.bytes.len())
                .then_some(reconstructed.image.len().min(image.bytes.len()))
        });
    let equal = reconstructed.image == image.bytes;
    for held in [
        &mut object,
        &mut image,
        &mut ir,
        &mut executable,
        &mut source,
    ] {
        held.verify_after();
    }
    let receipt = json!({
        "schema":"openpilot.historical-cached-image.readonly.v1",
        "source_commit":std::env::var("GITHUB_SHA").unwrap(),
        "historical_artifact":12365522894_u64,"historical_source":"e25a14386582858dae25c6daca5ced3bfb4bacab",
        "object_sha256":OBJECT_SHA,"source_ir_sha256":IR_SHA,"frozen_image_sha256":IMAGE_SHA,
        "reconstructed_image_sha256":digest(&reconstructed.image),"reconstructed_bytes":reconstructed.image.len(),
        "frozen_bytes":image.bytes.len(),"text_bytes":reconstructed.text_bytes,"alignment":reconstructed.alignment,
        "entire_image_equal":equal,"first_mismatch_byte":mismatch,
        "native_test_elf":{"sha256":digest(&executable.bytes),"identity":executable.before},
        "cached_elf_source_sha256":digest(&source.bytes),"held_inputs_and_sources_unchanged":true,
        "generated_code_or_model_executions":0,"compiler_calls":0,
        "historical_first_compiler_object_proved":false,"isa_admission":false,"numerical_acceptance":false,
        "full_zip_digest_independently_verified":false
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(proof.join("reconstruction.json"))
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &receipt).unwrap();
    file.write_all(b"\n").unwrap();
    println!("HISTORICAL_IMAGE_RECONSTRUCTION_JSON {receipt}");
    assert!(
        equal,
        "historical object does not reconstruct the entire frozen BINARY; see readonly receipt"
    );
}

#[cfg(all(feature = "cached-program-isa", target_arch = "x86_64"))]
#[test]
#[ignore = "등록 Actions의 역사 triplet/binding과 실제 host ISA가 필요; decoder만 호출하며 code/model은 실행하지 않음"]
fn historical_program_triplet_and_current_host_isa_are_admitted_without_execution() {
    use openpilot_model_runtime::{cached_code, cached_isa};

    const BINDING_SHA: &str = "e9353a5e24b982c1cbe45f9e02bb479a867273115971da9104d03f7d06e78728";
    assert_eq!(std::env::var("GITHUB_ACTIONS").unwrap(), "true");
    assert_eq!(
        std::env::var("GITHUB_REPOSITORY").unwrap(),
        "bin9208/openpilot-rust"
    );
    assert_eq!(
        std::env::var("RUNNER_ENVIRONMENT").unwrap(),
        "github-hosted"
    );
    assert_eq!(std::env::var("RUNNER_OS").unwrap(), "Linux");
    assert_eq!(std::env::var("RUNNER_ARCH").unwrap(), "X64");
    assert_eq!(std::env::consts::OS, "linux");
    assert_eq!(std::env::consts::ARCH, "x86_64");
    let proof = PathBuf::from(std::env::var_os("HISTORICAL_IMAGE_PROOF").unwrap());
    assert!(proof.is_absolute() && proof.canonicalize().unwrap() == proof);
    let metadata = fs::symlink_metadata(&proof).unwrap();
    assert!(metadata.is_dir() && metadata.mode() & 0o077 == 0);
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    let mut object = Held::open(proof.join("history92.o"), 1024 * 1024);
    let mut image = Held::open(proof.join("history92.frozen.bin"), 1024 * 1024);
    let mut ir = Held::open(proof.join("history92.ll"), 1024 * 1024);
    let mut binding = Held::open(proof.join("history92-binding.json"), 64 * 1024);
    let mut executable = Held::open(std::env::current_exe().unwrap(), 128 * 1024 * 1024);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    // Compile 시 읽은 원문 bytes와 현재 held source를 직접 연결한다. 새 expected를 생성하지 않는다.
    let source_pins: [(&str, &[u8]); 4] = [
        (
            "rust/crates/model-runtime/src/cached_elf.rs",
            include_bytes!("../src/cached_elf.rs"),
        ),
        (
            "rust/crates/model-runtime/src/cached_isa.rs",
            include_bytes!("../src/cached_isa.rs"),
        ),
        (
            "rust/crates/model-runtime/src/cached_code.rs",
            include_bytes!("../src/cached_code.rs"),
        ),
        (
            "rust/crates/model-runtime/tests/historical_cached_image.rs",
            include_bytes!("historical_cached_image.rs"),
        ),
    ];
    let mut sources: Vec<_> = source_pins
        .iter()
        .map(|(path, expected)| {
            let held = Held::open(root.join(path), 1024 * 1024);
            assert_eq!(
                held.bytes.as_slice(),
                *expected,
                "compiled/current source differs: {path}"
            );
            held
        })
        .collect();
    assert_eq!(object.bytes.len(), 15928);
    assert_eq!(digest(&object.bytes), OBJECT_SHA);
    assert_eq!(image.bytes.len(), 14442);
    assert_eq!(digest(&image.bytes), IMAGE_SHA);
    assert_eq!(ir.bytes.len(), 166049);
    assert_eq!(digest(&ir.bytes), IR_SHA);
    assert_eq!(binding.bytes.len(), 967);
    assert_eq!(digest(&binding.bytes), BINDING_SHA);
    assert!(executable.bytes.starts_with(b"\x7fELF\x02\x01\x01"));
    assert_eq!(
        u16::from_le_bytes(executable.bytes[18..20].try_into().unwrap()),
        62
    );
    let metadata: serde_json::Value = serde_json::from_slice(&binding.bytes).unwrap();
    assert_eq!(
        metadata["pkl_sha256"],
        "487ef8698775a593726c79ef0651940480f755f5ded57f672d0ee3a4bd8a624f"
    );
    assert_eq!(
        metadata["metadata_sha256"],
        "8d86fabaa6e1d89f2f69028a1cf158be512cace51183f13c719d0434d9056026"
    );
    assert_eq!(metadata["ir_sha256"], IR_SHA);
    assert_eq!(metadata["code_sha256"], IMAGE_SHA);
    assert_eq!(metadata["code_bytes"], 14442);
    assert_eq!(metadata["globals"], json!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]));
    assert_eq!(metadata["outs"], json!([0]));
    assert_eq!(metadata["ins"], json!([1, 2, 3, 4, 5, 6, 7, 8, 9]));
    assert_eq!(metadata["workers"], 2);
    assert_eq!(metadata["scalar"], "core_id");
    assert_eq!(metadata["scalar_slot"], 0);
    assert_eq!(metadata["direct_abi"], "10 pointers then i32");
    assert_eq!(metadata["project_imports_executed"], false);
    assert_eq!(metadata["pickle_globals_invoked"], false);
    let original = cached_code::OriginalProgram {
        source_sha256: IR_SHA.into(),
        image_sha256: IMAGE_SHA.into(),
        reconstruction_object_sha256: OBJECT_SHA.into(),
        image_bytes: 14442,
        image_alignment: 16,
        text_bytes: 14244,
        pointers: 10,
        scalars: 1,
        global_slots: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        workers: 2,
        core_id: Some(0),
    };
    // 기존의 엄격한 triplet/선언 검증을 먼저 수행한다. 실패하면 decoder도 호출하지 않는다.
    let triplet =
        cached_code::verify_original_program(&original, &ir.bytes, &image.bytes, &object.bytes);
    let host = cached_isa::current_host();
    // image 전체를 고정하고 text prefix만 decode한다. padding/constants tail은 decode하지 않는다.
    let isa = if triplet.is_ok() {
        cached_isa::admit_all(&[(image.bytes.as_slice(), original.text_bytes)], 1, &host)
    } else {
        Err("ISA admission withheld after strict triplet rejection".into())
    };
    for held in [
        &mut object,
        &mut image,
        &mut ir,
        &mut binding,
        &mut executable,
    ] {
        held.verify_after();
    }
    for held in &mut sources {
        held.verify_after();
    }
    let isa_proofs: Vec<_> = isa
        .as_ref()
        .map(|proofs| {
            proofs
                .iter()
                .map(|item| {
                    json!({
                        "text_bytes":item.text_bytes,"instructions":item.instructions,
                        "required":item.required
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let source_receipts: Vec<_> = source_pins
        .iter()
        .zip(&sources)
        .map(|((path, _), held)| {
            json!({"path":path,"sha256":digest(&held.bytes),"identity":held.before})
        })
        .collect();
    let receipt = json!({
        "schema":"openpilot.historical-cached-isa.readonly.v1",
        "source_commit":std::env::var("GITHUB_SHA").unwrap(),
        "historical_artifact":12365522894_u64,"historical_source":"e25a14386582858dae25c6daca5ced3bfb4bacab",
        "object_sha256":OBJECT_SHA,"source_ir_sha256":IR_SHA,"frozen_image_sha256":IMAGE_SHA,
        "program_binding_sha256":BINDING_SHA,"program_binding":metadata,
        "original_program":original,"whole_image_bytes":image.bytes.len(),
        "strict_triplet_verified":triplet.is_ok(),"triplet_error":triplet.as_ref().err(),
        "isa_admission":isa.is_ok(),"isa_error":isa.as_ref().err(),
        "host_features":host,"isa_programs":isa_proofs,
        "native_test_elf":{"sha256":digest(&executable.bytes),"identity":executable.before},
        "current_sources":source_receipts,"held_inputs_and_sources_unchanged":true,
        "generated_code_or_model_executions":0,"compiler_calls":0,"original_python_executions":0,
        "historical_first_compiler_object_proved":false,"ABI_or_numeric_acceptance":false,
        "numerical_acceptance":false,"whole_model_program_roster_accepted":false,
        "full_zip_digest_independently_verified":false
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(proof.join("isa-admission.json"))
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &receipt).unwrap();
    file.write_all(b"\n").unwrap();
    println!("HISTORICAL_IMAGE_ISA_JSON {receipt}");
    triplet.expect("historical triplet rejected; see readonly receipt");
    let isa = isa.expect("historical host ISA rejected; see readonly receipt");
    assert_eq!(isa.len(), 1);
    assert_eq!(isa[0].text_bytes, 14244);
    assert!(isa[0].instructions > 0);
}
