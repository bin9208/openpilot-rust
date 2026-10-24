//! Explicit admission for an authenticated cached-code candidate. Disabled by
//! `CpuModel::load`; callers must supply independently admitted original/current
//! identities. Checksums bind evidence, they do not authenticate its issuer.
use crate::{cached_elf, graph::Graph};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

pub const GUARDED_ADD_SOURCES: [&str; 8] = [
    "rust/crates/model-runtime/src/guarded_add.rs",
    "rust/crates/model-runtime/src/bin/offline-llvm-guarded-add.rs",
    "rust/crates/model-runtime/src/owned_outputs.rs",
    "rust/crates/model-runtime/src/lib.rs",
    "rust/crates/model-runtime/Cargo.toml",
    "rust/Cargo.toml",
    "rust/Cargo.lock",
    "rust/rust-toolchain.toml",
];
pub const CACHED_SOURCES: [&str; 10] = [
    "rust/crates/model-runtime/src/cached_code.rs",
    "rust/crates/model-runtime/src/bin/offline-cached-code.rs",
    "rust/crates/model-runtime/src/cached_elf.rs",
    "rust/crates/model-runtime/src/cached_isa.rs",
    "rust/crates/model-runtime/src/cpu.rs",
    "rust/crates/model-runtime/src/graph.rs",
    "rust/crates/model-runtime/src/error.rs",
    "rust/crates/model-runtime/src/entrypoint.rs",
    "rust/crates/model-runtime/src/assets.rs",
    "rust/crates/model-runtime/src/buffer.rs",
];

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSeal {
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentBinding {
    pub helper_sha256: String,
    /// Full held-helper receipt (absolute path, bytes, SHA and inode identity),
    /// independently captured by the current-catalog issuer.
    pub helper_binding_sha256: String,
    pub cached_helper_sha256: String,
    pub cached_helper_binding_sha256: String,
    /// Exact existing eight-source guarded-add closure; never replaced by old
    /// source hashes or silently expanded without explicit source admission.
    pub guarded_add_sources: Vec<SourceSeal>,
    pub cached_sources: Vec<SourceSeal>,
    pub exporter_roster_sha256: String,
    pub isa_verifier_sha256: String,
    pub isa_verifier_source_sha256: String,
    pub isa_receipt_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalProgram {
    pub source_sha256: String,
    pub image_sha256: String,
    pub reconstruction_object_sha256: String,
    pub image_bytes: usize,
    pub image_alignment: usize,
    pub text_bytes: usize,
    pub pointers: usize,
    pub scalars: usize,
    pub global_slots: Vec<usize>,
    pub workers: u16,
    pub core_id: Option<usize>,
}
/// Trusted expectations come from original/source admission and the held current
/// issuer, never from the candidate graph/certificate itself. No environment flag
/// or constructor reads these values out of a model to enable cached execution.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    pub original_manifest_sha256: String,
    pub original_model_sha256: String,
    pub current: CurrentBinding,
    pub programs: Vec<OriginalProgram>,
}

impl Admission {
    /// Explicit caller opt-in. `expected_sha256` must be authenticated separately
    /// from the candidate model or sidecar; accepting its self-reported digest
    /// does not establish the original/current issuer trust boundary.
    pub fn read_authenticated(path: &Path, expected_sha256: &str) -> Result<Self, String> {
        if !hash(expected_sha256) {
            return Err("cached admission requires explicit SHA256".into());
        }
        let (guard, bytes) = hold(path, 4 * 1024 * 1024)?;
        if digest(&bytes) != expected_sha256 {
            return Err("cached admission explicit SHA256 differs".into());
        }
        let admission: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        admission.current.validate()?;
        if !hash(&admission.original_manifest_sha256)
            || !hash(&admission.original_model_sha256)
            || admission.programs.is_empty()
            || admission.programs.len() > 4096
        {
            return Err("cached admission original/complete roster".into());
        }
        guard.verify()?;
        Ok(admission)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub kernel: usize,
    pub original: OriginalProgram,
    pub object_sha256: String,
    pub isa_proof_index: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Certificate {
    pub version: u32,
    pub original_manifest_sha256: String,
    pub original_model_sha256: String,
    pub current: CurrentBinding,
    pub library_sha256: String,
    pub container_sha256: String,
    pub programs: Vec<Program>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IsaReceipt {
    status: String,
    generated_code_executed: bool,
    #[serde(rename = "ABI_or_numeric_acceptance")]
    abi_or_numeric_acceptance: bool,
    host_features: Vec<String>,
    programs: Vec<IsaProgram>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IsaProgram {
    index: usize,
    source_sha256: String,
    image_sha256: String,
    text_bytes: usize,
    instructions: usize,
    required: Vec<String>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn roster(seals: &[SourceSeal], expected: &[&str]) -> bool {
    seals.len() == expected.len()
        && seals.iter().all(|seal| hash(&seal.sha256))
        && seals
            .iter()
            .map(|seal| seal.path.as_str())
            .collect::<BTreeSet<_>>()
            == expected.iter().copied().collect()
}
impl CurrentBinding {
    fn validate(&self) -> Result<(), String> {
        if !roster(&self.guarded_add_sources, &GUARDED_ADD_SOURCES)
            || !roster(&self.cached_sources, &CACHED_SOURCES)
            || [
                &self.helper_sha256,
                &self.helper_binding_sha256,
                &self.cached_helper_sha256,
                &self.cached_helper_binding_sha256,
                &self.exporter_roster_sha256,
                &self.isa_verifier_sha256,
                &self.isa_verifier_source_sha256,
                &self.isa_receipt_sha256,
            ]
            .iter()
            .any(|value| !hash(value))
        {
            return Err("cached current helper/source/ISA binding".into());
        }
        Ok(())
    }
}

/// Closed feature vocabulary; Rust detection includes OS AVX state availability.
/// Unknown features reject even on a host which might implement them. The native
/// ISA issuer must decode all instructions, not infer requirements from a label.
fn host_has(feature: &str) -> bool {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    match feature {
        "cmov" | "multibyte-nop" => true,
        "sse" => std::is_x86_feature_detected!("sse"),
        "sse2" => std::is_x86_feature_detected!("sse2"),
        "sse3" => std::is_x86_feature_detected!("sse3"),
        "ssse3" => std::is_x86_feature_detected!("ssse3"),
        "sse4.1" => std::is_x86_feature_detected!("sse4.1"),
        "sse4.2" => std::is_x86_feature_detected!("sse4.2"),
        "avx" => std::is_x86_feature_detected!("avx"),
        "avx2" => std::is_x86_feature_detected!("avx2"),
        "bmi1" => std::is_x86_feature_detected!("bmi1"),
        "bmi2" => std::is_x86_feature_detected!("bmi2"),
        "fma" => std::is_x86_feature_detected!("fma"),
        "f16c" => std::is_x86_feature_detected!("f16c"),
        _ => false,
    }
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    {
        let _ = feature;
        false
    }
}

fn validate(
    certificate: &Certificate,
    receipt: &IsaReceipt,
    expected: &Admission,
    graph: &Graph,
    library: &[u8],
    container: &[u8],
    supported: impl Fn(&str) -> bool,
) -> Result<(), String> {
    expected.current.validate()?;
    let descriptor = graph
        .cached_code
        .as_ref()
        .ok_or("missing cached graph descriptor")?;
    if graph.backend != "cpu-cached-llvm"
        || graph.version != 3
        || graph.arch != "x86_64"
        || certificate.version != 1
        || certificate.current != expected.current
        || certificate.original_manifest_sha256 != expected.original_manifest_sha256
        || certificate.original_model_sha256 != expected.original_model_sha256
        || !hash(&expected.original_manifest_sha256)
        || !hash(&expected.original_model_sha256)
        || certificate.library_sha256 != graph.library_sha256
        || digest(library) != graph.library_sha256
        || digest(container) != certificate.container_sha256
        || !(receipt.status == "COMPLETE_INERT_ISA_ADMISSION"
            || (receipt.status == "ALL44_INERT_ISA_ADMISSION" && receipt.programs.len() == 44))
        || receipt.generated_code_executed
        || receipt.abi_or_numeric_acceptance
        || receipt.host_features.len() > 128
        || receipt.programs.len() != expected.programs.len()
        || certificate.programs.is_empty()
        || certificate.programs.len() > 4096
        || certificate.programs.len() != expected.programs.len()
        || certificate.programs.len() != descriptor.kernels.len()
    {
        return Err("cached original/current/graph/complete proof identity".into());
    }
    let functions = cached_elf::functions(library, 3)?;
    let container_functions = cached_elf::functions(container, 1)?;
    let symbols: BTreeSet<_> = certificate
        .programs
        .iter()
        .map(|program| format!("op_impl_{}", program.kernel))
        .collect();
    if container_functions.keys().collect::<BTreeSet<_>>() != symbols.iter().collect()
        || functions.keys().collect::<BTreeSet<_>>() != symbols.iter().collect()
        || symbols.len() != certificate.programs.len()
    {
        return Err("cached container/library exact program symbol roster".into());
    }
    let called: BTreeSet<_> = graph.calls.iter().map(|call| call.kernel).collect();
    let mut originals = BTreeSet::new();
    let mut kernels = BTreeSet::new();
    let mut ranges = Vec::new();
    for (position, program) in certificate.programs.iter().enumerate() {
        let original = expected
            .programs
            .get(program.isa_proof_index)
            .ok_or("cached original proof index")?;
        let proof = receipt
            .programs
            .get(program.isa_proof_index)
            .ok_or("cached ISA proof index")?;
        if !originals.insert(program.isa_proof_index)
            || !kernels.insert(program.kernel)
            || descriptor.kernels.get(position) != Some(&program.kernel)
            || &program.original != original
            || proof.index != program.isa_proof_index
            || proof.source_sha256 != original.source_sha256
            || proof.image_sha256 != original.image_sha256
            || proof.text_bytes != original.text_bytes
            || proof.instructions == 0
            || proof.instructions > 1_000_000
            || proof.required.len() > 32
            || !hash(&original.source_sha256)
            || !hash(&original.image_sha256)
            || program.object_sha256 != certificate.container_sha256
            || !hash(&original.reconstruction_object_sha256)
            || !original.image_alignment.is_power_of_two()
            || original.image_alignment > 64
            || original.image_bytes == 0
            || original.image_bytes > 16 * 1024 * 1024
            || original.text_bytes == 0
            || original.text_bytes > original.image_bytes
            || graph.kernels.get(program.kernel).is_none_or(|kernel| {
                kernel.buffers != original.pointers || kernel.scalars != original.scalars
            })
            || original.global_slots.len() != original.pointers
            || original.global_slots.iter().collect::<BTreeSet<_>>().len()
                != original.global_slots.len()
            || original.global_slots.iter().any(|slot| *slot > 1024)
            || !(1..=1024).contains(&original.workers)
            || original
                .core_id
                .is_some_and(|index| index >= original.scalars)
            || graph
                .calls
                .iter()
                .filter(|call| call.kernel == program.kernel)
                .any(|call| call.workers != original.workers || call.core_id != original.core_id)
            || !called.contains(&program.kernel)
        {
            return Err("cached PROGRAM source/image/ABI/complete roster".into());
        }
        let requirements: BTreeSet<_> = proof.required.iter().collect();
        if requirements.len() != proof.required.len()
            || requirements.iter().any(|feature| !supported(feature))
        {
            return Err("cached PROGRAM unknown or unavailable host ISA".into());
        }
        let name = format!("op_impl_{}", program.kernel);
        let function = functions.get(&name).ok_or("missing cached image symbol")?;
        let packaged = container_functions
            .get(&name)
            .ok_or("missing cached container symbol")?;
        if !function
            .address
            .is_multiple_of(original.image_alignment as u64)
            || !packaged
                .address
                .is_multiple_of(original.image_alignment as u64)
            || packaged.bytes != function.bytes
            || function.bytes.len() != original.image_bytes
            || digest(function.bytes) != original.image_sha256
        {
            return Err("cached linked full image differs".into());
        }
        let end = function
            .address
            .checked_add(function.bytes.len() as u64)
            .ok_or("cached symbol range overflow")?;
        if ranges
            .iter()
            .any(|&(start, stop)| function.address < stop && start < end)
        {
            return Err("cached images alias".into());
        }
        ranges.push((function.address, end));
    }
    Ok(())
}

// The catalog admits at most 128 MiB across its original model roster, so no
// individual original can exceed this bound. Source/IR/output limits are separate.
const MAX_ORIGINAL_MODEL_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) struct HeldLibrary {
    file: File,
    path: PathBuf,
    identity: (u64, u64, u64, i64, i64, i64, i64, u32),
    digest: String,
}
fn identity(metadata: &std::fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64, u32) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
        metadata.mode(),
    )
}
fn read(path: &Path, maximum: usize) -> Result<(File, Vec<u8>), String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        return Err("cached asset regular-file/size guard".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() != metadata.len() as usize {
        return Err("cached asset changed length".into());
    }
    Ok((file, bytes))
}
fn streamed_digest(file: &File, length: u64) -> Result<String, String> {
    use std::os::unix::fs::FileExt;
    let mut hash = Sha256::new();
    let mut chunk = [0; 65536];
    let mut offset = 0;
    while offset < length {
        let count = (length - offset).min(chunk.len() as u64) as usize;
        file.read_exact_at(&mut chunk[..count], offset)
            .map_err(|error| error.to_string())?;
        hash.update(&chunk[..count]);
        offset += count as u64;
    }
    if file
        .read_at(&mut [0; 1], length)
        .map_err(|error| error.to_string())?
        != 0
    {
        return Err("cached asset changed length".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}
impl HeldLibrary {
    pub(crate) fn descriptor_path(&self) -> String {
        format!("/proc/self/fd/{}", self.file.as_raw_fd())
    }
    fn verify_identity(&self) -> Result<(), String> {
        let current = std::fs::symlink_metadata(&self.path).map_err(|error| error.to_string())?;
        let held = self.file.metadata().map_err(|error| error.to_string())?;
        if !current.is_file()
            || identity(&current) != self.identity
            || identity(&held) != self.identity
        {
            return Err("cached library identity changed".into());
        }
        Ok(())
    }
    pub(crate) fn verify(&self) -> Result<(), String> {
        self.verify_identity()?;
        if streamed_digest(&self.file, self.identity.2)? != self.digest {
            return Err("cached library bytes changed".into());
        }
        self.verify_identity()
    }
}
/// Read and verify all evidence before dlopen; retain the native library FD for
/// the model lifetime. The caller's ordinary immutable/trusted-kernel contract
/// still applies, as with fresh artifacts.
pub(crate) fn prepare(
    directory: &Path,
    graph: &Graph,
    expected: &Admission,
) -> Result<HeldLibrary, String> {
    if !cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        return Err("cached CPU mode requires x86_64 Linux".into());
    }
    let descriptor = graph
        .cached_code
        .as_ref()
        .ok_or("cached certificate is absent")?;
    let (_, bytes) = read(&directory.join("cached-code.json"), 4 * 1024 * 1024)?;
    if digest(&bytes) != descriptor.certificate_sha256 {
        return Err("cached certificate graph binding".into());
    }
    let certificate: Certificate =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let (_, proof) = read(&directory.join("cached-isa.json"), 4 * 1024 * 1024)?;
    if digest(&proof) != expected.current.isa_receipt_sha256 {
        return Err("cached ISA receipt current binding".into());
    }
    let receipt: IsaReceipt = serde_json::from_slice(&proof).map_err(|error| error.to_string())?;
    let path = std::fs::canonicalize(directory)
        .map_err(|error| error.to_string())?
        .join("kernels.so");
    let (file, library) = read(&path, 512 * 1024 * 1024)?;
    let (_, container) = read(&directory.join("cached-container.o"), 64 * 1024 * 1024)?;
    validate(
        &certificate,
        &receipt,
        expected,
        graph,
        &library,
        &container,
        host_has,
    )?;
    let guard = HeldLibrary {
        identity: identity(&file.metadata().map_err(|error| error.to_string())?),
        file,
        path,
        digest: graph.library_sha256.clone(),
    };
    guard.verify()?;
    Ok(guard)
}

pub struct Container {
    pub object: Vec<u8>,
    pub programs: Vec<Program>,
}
/// Build one complete native object from explicitly admitted original images.
/// ISA/ABI/issuer checks are still required before any resulting library loads.
/// Each input corresponds to one independent original PROGRAM, in original proof
/// order; it is never deduplicated by SOURCE alone or selected by a numeric mask.
pub fn package_programs(
    images: &[(usize, &[u8])],
    expected: &Admission,
) -> Result<Container, String> {
    expected.current.validate()?;
    if images.is_empty() || images.len() > 4096 || images.len() != expected.programs.len() {
        return Err("cached original/container complete roster".into());
    }
    for ((_, image), original) in images.iter().zip(&expected.programs) {
        if image.len() > 16 * 1024 * 1024
            || image.len() != original.image_bytes
            || digest(image) != original.image_sha256
            || !hash(&original.source_sha256)
            || !hash(&original.reconstruction_object_sha256)
            || !original.image_alignment.is_power_of_two()
            || original.image_alignment > 64
            || original.text_bytes == 0
            || original.text_bytes > image.len()
            || original.pointers > 1024
            || original.scalars > 1024
        {
            return Err("cached original/container image identity".into());
        }
    }
    let object = cached_elf::package(images)?;
    let object_sha256 = digest(&object);
    let programs = images
        .iter()
        .zip(&expected.programs)
        .enumerate()
        .map(|(index, ((kernel, _), original))| Program {
            kernel: *kernel,
            original: original.clone(),
            object_sha256: object_sha256.clone(),
            isa_proof_index: index,
        })
        .collect();
    Ok(Container { object, programs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{CachedCode, Call, Kernel};
    fn binding() -> CurrentBinding {
        let sources = |paths: &[&str]| {
            paths
                .iter()
                .map(|path| SourceSeal {
                    path: (*path).into(),
                    sha256: "1".repeat(64),
                })
                .collect()
        };
        CurrentBinding {
            helper_sha256: "2".repeat(64),
            helper_binding_sha256: "7".repeat(64),
            cached_helper_sha256: "8".repeat(64),
            cached_helper_binding_sha256: "9".repeat(64),
            guarded_add_sources: sources(&GUARDED_ADD_SOURCES),
            cached_sources: sources(&CACHED_SOURCES),
            exporter_roster_sha256: "3".repeat(64),
            isa_verifier_sha256: "4".repeat(64),
            isa_verifier_source_sha256: "5".repeat(64),
            isa_receipt_sha256: "6".repeat(64),
        }
    }
    fn fixture() -> (Admission, Certificate, IsaReceipt, Graph, Vec<u8>, Vec<u8>) {
        let image = [0xc3, 0xff, 0x62];
        let original = OriginalProgram {
            source_sha256: digest(b"source"),
            image_sha256: digest(&image),
            reconstruction_object_sha256: digest(b"original-object"),
            image_bytes: image.len(),
            image_alignment: 64,
            text_bytes: 1,
            pointers: 0,
            scalars: 0,
            global_slots: vec![],
            workers: 1,
            core_id: None,
        };
        let expected = Admission {
            original_manifest_sha256: digest(b"original-manifest"),
            original_model_sha256: digest(b"original-model"),
            current: binding(),
            programs: vec![original],
        };
        let packaged = package_programs(&[(0, &image)], &expected).unwrap();
        // Inert ET_DYN symbol-layout fixture only; no dlopen or code execution.
        let mut library = packaged.object.clone();
        library[16..18].copy_from_slice(&3_u16.to_le_bytes());
        let certificate = Certificate {
            version: 1,
            original_manifest_sha256: expected.original_manifest_sha256.clone(),
            original_model_sha256: expected.original_model_sha256.clone(),
            current: expected.current.clone(),
            library_sha256: digest(&library),
            container_sha256: digest(&packaged.object),
            programs: packaged.programs,
        };
        let receipt = IsaReceipt {
            status: "COMPLETE_INERT_ISA_ADMISSION".into(),
            generated_code_executed: false,
            abi_or_numeric_acceptance: false,
            host_features: vec![],
            programs: vec![IsaProgram {
                index: 0,
                source_sha256: expected.programs[0].source_sha256.clone(),
                image_sha256: expected.programs[0].image_sha256.clone(),
                text_bytes: 1,
                instructions: 1,
                required: vec![],
            }],
        };
        let graph = Graph {
            version: 3,
            backend: "cpu-cached-llvm".into(),
            arch: "x86_64".into(),
            weights_sha256: digest(b""),
            library_sha256: digest(&library),
            allocations: vec![],
            views: vec![],
            inputs: vec![],
            outputs: vec![],
            kernels: vec![Kernel {
                buffers: 0,
                scalars: 0,
            }],
            calls: vec![Call {
                kernel: 0,
                views: vec![],
                scalars: vec![],
                workers: 1,
                core_id: None,
            }],
            entrypoints: vec![],
            cached_code: Some(CachedCode {
                certificate_sha256: digest(b"certificate"),
                kernels: vec![0],
            }),
        };
        (
            expected,
            certificate,
            receipt,
            graph,
            library,
            packaged.object,
        )
    }
    #[test]
    fn exact_original_current_full_image_and_isa_certificate_is_required() {
        let (expected, mut certificate, receipt, graph, library, container) = fixture();
        validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true,
        )
        .unwrap();
        certificate.original_manifest_sha256 = digest(b"replaced-original");
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true
        )
        .is_err());
    }
    #[test]
    fn mutated_exporter_and_stale_helper_binding_reject() {
        let (expected, mut certificate, receipt, graph, library, container) = fixture();
        certificate.current.cached_sources[0].sha256 = digest(b"mutated-exporter");
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true
        )
        .is_err());
        certificate.current = expected.current.clone();
        certificate.current.helper_sha256 = digest(b"stale-helper");
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true
        )
        .is_err());
        certificate.current = expected.current.clone();
        certificate.current.guarded_add_sources.pop();
        assert!(certificate.current.validate().is_err());
    }
    #[test]
    fn current_binding_requires_the_extracted_isa_source() {
        const ISA_SOURCE: &str = "rust/crates/model-runtime/src/cached_isa.rs";
        let (expected, certificate, receipt, graph, library, container) = fixture();
        let check = |candidate: &Certificate, admitted: &Admission| {
            validate(
                candidate,
                &receipt,
                admitted,
                &graph,
                &library,
                &container,
                |_| true,
            )
        };
        check(&certificate, &expected).unwrap();
        // fixture가 production constant에서 파생되어도 이 별도 필수 경로를 빠뜨릴 수 없다.
        let index = expected
            .current
            .cached_sources
            .iter()
            .position(|seal| seal.path == ISA_SOURCE)
            .expect("current binding must seal the extracted ISA source");

        // 옛 9경로 receipt끼리 일치해도 현재의 정확한 roster gate를 통과하지 못한다.
        let mut old_admission = expected.clone();
        old_admission.current.cached_sources.remove(index);
        let mut old_certificate = certificate.clone();
        old_certificate.current = old_admission.current.clone();
        assert_eq!(
            check(&old_certificate, &old_admission).unwrap_err(),
            "cached current helper/source/ISA binding"
        );

        // 독립적으로 수용한 current hash는 그대로 두고 검증서의 ISA source seal만 변조한다.
        let mut changed_certificate = certificate.clone();
        changed_certificate.current.cached_sources[index].sha256 = digest(b"changed ISA source");
        assert_eq!(
            check(&changed_certificate, &expected).unwrap_err(),
            "cached original/current/graph/complete proof identity"
        );

        // 개수만 맞추고 다른 경로로 바꾼 admission/검증서도 모두 거부해야 한다.
        let mut substituted_admission = expected.clone();
        substituted_admission.current.cached_sources[index].path =
            "rust/crates/model-runtime/src/unreviewed_isa.rs".into();
        let mut substituted_certificate = certificate;
        substituted_certificate.current = substituted_admission.current.clone();
        assert_eq!(
            check(&substituted_certificate, &substituted_admission).unwrap_err(),
            "cached current helper/source/ISA binding"
        );
    }
    #[test]
    fn omitted_final_program_abi_change_and_unknown_isa_reject() {
        let (expected, mut certificate, mut receipt, graph, library, container) = fixture();
        certificate.programs.clear();
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true
        )
        .is_err());
        let (_, mut certificate, _, _, _, _) = fixture();
        certificate.programs[0].original.scalars = 1;
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true
        )
        .is_err());
        let (_, certificate, _, _, _, _) = fixture();
        receipt.programs[0]
            .required
            .push("unadmitted-future-extension".into());
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            host_has
        )
        .is_err());
        receipt.programs[0].required = vec!["avx2".into()];
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| false
        )
        .is_err());
    }
    #[test]
    fn changed_constant_tail_or_container_cannot_hide_behind_matching_text() {
        let (expected, certificate, receipt, graph, mut library, mut container) = fixture();
        let function = cached_elf::functions(&library, 3)
            .unwrap()
            .remove("op_impl_0")
            .unwrap();
        let start = function.bytes.as_ptr() as usize - library.as_ptr() as usize;
        library[start + 1] ^= 1;
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true
        )
        .is_err());
        let (_, _, _, _, library, _) = fixture();
        container[start + 2] ^= 1;
        assert!(validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true
        )
        .is_err());
    }
    #[test]
    fn extra_linked_image_cannot_hide_outside_the_certificate_or_graph() {
        let (expected, mut certificate, receipt, mut graph, _, container) = fixture();
        let mut library =
            cached_elf::package(&[(0, &[0xc3, 0xff, 0x62]), (1, &[0xc3, 0x80])]).unwrap();
        library[16..18].copy_from_slice(&3_u16.to_le_bytes());
        // Rebind the outer library hash so this tests exact symbol coverage,
        // rather than merely detecting a stale whole-file checksum.
        graph.library_sha256 = digest(&library);
        certificate.library_sha256 = graph.library_sha256.clone();
        let error = validate(
            &certificate,
            &receipt,
            &expected,
            &graph,
            &library,
            &container,
            |_| true,
        )
        .unwrap_err();
        assert!(error.contains("exact program symbol roster"));
    }

    #[test]
    fn authentic_triplet_still_requires_original_argument_order_and_launch_abi() {
        let image = [0xc3];
        let object = cached_elf::package(&[(0, &image)]).unwrap();
        let source = b"define void @original(float* noalias align 32 %data2, i8* noalias align 16 %data0, i32 %data1) #0\n{\nret void\n}\n";
        let mut original = OriginalProgram {
            source_sha256: digest(source),
            image_sha256: digest(&image),
            reconstruction_object_sha256: digest(&object),
            image_bytes: 1,
            image_alignment: 64,
            text_bytes: 1,
            pointers: 2,
            scalars: 1,
            global_slots: vec![2, 0],
            workers: 4,
            core_id: Some(0),
        };
        verify_original_program(&original, source, &image, &object).unwrap();
        original.global_slots.swap(0, 1);
        assert!(verify_original_program(&original, source, &image, &object)
            .unwrap_err()
            .contains("global argument order"));
        original.global_slots.swap(0, 1);
        original.core_id = None;
        assert!(verify_original_program(&original, source, &image, &object)
            .unwrap_err()
            .contains("launch contract"));
        original.core_id = Some(0);
        let changed = String::from_utf8(source.to_vec())
            .unwrap()
            .replace("define void", "define fastcc void");
        // Rebind the authenticated source hash: C-ABI rejection must not depend
        // merely on a stale checksum or on executing the supplied image.
        original.source_sha256 = digest(changed.as_bytes());
        assert!(
            verify_original_program(&original, changed.as_bytes(), &image, &object)
                .unwrap_err()
                .contains("function ABI")
        );
    }

    #[test]
    fn source_only_dedup_and_partial_original_roster_are_not_admitted() {
        let (mut expected, _, _, _, _, _) = fixture();
        let original = expected.programs[0].clone();
        expected.programs.push(original);
        assert!(package_programs(&[(0, &[0xc3, 0xff, 0x62])], &expected).is_err());
        expected.programs[1].image_sha256 = digest(&[0xc3, 0x80]);
        expected.programs[1].image_bytes = 2;
        let built =
            package_programs(&[(0, &[0xc3, 0xff, 0x62]), (1, &[0xc3, 0x80])], &expected).unwrap();
        assert_eq!(
            built.programs[0].original.source_sha256,
            built.programs[1].original.source_sha256
        );
        assert_ne!(
            built.programs[0].original.image_sha256,
            built.programs[1].original.image_sha256
        );
        assert_eq!(cached_elf::functions(&built.object, 1).unwrap().len(), 2);
    }

    #[test]
    fn streamed_original_model_exceeds_source_limit_without_moving_fd_offset() {
        use std::io::{Seek, SeekFrom, Write};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original-model.pkl");
        let mut file = File::create(&path).unwrap();
        let mut expected = Sha256::new();
        // 513 distinct 64 KiB chunks exceed the source limit; the final write
        // also exercises a partial chunk. These are inert synthetic bytes.
        for index in 0_u64..513 {
            let mut chunk = [0xa5; 65536];
            chunk[..8].copy_from_slice(&index.to_le_bytes());
            file.write_all(&chunk).unwrap();
            expected.update(chunk);
        }
        file.write_all(b"distinct model tail").unwrap();
        expected.update(b"distinct model tail");
        drop(file);
        assert!(hold(&path, 32 * 1024 * 1024).is_err());
        let mut held = hold_original_model(&path).unwrap();
        assert_eq!(held.digest, format!("{:x}", expected.finalize()));
        assert_eq!(held.file.stream_position().unwrap(), 0);
        held.file.seek(SeekFrom::Start(19)).unwrap();
        held.verify().unwrap();
        assert_eq!(held.file.stream_position().unwrap(), 19);
    }

    #[test]
    fn streamed_original_model_rejects_cap_overflow_and_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("oversized-model.pkl");
        File::create(&path)
            .unwrap()
            .set_len(MAX_ORIGINAL_MODEL_BYTES + 1)
            .unwrap();
        assert!(hold_original_model(&path).is_err());
        let target = directory.path().join("model.pkl");
        std::fs::write(&target, b"inert model").unwrap();
        let alias = directory.path().join("alias.pkl");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert!(hold_original_model(&alias).is_err());
    }

    #[test]
    fn streamed_digest_requires_exact_length_and_eof() {
        use std::io::{Seek, SeekFrom};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("data");
        let mut data = vec![0x3c; 65539];
        data[0] = 0x81;
        data[65536] = 0x62;
        data[65538] = 0xff;
        std::fs::write(&path, &data).unwrap();
        let mut file = File::open(&path).unwrap();
        file.seek(SeekFrom::Start(23)).unwrap();
        assert_eq!(
            streamed_digest(&file, data.len() as u64).unwrap(),
            format!("{:x}", Sha256::digest(&data))
        );
        assert!(streamed_digest(&file, data.len() as u64 - 1).is_err());
        assert!(streamed_digest(&file, data.len() as u64 + 1).is_err());
        assert_eq!(file.stream_position().unwrap(), 23);
    }

    #[test]
    fn streamed_guard_rejects_same_size_mutation_and_truncation() {
        use std::os::unix::fs::FileExt;
        let directory = tempfile::tempdir().unwrap();
        for truncate in [false, true] {
            let path = directory.path().join(format!("model-{truncate}"));
            std::fs::write(&path, b"original model bytes").unwrap();
            let mut held = hold_original_model(&path).unwrap();
            let file = OpenOptions::new().write(true).open(&path).unwrap();
            if truncate {
                file.set_len(held.identity.2 - 1).unwrap();
            } else {
                file.write_all_at(b"X", 9).unwrap();
                assert_eq!(file.metadata().unwrap().len(), held.identity.2);
            }
            assert!(held.verify().is_err());
            if !truncate {
                // Rebase only this test's metadata to isolate the independent
                // content seal: unchanged length/identity cannot replace SHA.
                held.identity = identity(&file.metadata().unwrap());
                assert_eq!(held.verify().unwrap_err(), "cached library bytes changed");
            }
        }
    }

    #[test]
    fn streamed_guard_rejects_replacement_even_with_identical_bytes() {
        let directory = tempfile::tempdir().unwrap();
        for symlink in [false, true] {
            let path = directory.path().join(format!("model-{symlink}"));
            std::fs::write(&path, b"identical model bytes").unwrap();
            let held = hold_original_model(&path).unwrap();
            let retained = directory.path().join(format!("retained-{symlink}"));
            std::fs::rename(&path, &retained).unwrap();
            if symlink {
                std::os::unix::fs::symlink(&retained, &path).unwrap();
            } else {
                std::fs::write(&path, b"identical model bytes").unwrap();
            }
            assert!(held.verify().is_err());
            assert_eq!(
                streamed_digest(&held.file, held.identity.2).unwrap(),
                held.digest
            );
        }
    }
}

/// Authenticate the exact SOURCE/BINARY/object triplet and the ordinary LLVM C
/// parameter ABI before issuing transport evidence. No source recompilation or
/// historic compiler-identity assertion is made by this byte certificate.
pub fn verify_original_program(
    original: &OriginalProgram,
    source: &[u8],
    image: &[u8],
    object: &[u8],
) -> Result<(), String> {
    if source.is_empty()
        || source.len() > 16 * 1024 * 1024
        || digest(source) != original.source_sha256
        || image.len() != original.image_bytes
        || digest(image) != original.image_sha256
        || object.len() > 16 * 1024 * 1024
        || digest(object) != original.reconstruction_object_sha256
    {
        return Err("cached original SOURCE/BINARY/object triplet differs".into());
    }
    let reconstructed = cached_elf::reconstruct(object)?;
    if reconstructed.image != image
        || reconstructed.text_bytes != original.text_bytes
        || reconstructed.alignment > original.image_alignment
        || !original.image_alignment.is_power_of_two()
        || original.image_alignment > 64
    {
        return Err("cached independent complete object reconstruction differs".into());
    }
    let text = std::str::from_utf8(source).map_err(|_| "cached LLVM source is not UTF8")?;
    let header = text
        .lines()
        .next()
        .ok_or("cached LLVM source has no declaration")?;
    let declaration = header
        .strip_prefix("define void @")
        .and_then(|value| value.strip_suffix(") #0"))
        .ok_or("cached LLVM function ABI")?;
    let (name, arguments) = declaration
        .split_once('(')
        .ok_or("cached LLVM function declaration")?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("cached LLVM function name".into());
    }
    let arguments: Vec<_> = if arguments.is_empty() {
        vec![]
    } else {
        arguments.split(", ").collect()
    };
    if arguments.len() != original.pointers + original.scalars
        || original.global_slots.len() != original.pointers
    {
        return Err("cached original pointer/scalar arity".into());
    }
    let mut names = BTreeSet::new();
    for (index, argument) in arguments.iter().enumerate() {
        let fields: Vec<_> = argument.split_whitespace().collect();
        let label = if index < original.pointers {
            if fields.len() != 5
                || !matches!(
                    fields[0],
                    "i8*" | "i16*" | "i32*" | "i64*" | "float*" | "double*"
                )
                || fields[1] != "noalias"
                || fields[2] != "align"
            {
                return Err("cached LLVM pointer calling convention".into());
            }
            let alignment: usize = fields[3]
                .parse()
                .map_err(|_| "cached LLVM pointer alignment")?;
            if !alignment.is_power_of_two() || alignment > 64 {
                return Err("cached LLVM pointer alignment exceeds native buffers".into());
            }
            if fields[4] != format!("%data{}", original.global_slots[index]) {
                return Err("cached original global argument order".into());
            }
            fields[4]
        } else {
            if fields.len() != 2 || fields[0] != "i32" {
                return Err("cached LLVM scalar calling convention".into());
            }
            fields[1]
        };
        if !label.strip_prefix("%data").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        }) || !names.insert(label)
        {
            return Err("cached LLVM parameter identity".into());
        }
    }
    if !(1..=1024).contains(&original.workers)
        || original
            .core_id
            .is_some_and(|index| index >= original.scalars)
        || (original.core_id.is_none() && original.workers != 1)
    {
        return Err("cached original launch contract".into());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramFiles {
    pub original_index: usize,
    pub kernel: usize,
    pub source: PathBuf,
    pub image: PathBuf,
    pub reconstruction_object: PathBuf,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerRequest {
    pub source_root: PathBuf,
    pub original_manifest: PathBuf,
    pub original_model: PathBuf,
    /// An independently authenticated ISA receipt, not requirements copied from
    /// untrusted model metadata. An issuer must also decode the held images.
    pub isa_receipt: PathBuf,
    pub admission: Admission,
    pub programs: Vec<ProgramFiles>,
    /// Both absent means preflight/package only. Both present means final static
    /// seal of the actual grouped library and graph, still without dlopen.
    pub library: Option<PathBuf>,
    pub graph: Option<PathBuf>,
}
#[derive(Debug)]
pub struct InstructionAnalysis {
    pub instructions: usize,
    pub required: BTreeSet<String>,
}
pub struct Issued {
    pub container: Vec<u8>,
    pub isa_receipt: Vec<u8>,
    pub certificate: Option<Vec<u8>>,
    pub original_and_current: Vec<u8>,
}

fn hold(path: &Path, limit: usize) -> Result<(HeldLibrary, Vec<u8>), String> {
    if !path.is_absolute()
        || std::fs::canonicalize(path).map_err(|error| error.to_string())? != path
    {
        return Err("cached issuer input path must be absolute and alias-free".into());
    }
    let (file, bytes) = read(path, limit)?;
    let guard = HeldLibrary {
        identity: identity(&file.metadata().map_err(|error| error.to_string())?),
        file,
        path: path.to_owned(),
        digest: digest(&bytes),
    };
    guard.verify()?;
    Ok((guard, bytes))
}

fn hold_original_model(path: &Path) -> Result<HeldLibrary, String> {
    if !path.is_absolute()
        || std::fs::canonicalize(path).map_err(|error| error.to_string())? != path
    {
        return Err("cached issuer input path must be absolute and alias-free".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_ORIGINAL_MODEL_BYTES {
        return Err("cached asset regular-file/size guard".into());
    }
    let mut guard = HeldLibrary {
        identity: identity(&metadata),
        file,
        path: path.to_owned(),
        digest: String::new(),
    };
    guard.verify_identity()?;
    guard.digest = streamed_digest(&guard.file, guard.identity.2)?;
    guard.verify_identity()?;
    Ok(guard)
}

/// Native packing/sealing interface for the separately pending exporter bridge.
/// The launcher MUST authenticate the complete request hash and the issuer ELF
/// before calling this function. Each original identity is supplied independently
/// by original/source admission. No expected hash is inferred from input bytes.
/// `inspect` is the sealed native decoder; no successful/fresh-compile fallback
/// is permitted when an image or decoder contract is unsupported.
pub fn issue(
    request: &IssuerRequest,
    inspect: impl Fn(&[u8], usize) -> Result<InstructionAnalysis, String>,
) -> Result<Issued, String> {
    let expected = &request.admission;
    expected.current.validate()?;
    if request.programs.is_empty()
        || request.programs.len() > 4096
        || request.programs.len() != expected.programs.len()
        || request.library.is_some() != request.graph.is_some()
        || !request.source_root.is_absolute()
        || std::fs::canonicalize(&request.source_root).map_err(|error| error.to_string())?
            != request.source_root
    {
        return Err("cached issuer request mode/complete roster".into());
    }
    let mut held = Vec::new();
    let (guard, manifest) = hold(&request.original_manifest, 4 * 1024 * 1024)?;
    held.push(guard);
    let model = hold_original_model(&request.original_model)?;
    if digest(&manifest) != expected.original_manifest_sha256
        || model.digest != expected.original_model_sha256
    {
        return Err("cached issuer authentic original manifest/model differs".into());
    }
    held.push(model);
    for seal in expected
        .current
        .guarded_add_sources
        .iter()
        .chain(&expected.current.cached_sources)
    {
        let (guard, bytes) = hold(&request.source_root.join(&seal.path), 32 * 1024 * 1024)?;
        held.push(guard);
        if digest(&bytes) != seal.sha256 {
            return Err("cached issuer current source differs".into());
        }
    }
    let (guard, isa_bytes) = hold(&request.isa_receipt, 4 * 1024 * 1024)?;
    held.push(guard);
    if digest(&isa_bytes) != expected.current.isa_receipt_sha256 {
        return Err("cached issuer ISA provider receipt differs".into());
    }
    let receipt: IsaReceipt =
        serde_json::from_slice(&isa_bytes).map_err(|error| error.to_string())?;
    if receipt.programs.len() != expected.programs.len()
        || receipt.generated_code_executed
        || receipt.abi_or_numeric_acceptance
        || !(receipt.status == "COMPLETE_INERT_ISA_ADMISSION"
            || (receipt.status == "ALL44_INERT_ISA_ADMISSION" && receipt.programs.len() == 44))
    {
        return Err("cached issuer complete inert ISA receipt".into());
    }
    let mut ordered: Vec<Option<(usize, Vec<u8>)>> =
        (0..request.programs.len()).map(|_| None).collect();
    for files in &request.programs {
        let original = expected
            .programs
            .get(files.original_index)
            .ok_or("cached issuer original program index")?;
        if ordered[files.original_index].is_some() {
            return Err("cached issuer duplicate original program".into());
        }
        let (guard, source) = hold(&files.source, 16 * 1024 * 1024)?;
        held.push(guard);
        let (guard, image) = hold(&files.image, 16 * 1024 * 1024)?;
        held.push(guard);
        let (guard, object) = hold(&files.reconstruction_object, 16 * 1024 * 1024)?;
        held.push(guard);
        verify_original_program(original, &source, &image, &object)?;
        let analysis = inspect(&image, original.text_bytes)?;
        let proof = &receipt.programs[files.original_index];
        let required: BTreeSet<_> = proof.required.iter().cloned().collect();
        if proof.index != files.original_index
            || proof.source_sha256 != original.source_sha256
            || proof.image_sha256 != original.image_sha256
            || proof.text_bytes != original.text_bytes
            || proof.instructions != analysis.instructions
            || analysis.instructions == 0
            || proof.required.len() != required.len()
            || required != analysis.required
            || required.iter().any(|feature| !host_has(feature))
        {
            return Err("cached issuer independent complete decode/host proof differs".into());
        }
        ordered[files.original_index] = Some((files.kernel, image));
    }
    let ordered: Vec<_> = ordered
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("cached issuer missing original program")?;
    let images: Vec<_> = ordered
        .iter()
        .map(|(kernel, bytes)| (*kernel, bytes.as_slice()))
        .collect();
    let container = package_programs(&images, expected)?;
    let certificate =
        if let (Some(library_path), Some(graph_path)) = (&request.library, &request.graph) {
            let (guard, library) = hold(library_path, 512 * 1024 * 1024)?;
            held.push(guard);
            let (guard, graph) = hold(graph_path, 64 * 1024 * 1024)?;
            held.push(guard);
            let graph = Graph::parse(&graph)
                .map_err(|error| error.to_string())?
                .graph;
            let certificate = Certificate {
                version: 1,
                original_manifest_sha256: expected.original_manifest_sha256.clone(),
                original_model_sha256: expected.original_model_sha256.clone(),
                current: expected.current.clone(),
                library_sha256: digest(&library),
                container_sha256: digest(&container.object),
                programs: container.programs,
            };
            validate(
                &certificate,
                &receipt,
                expected,
                &graph,
                &library,
                &container.object,
                host_has,
            )?;
            Some(serde_json::to_vec_pretty(&certificate).map_err(|error| error.to_string())?)
        } else {
            None
        };
    for guard in &held {
        guard.verify()?;
    }
    Ok(Issued {
        container: container.object,
        isa_receipt: isa_bytes,
        certificate,
        original_and_current: serde_json::to_vec_pretty(expected)
            .map_err(|error| error.to_string())?,
    })
}
