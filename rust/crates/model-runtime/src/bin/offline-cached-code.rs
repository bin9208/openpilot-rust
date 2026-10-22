//! Explicit isolated native cached-image preflight/package/seal worker.
//! No model/provider import, source compilation, dlopen or generated-code call.
fn main() {
    if let Err(error) = run() {
        eprintln!("native cached code: {error}");
        std::process::exit(1);
    }
}
#[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
fn run() -> Result<(), String> {
    Err("cached program issuer requires x86_64 Linux".into())
}
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn run() -> Result<(), String> {
    use openpilot_model_runtime::{
        cached_code::{self, InstructionAnalysis, IssuerRequest},
        owned_outputs::Outputs,
    };
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs::OpenOptions,
        io::Read,
        os::unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
        path::PathBuf,
    };
    for (resource, limit) in [
        (libc::RLIMIT_CORE, 0),
        (libc::RLIMIT_AS, 4_u64 << 30),
        (libc::RLIMIT_CPU, 60),
        (libc::RLIMIT_FSIZE, 64_u64 << 20),
    ] {
        let bounds = libc::rlimit {
            rlim_cur: limit,
            rlim_max: limit,
        };
        // SAFETY: setrlimit reads one initialized live structure, retaining none.
        if unsafe { libc::setrlimit(resource, &bounds) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        let key = key
            .to_str()
            .ok_or("non-UTF8 cached worker option")?
            .to_owned();
        if !matches!(
            key.as_str(),
            "--request"
                | "--request-sha256"
                | "--object"
                | "--isa-receipt"
                | "--admission"
                | "--certificate"
        ) || values
            .insert(
                key,
                args.next().ok_or("cached worker option requires value")?,
            )
            .is_some()
        {
            return Err("unknown/duplicate cached worker option".into());
        }
    }
    if values.len() != 5 && values.len() != 6 {
        return Err("cached worker requires explicit request/SHA and fresh output paths".into());
    }
    let path = PathBuf::from(values.get("--request").ok_or("missing cached request")?);
    let expected = values
        .get("--request-sha256")
        .and_then(|v| v.to_str())
        .ok_or("missing cached request SHA256")?;
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || !path.is_absolute()
        || std::fs::canonicalize(&path).map_err(|e| e.to_string())? != path
    {
        return Err("invalid cached request path/SHA".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    if !before.is_file() || before.len() > 4 * 1024 * 1024 {
        return Err("cached request regular-file/length guard".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() != before.len() as usize || format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err("cached request SHA mismatch".into());
    }
    let request: IssuerRequest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let self_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let self_file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&self_path)
        .map_err(|e| e.to_string())?;
    let self_before = self_file.metadata().map_err(|e| e.to_string())?;
    if !self_before.is_file()
        || !(64..=128 * 1024 * 1024).contains(&self_before.len())
        || self_before.mode() & 0o111 == 0
    {
        return Err("cached issuer ELF size".into());
    }
    // Hash a bounded held descriptor. It stays open throughout issuance.
    use std::os::unix::fs::FileExt;
    let size = self_file.metadata().map_err(|e| e.to_string())?.len() as usize;
    let mut self_bytes = vec![0; size];
    self_file
        .read_exact_at(&mut self_bytes, 0)
        .map_err(|e| e.to_string())?;
    if self_bytes[..7] != *b"\x7fELF\x02\x01\x01"
        || !matches!(
            u16::from_le_bytes(self_bytes[16..18].try_into().unwrap()),
            2 | 3
        )
        || u16::from_le_bytes(self_bytes[18..20].try_into().unwrap()) != 62
    {
        return Err("cached issuer must be a native executable ELF".into());
    }
    if format!("{:x}", Sha256::digest(&self_bytes))
        != request.admission.current.cached_helper_sha256
    {
        return Err("cached issuer explicit ELF SHA differs".into());
    }
    let host = instruction_isa::current_host();
    let issued = cached_code::issue(&request, |image, text| {
        let proof = instruction_isa::admit_all(&[(image, text)], 1, &host)?
            .pop()
            .ok_or("empty cached ISA proof")?;
        Ok(InstructionAnalysis {
            instructions: proof.instructions,
            required: proof.required.into_iter().map(str::to_owned).collect(),
        })
    })?;
    let identity = |m: &std::fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mode(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    let verify = || -> Result<(), String> {
        for (held, path, metadata, expected) in [
            (&file, &path, &before, expected),
            (
                &self_file,
                &self_path,
                &self_before,
                request.admission.current.cached_helper_sha256.as_str(),
            ),
        ] {
            if identity(metadata) != identity(&held.metadata().map_err(|e| e.to_string())?)
                || identity(metadata)
                    != identity(&std::fs::symlink_metadata(path).map_err(|e| e.to_string())?)
                || std::fs::canonicalize(path).map_err(|e| e.to_string())? != *path
            {
                return Err("cached issuer/request identity changed".into());
            }
            let mut bytes = vec![0; metadata.len() as usize];
            held.read_exact_at(&mut bytes, 0)
                .map_err(|e| e.to_string())?;
            if format!("{:x}", Sha256::digest(&bytes)) != expected {
                return Err("cached issuer/request held bytes changed".into());
            }
        }
        Ok(())
    };
    verify()?;
    for option in ["--object", "--isa-receipt", "--admission", "--certificate"] {
        let Some(value) = values.get(option) else {
            continue;
        };
        let output = PathBuf::from(value);
        let parent = output.parent().ok_or("cached output has no parent")?;
        if !output.is_absolute()
            || std::fs::canonicalize(parent).map_err(|e| e.to_string())? != parent
        {
            return Err("cached output requires absolute alias-free parent".into());
        }
        let parent =
            std::ffi::CString::new(parent.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: statvfs writes exactly one live structure, retaining no pointer.
        if unsafe { libc::statvfs(parent.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SAFETY: the successful call initialized this structure.
        let stat = unsafe { stat.assume_init() };
        if u128::from(stat.f_bavail) * u128::from(stat.f_frsize) < 128_u128 << 20 {
            return Err("cached output requires bounded operation growth".into());
        }
    }
    let mut transaction = Outputs::default();
    for (option, bytes) in [
        ("--object", &issued.container),
        ("--isa-receipt", &issued.isa_receipt),
        ("--admission", &issued.original_and_current),
    ] {
        let output = PathBuf::from(values.get(option).ok_or("missing cached worker output")?);
        transaction
            .write(&output, bytes)
            .map_err(|e| e.to_string())?;
    }
    match (values.get("--certificate"), &issued.certificate) {
        (Some(path), Some(bytes)) => transaction
            .write(&PathBuf::from(path), bytes)
            .map_err(|e| e.to_string())?,
        (None, None) => {}
        _ => return Err("cached certificate output/mode differs".into()),
    }
    verify()?;
    transaction.commit().map_err(|e| e.to_string())
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
mod instruction_isa {
    // Inert native ISA admission candidate; deliberately not wired to any exporter.
    // Requires iced-x86 =1.21.0 with decoder,instr_info,std (default features off).
    // The caller must authenticate each complete image and its executable extent
    // against the frozen original plus an independent object reconstruction receipt.
    // This checks instruction/host eligibility only, not code trust, memory safety,
    // argument ABI, relocation provenance, or model numerical acceptance.
    use iced_x86::{Code, CpuidFeature, Decoder, DecoderOptions, FlowControl, OpKind};
    use std::collections::BTreeSet;

    const MAX_IMAGE: usize = 16 * 1024 * 1024;
    const MAX_IMAGES: usize = 4096;
    const MAX_TOTAL: usize = 64 * 1024 * 1024;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct IsaProof {
        pub text_bytes: usize,
        pub instructions: usize,
        pub required: BTreeSet<&'static str>,
    }

    /// `text_bytes` comes from the authenticated object's sole executable section.
    /// Entry is zero; unsupported layouts must be rejected before this function.
    /// Inspect the actual relocated cached bytes, never an unrelocated substitute.
    /// Every decoded instruction contributes requirements, including unreachable code.
    /// Unknown extensions fail closed even when this host might support them.
    pub fn inspect(image: &[u8], text_bytes: usize) -> Result<IsaProof, String> {
        if image.is_empty()
            || image.len() > MAX_IMAGE
            || text_bytes == 0
            || text_bytes > image.len()
        {
            return Err("cached image/executable extent outside bounds".into());
        }
        let mut decoder = Decoder::with_ip(64, &image[..text_bytes], 0, DecoderOptions::NONE);
        let mut starts = BTreeSet::new();
        let mut targets = Vec::new();
        let mut required = BTreeSet::new();
        let mut returns = 0;
        let mut instructions = 0;
        let mut final_falls_through = true;
        while decoder.can_decode() {
            let instruction = decoder.decode();
            let code = instruction.code();
            if code == Code::INVALID || instruction.len() == 0 || code.is_privileged() {
                return Err(format!(
                    "invalid/privileged instruction at {}",
                    instruction.ip()
                ));
            }
            starts.insert(instruction.ip());
            for feature in code.cpuid_features() {
                if let Some(name) = feature_name(*feature)? {
                    required.insert(name);
                }
            }
            final_falls_through = match code.flow_control() {
                FlowControl::Next => true,
                FlowControl::ConditionalBranch | FlowControl::UnconditionalBranch => {
                    if instruction.op0_kind() != OpKind::NearBranch64 {
                        return Err("unsupported branch operand".into());
                    }
                    targets.push(instruction.near_branch_target());
                    code.flow_control() == FlowControl::ConditionalBranch
                }
                FlowControl::Return if code == Code::Retnq => {
                    returns += 1;
                    false
                }
                // Calls, indirect/far branches, interrupt/transaction/exception
                // control flow and special returns require a separate contract.
                _ => return Err(format!("unsupported control flow at {}", instruction.ip())),
            };
            instructions += 1;
            if instructions > 1_000_000 {
                return Err("cached instruction count outside bound".into());
            }
        }
        if decoder.position() != text_bytes || returns == 0 || final_falls_through {
            return Err("incomplete executable extent or fallthrough into constants".into());
        }
        if targets.iter().any(|target| !starts.contains(target)) {
            return Err("branch target is outside text or inside another instruction".into());
        }
        Ok(IsaProof {
            text_bytes,
            instructions,
            required,
        })
    }

    fn feature_name(feature: CpuidFeature) -> Result<Option<&'static str>, String> {
        use CpuidFeature as F;
        // ISA generations required by the x86-64 Linux ABI. *_ONLY features are
        // deliberately excluded: they are not monotonically available extensions.
        let name = match feature {
            F::INTEL8086 | F::INTEL186 | F::INTEL286 | F::INTEL386 | F::INTEL486 | F::X64 => {
                return Ok(None);
            }
            F::CMOV => "cmov",
            F::MULTIBYTENOP => "multibyte-nop",
            F::SSE => "sse",
            F::SSE2 => "sse2",
            F::SSE3 => "sse3",
            F::SSSE3 => "ssse3",
            F::SSE4_1 => "sse4.1",
            F::SSE4_2 => "sse4.2",
            F::AVX => "avx",
            F::AVX2 => "avx2",
            F::BMI1 => "bmi1",
            F::BMI2 => "bmi2",
            F::FMA => "fma",
            F::F16C => "f16c",
            _ => {
                return Err(format!(
                    "unadmitted cached instruction extension: {feature:?}"
                ));
            }
        };
        Ok(Some(name))
    }

    /// Require the complete roster, then the union of every image's requirements.
    /// No prefix/sample admission and no fallback to fresh compilation on mismatch.
    /// A host snapshot is explicit so hosted tests can deny each feature without
    /// executing unsupported instructions or changing CPU/compiler target policy.
    pub fn admit_all(
        images: &[(&[u8], usize)],
        expected_images: usize,
        host: &BTreeSet<&str>,
    ) -> Result<Vec<IsaProof>, String> {
        if images.is_empty() || images.len() > MAX_IMAGES || images.len() != expected_images {
            return Err("cached program roster count mismatch".into());
        }
        let total = images
            .iter()
            .try_fold(0_usize, |total, (image, _)| total.checked_add(image.len()))
            .ok_or("cached program total overflow")?;
        if total > MAX_TOTAL {
            return Err("cached program total outside bound".into());
        }
        let mut proofs = Vec::with_capacity(images.len());
        for (index, (image, text_bytes)) in images.iter().enumerate() {
            let proof = inspect(image, *text_bytes)
                .map_err(|error| format!("cached program {index}: {error}"))?;
            for feature in &proof.required {
                if !host.contains(feature) {
                    return Err(format!("cached program {index}: host lacks {feature}"));
                }
            }
            proofs.push(proof);
        }
        Ok(proofs)
    }

    /// Rust's AVX/FMA/AVX2 detection includes the OS XSAVE state requirements.
    /// This snapshot must be checked again at actual load on the executing host;
    /// exporter-time checks alone do not admit a copied artifact on another machine.
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    pub fn current_host() -> BTreeSet<&'static str> {
        let mut result = BTreeSet::from(["cmov", "multibyte-nop"]);
        // Keep literal tokens at the standard detector call sites: forwarding a
        // macro literal fragment makes its named-feature arms opaque.
        for (name, supported) in [
            ("sse", std::is_x86_feature_detected!("sse")),
            ("sse2", std::is_x86_feature_detected!("sse2")),
            ("sse3", std::is_x86_feature_detected!("sse3")),
            ("ssse3", std::is_x86_feature_detected!("ssse3")),
            ("sse4.1", std::is_x86_feature_detected!("sse4.1")),
            ("sse4.2", std::is_x86_feature_detected!("sse4.2")),
            ("avx", std::is_x86_feature_detected!("avx")),
            ("avx2", std::is_x86_feature_detected!("avx2")),
            ("bmi1", std::is_x86_feature_detected!("bmi1")),
            ("bmi2", std::is_x86_feature_detected!("bmi2")),
            ("fma", std::is_x86_feature_detected!("fma")),
            ("f16c", std::is_x86_feature_detected!("f16c")),
        ] {
            if supported {
                result.insert(name);
            }
        }
        result
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn constant_tail_is_not_decoded_as_instructions() {
            let proof = inspect(&[0xc3, 0x62, 0xff], 1).unwrap();
            assert_eq!(proof.instructions, 1);
            assert!(inspect(&[0xc3, 0x62, 0xff], 3).is_err());
        }

        #[test]
        fn invalid_privileged_external_and_indirect_control_flow_rejected() {
            for bytes in [
                &[0x0f, 0x0b][..],
                &[0xfa, 0xc3],
                &[0xeb, 0x02, 0xc3],
                &[0xff, 0xe0],
                &[0xe8, 0, 0, 0, 0, 0xc3],
            ] {
                assert!(inspect(bytes, bytes.len()).is_err(), "{bytes:x?}");
            }
            // Jump to byte one of its own two-byte encoding, rather than a boundary.
            assert!(inspect(&[0xeb, 0xff, 0xc3], 3).is_err());
            assert!(inspect(&[0x90], 1).is_err());
        }

        #[test]
        fn last_image_cannot_bypass_host_feature_admission() {
            let baseline = [0xc3];
            let avx = [0xc5, 0xfc, 0x58, 0xc0, 0xc3]; // vaddps ymm0,ymm0,ymm0; ret
            let mut images = vec![(&baseline[..], 1); 44];
            images[43] = (&avx, avx.len());
            let weak = BTreeSet::new();
            assert!(admit_all(&images, 44, &weak)
                .unwrap_err()
                .contains("program 43"));
            assert_eq!(
                admit_all(&images, 44, &BTreeSet::from(["avx"]))
                    .unwrap()
                    .len(),
                44
            );
            assert!(admit_all(&images[..43], 44, &BTreeSet::from(["avx"])).is_err());
        }

        #[test]
        fn unknown_extension_fails_even_on_a_claimed_capable_host() {
            let bytes = [0x62, 0xf1, 0x7c, 0x48, 0x58, 0xc0, 0xc3]; // AVX512 vaddps zmm0,zmm0,zmm0
            assert!(inspect(&bytes, bytes.len())
                .unwrap_err()
                .contains("unadmitted"));
        }

        #[test]
        fn avx2_and_fma_are_not_implied_by_avx() {
            let avx_only = BTreeSet::from(["avx"]);
            for bytes in [
                &[0xc5, 0xfd, 0xfe, 0xc0, 0xc3][..],
                &[0xc4, 0xe2, 0x7d, 0xa8, 0xc0, 0xc3],
            ] {
                assert!(admit_all(&[(bytes, bytes.len())], 1, &avx_only).is_err());
            }
        }
    }
}
