//! The only unsafe boundary in the native corpus generator. The user selects
//! the original C++ oracle directory explicitly with `--oracle`; loading that
//! provider executes its code. The source/header/ELF proof belongs to the
//! hosted caller, and these held pins detect changes during this invocation.
use super::{functions, Model};
use crate::seal::Result;
use libloading::Library;
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::{
    ffi::CString,
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

type Timed = unsafe extern "C" fn(*const f64, f64, *mut f64);
type Observed = unsafe extern "C" fn(*const f64, *const f64, *mut f64);
type Setter = unsafe extern "C" fn(f64);

#[derive(Clone, Copy)]
enum FunctionPointer {
    Timed(Timed),
    Observed(Observed),
}

struct ResolvedCall {
    pointer: FunctionPointer,
    width: usize,
}

fn identity(metadata: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

struct Pin {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    digest: [u8; 32],
}

impl Pin {
    fn open(path: PathBuf, limit: u64) -> Result<(Self, Vec<u8>)> {
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit {
            return Err(format!("unsupported oracle input: {}", path.display()).into());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)?;
        if identity(&metadata) != identity(&file.metadata()?)
            || u64::try_from(bytes.len())? != metadata.len()
        {
            return Err(format!("oracle input changed while reading: {}", path.display()).into());
        }
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        Ok((
            Self {
                path,
                file,
                metadata,
                digest,
            },
            bytes,
        ))
    }

    fn verify(&self, limit: u64) -> Result<()> {
        let (current, _) = Self::open(self.path.clone(), limit)?;
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&current.metadata)
            || self.digest != current.digest
        {
            return Err(format!("oracle input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
}

fn validate_prototype(header: &str, symbol: &str, pointer_arguments: &[bool]) -> Result<()> {
    let prefix = format!("void {symbol}(");
    let mut matching = header.lines().filter(|line| line.starts_with(&prefix));
    let line = matching
        .next()
        .ok_or_else(|| format!("missing oracle prototype {symbol}"))?;
    if matching.next().is_some() {
        return Err(format!("duplicate oracle prototype {symbol}").into());
    }
    let arguments = line
        .strip_prefix(&prefix)
        .and_then(|line| line.strip_suffix(");"))
        .ok_or_else(|| format!("invalid oracle prototype {symbol}"))?;
    let fields = arguments.split(',').collect::<Vec<_>>();
    if fields.len() != pointer_arguments.len()
        || fields
            .iter()
            .zip(pointer_arguments)
            .any(|(field, pointer)| {
                let field = field.trim().strip_prefix("const ").unwrap_or(field.trim());
                let Some(rest) = field.strip_prefix("double") else {
                    return true;
                };
                if !rest
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'*')
                {
                    return true;
                }
                let rest = rest.trim_start();
                let rest = if *pointer {
                    let Some(rest) = rest.strip_prefix('*') else {
                        return true;
                    };
                    rest.trim_start()
                } else {
                    rest
                };
                let mut bytes = rest.bytes();
                !bytes
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                    || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
    {
        return Err(format!("unexpected oracle ABI for {symbol}: {line}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_prototype;

    #[test]
    fn reject_wrong_or_duplicate_c_abi_before_library_load() {
        let valid = "void car_f_fun(double *state, double dt, double *out);\n";
        assert!(validate_prototype(valid, "car_f_fun", &[true, false, true]).is_ok());
        assert!(validate_prototype(valid, "car_f_fun", &[true, true, true]).is_err());
        assert!(validate_prototype(valid, "car_F_fun", &[true, false, true]).is_err());
        assert!(validate_prototype(
            &(valid.to_owned() + valid),
            "car_f_fun",
            &[true, false, true]
        )
        .is_err());
        assert!(validate_prototype(
            "void car_set_mass(double *value);\n",
            "car_set_mass",
            &[false]
        )
        .is_err());
    }
}

pub(super) struct Provider {
    _library: Library,
    header: Pin,
    binary: Pin,
    calls: Vec<ResolvedCall>,
    setters: Option<[Setter; 6]>,
    state_width: usize,
}

impl Provider {
    pub(super) fn load(model: Model, root: &Path) -> Result<Self> {
        let directory = fs::symlink_metadata(root)?;
        if !directory.is_dir() || directory.file_type().is_symlink() {
            return Err(format!("oracle directory is not real: {}", root.display()).into());
        }
        let stem = model.name();
        let extension = if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
        let (header, bytes) = Pin::open(root.join(format!("{stem}.h")), 1024 * 1024)?;
        let header_text = String::from_utf8(bytes)?;
        let (binary, _) = Pin::open(
            root.join(format!("lib{stem}.{extension}")),
            64 * 1024 * 1024,
        )?;
        let roster = functions(model);
        for function in &roster {
            let symbol = format!("{stem}_{}", function.name);
            let arguments = if function.timed {
                [true, false, true]
            } else {
                [true, true, true]
            };
            validate_prototype(&header_text, &symbol, &arguments)?;
        }
        const GLOBALS: [&str; 6] = [
            "mass",
            "rotational_inertia",
            "center_to_front",
            "center_to_rear",
            "stiffness_front",
            "stiffness_rear",
        ];
        if model == Model::Car {
            for name in GLOBALS {
                validate_prototype(&header_text, &format!("car_set_{name}"), &[false])?;
            }
        }
        // Linux loads through the held descriptor, so a pathname replacement
        // cannot redirect dlopen. macOS loads the path, checked on both sides.
        #[cfg(target_os = "linux")]
        let selected = PathBuf::from(format!("/proc/self/fd/{}", binary.file.as_raw_fd()));
        #[cfg(not(target_os = "linux"))]
        let selected = binary.path.clone();
        // SAFETY: --oracle is an explicitly selected compiled provider. All
        // known prototypes were checked before calling; this load may execute
        // C++ initializers and therefore runs only in this tool process.
        let library = unsafe { Library::new(&selected) }?;
        header.verify(1024 * 1024)?;
        binary.verify(64 * 1024 * 1024)?;

        let mut calls = Vec::with_capacity(roster.len());
        for function in &roster {
            let symbol = CString::new(format!("{stem}_{}", function.name))?;
            // SAFETY: The checked C header fixes each function's ABI. Copying
            // the pointer is valid while `_library` remains held by Provider.
            let pointer = unsafe {
                if function.timed {
                    FunctionPointer::Timed(*library.get::<Timed>(symbol.as_bytes_with_nul())?)
                } else {
                    FunctionPointer::Observed(*library.get::<Observed>(symbol.as_bytes_with_nul())?)
                }
            };
            calls.push(ResolvedCall {
                pointer,
                width: function.width,
            });
        }
        let setters = if model == Model::Car {
            let mut found = Vec::with_capacity(6);
            for name in GLOBALS {
                let symbol = CString::new(format!("car_set_{name}"))?;
                // SAFETY: The setter's one-double C ABI is checked above.
                found.push(unsafe { *library.get::<Setter>(symbol.as_bytes_with_nul())? });
            }
            Some(found.try_into().map_err(|_| "wrong car setter count")?)
        } else {
            None
        };
        Ok(Self {
            _library: library,
            header,
            binary,
            calls,
            setters,
            state_width: if model == Model::Pose { 18 } else { 9 },
        })
    }

    pub(super) fn set_globals(&self, values: &[f64]) -> Result<()> {
        let setters = self.setters.as_ref().ok_or("pose has no global setters")?;
        let values: &[f64; 6] = values.try_into().map_err(|_| "wrong car global count")?;
        for (&setter, &value) in setters.iter().zip(values) {
            // SAFETY: One scalar follows the checked original setter ABI, in
            // source order; provider calls are serialized within this process.
            unsafe { setter(value) };
        }
        Ok(())
    }

    pub(super) fn call(
        &self,
        index: usize,
        state: &[f64],
        dt: f64,
        output: &mut [f64],
    ) -> Result<()> {
        let call = self.calls.get(index).ok_or("unknown oracle function")?;
        if state.len() != self.state_width || output.len() != call.width {
            return Err("invalid oracle function buffer".into());
        }
        let unused = [0.0_f64];
        // SAFETY: State and output lengths are fixed by the closed function
        // roster; the original C++ functions only borrow these live buffers.
        unsafe {
            match call.pointer {
                FunctionPointer::Timed(function) => {
                    function(state.as_ptr(), dt, output.as_mut_ptr())
                }
                FunctionPointer::Observed(function) => {
                    function(state.as_ptr(), unused.as_ptr(), output.as_mut_ptr())
                }
            }
        }
        Ok(())
    }

    pub(super) fn verify(&self) -> Result<()> {
        self.header.verify(1024 * 1024)?;
        self.binary.verify(64 * 1024 * 1024)
    }
}
