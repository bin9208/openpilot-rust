//! Stage the genuine compiler and its native ARM frontend, without an interpreter.
use crate::{
    disk_space,
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
const COMPILER_SHA: &str = "fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093";
fn arm(path: &Path) -> Result<()> {
    let mut header = [0u8; 20];
    File::open(path)?.read_exact(&mut header)?;
    if &header[..6] != b"\x7fELF\x02\x01" || u16::from_le_bytes([header[18], header[19]]) != 183 {
        return Err(format!("expected AArch64 ELF: {}", path.display()).into());
    }
    Ok(())
}
pub struct Providers {
    pub binary: PathBuf,
    pub compiler: PathBuf,
    pub qemu: PathBuf,
    pub libraries: PathBuf,
}
pub fn stage(output: &Path, providers: &Providers) -> Result<Value> {
    stage_with_audit(output, providers, COMPILER_SHA, |roots, libraries| {
        crate::elf_audit::audit(roots, libraries, 183)
    })
}
fn stage_with_audit(
    output: &Path,
    providers: &Providers,
    compiler_sha: &str,
    audit: impl FnOnce(&[PathBuf], &[PathBuf]) -> Result<Value>,
) -> Result<Value> {
    let parent = output
        .parent()
        .ok_or("staging output has no parent")?
        .canonicalize()?;
    let binary_size = fs::metadata(&providers.binary)?.len();
    let compiler_size = fs::metadata(&providers.compiler)?.len();
    let growth = staging_growth(binary_size, compiler_size)?;
    disk_space::inspect(&parent, growth)?.require()?;
    if output.exists() || output.is_symlink() {
        return Err("compiler stage output already exists".into());
    }
    if digest(&providers.compiler)? != compiler_sha {
        return Err("compiler provider SHA256 mismatch".into());
    }
    let binary = providers.binary.canonicalize()?;
    let compiler = providers.compiler.canonicalize()?;
    let qemu = providers.qemu.canonicalize()?;
    let libraries = providers.libraries.canonicalize()?;
    let loader = libraries.join("ld-linux-aarch64.so.1");
    for path in [&binary, &compiler, &loader] {
        arm(path)?;
    }
    let binary_hash = digest(&binary)?;
    let temporary = tempfile::tempdir_in(&parent)?;
    bounded_copy(
        &binary,
        &temporary.path().join("offline-qcom-compile"),
        binary_size,
    )?;
    bounded_copy(
        &compiler,
        &temporary.path().join("libllvm-qcom.so"),
        compiler_size,
    )?;
    if digest(&temporary.path().join("libllvm-qcom.so"))? != compiler_sha
        || digest(&temporary.path().join("offline-qcom-compile"))? != binary_hash
    {
        return Err("compiler staging input changed while copying".into());
    }
    let frozen = [
        temporary.path().join("offline-qcom-compile"),
        temporary.path().join("libllvm-qcom.so"),
    ];
    let abi = audit(&frozen, std::slice::from_ref(&libraries))?;
    if abi["compatible"] != true {
        return Err("native compiler dependency closure is incompatible".into());
    }
    if digest(&frozen[0])? != binary_hash || digest(&frozen[1])? != compiler_sha {
        return Err("frozen compiler input changed during audit".into());
    }
    let output = parent.join(output.file_name().ok_or("staging output has no filename")?);
    let command = vec![
        qemu.to_string_lossy().into_owned(),
        "-cpu".into(),
        "cortex-a57".into(),
        loader.to_string_lossy().into_owned(),
        "--library-path".into(),
        libraries.to_string_lossy().into_owned(),
        output
            .join("offline-qcom-compile")
            .to_string_lossy()
            .into_owned(),
        "--library".into(),
        output
            .join("libllvm-qcom.so")
            .to_string_lossy()
            .into_owned(),
    ];
    let provenance = json!({"compiler_url":"https://raw.githubusercontent.com/commaai/agnos-builder/ec1cf237a84565a056dbbc6f433b1b1c20c07a2c/userspace/root/usr/lib/aarch64-linux-gnu/libllvm-qcom.so","compiler_sha256":compiler_sha,"native_frontend_sha256":binary_hash,"qemu_sha256":digest(&qemu)?,"loader_sha256":digest(&loader)?,"command":command,"gpu_executed":false,"dependency_audit":abi});
    fs::write(
        temporary.path().join("command.json"),
        format!("{}\n", serde_json::to_string(&command)?),
    )?;
    fs::write(
        temporary.path().join("provenance.json"),
        format!("{}\n", serde_json::to_string_pretty(&provenance)?),
    )?;
    fs::rename(temporary.path(), &output)?;
    Ok(provenance)
}
fn bounded_copy(source: &Path, destination: &Path, size: u64) -> Result<()> {
    let mut input = File::open(source)?;
    let metadata = input.metadata()?;
    if metadata.len() != size {
        return Err("compiler staging input size changed".into());
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    copy_exact(&mut input, &mut output, size)?;
    output.set_permissions(metadata.permissions())?;
    Ok(())
}
fn copy_exact(
    input: &mut impl std::io::Read,
    output: &mut impl std::io::Write,
    size: u64,
) -> Result<()> {
    let count = std::io::copy(
        &mut input.take(size.checked_add(1).ok_or("copy bound overflow")?),
        output,
    )?;
    if count != size {
        return Err("compiler staging input size changed while copying".into());
    }
    Ok(())
}
fn staging_growth(binary: u64, compiler: u64) -> Result<u64> {
    if binary > 64 * 1024 * 1024 || compiler > 64 * 1024 * 1024 {
        return Err("compiler staging input exceeds 64 MiB".into());
    }
    binary
        .checked_add(compiler)
        .and_then(|value| value.checked_add(4 * 1024 * 1024))
        .ok_or_else(|| "compiler staging size overflow".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_provider_does_not_create_output() {
        let temp = tempfile::tempdir().unwrap();
        let bad = temp.path().join("bad");
        fs::write(&bad, "changed").unwrap();
        let providers = Providers {
            binary: bad.clone(),
            compiler: bad.clone(),
            qemu: bad.clone(),
            libraries: temp.path().into(),
        };
        let output = temp.path().join("stage");
        assert!(stage(&output, &providers)
            .unwrap_err()
            .to_string()
            .contains("SHA256"));
        assert!(!output.exists());
    }
    #[test]
    fn host_elf_is_not_an_arm_provider() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("elf");
        let mut bytes = [0u8; 20];
        bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
        bytes[18] = 62;
        fs::write(&path, bytes).unwrap();
        assert!(arm(&path).is_err());
    }
    #[test]
    fn bounded_growth_tracks_actual_files() {
        assert_eq!(staging_growth(7, 11).unwrap(), 4 * 1024 * 1024 + 18);
        assert!(staging_growth(64 * 1024 * 1024 + 1, 1).is_err());
        assert!(staging_growth(1, u64::MAX).is_err());
    }
    #[test]
    fn audit_sees_frozen_bytes_and_mutations_cannot_be_published() {
        for mutate_frozen in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let mut header = [0u8; 20];
            header[..6].copy_from_slice(b"\x7fELF\x02\x01");
            header[18..20].copy_from_slice(&183u16.to_le_bytes());
            let binary = temp.path().join("frontend");
            let compiler = temp.path().join("compiler");
            let loader = temp.path().join("ld-linux-aarch64.so.1");
            for path in [&binary, &compiler, &loader] {
                fs::write(path, header).unwrap();
            }
            let providers = Providers {
                binary: binary.clone(),
                compiler: compiler.clone(),
                qemu: loader,
                libraries: temp.path().into(),
            };
            let output = temp.path().join("stage");
            let expected = digest(&compiler).unwrap();
            let result = stage_with_audit(&output, &providers, &expected, |roots, _| {
                assert_eq!(fs::read(&roots[0]).unwrap(), header);
                assert_ne!(roots[0], binary.canonicalize().unwrap());
                fs::write(&binary, b"changed original after freezing").unwrap();
                if mutate_frozen {
                    fs::write(&roots[0], b"changed frozen after audit").unwrap();
                }
                Ok(json!({"compatible":true}))
            });
            if mutate_frozen {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("changed during audit"));
                assert!(!output.exists());
            } else {
                let proof = result.unwrap();
                assert_eq!(
                    digest(&output.join("offline-qcom-compile")).unwrap(),
                    proof["native_frontend_sha256"].as_str().unwrap()
                );
                assert_eq!(
                    fs::read(output.join("offline-qcom-compile")).unwrap(),
                    header
                );
            }
        }
    }
    #[test]
    fn growing_inputs_cannot_copy_beyond_reserved_bytes() {
        let mut input = std::io::Cursor::new(vec![1u8; 1024]);
        let mut output = Vec::new();
        assert!(copy_exact(&mut input, &mut output, 2).is_err());
        assert_eq!(output.len(), 3);
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("growing");
        let destination = temp.path().join("copy");
        fs::write(&source, [1; 4]).unwrap();
        assert!(bounded_copy(&source, &destination, 2).is_err());
        assert!(!destination.exists());
    }
}
