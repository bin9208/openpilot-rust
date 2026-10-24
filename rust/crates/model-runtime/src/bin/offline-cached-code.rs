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
use openpilot_model_runtime::cached_isa as instruction_isa;
