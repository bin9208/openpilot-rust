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
    assert_eq!(std::env::var("RUNNER_NAME").unwrap(), "bin-actions");
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
