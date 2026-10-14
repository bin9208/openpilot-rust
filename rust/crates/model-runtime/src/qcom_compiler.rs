//! Genuine A630 source compiler ABI, retained from the pinned tinygrad provider.
//! Derived from tinygrad/runtime/support/compiler_qcom.py; MIT notice: src/qcom/LICENSE.
//! This compiles a kernel; tensor graph scheduling is a separate compiler stage.
use libloading::Library;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    fs,
    path::Path,
    ptr,
};

pub const PROVIDER_SHA256: &str =
    "fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[repr(C)]
struct Compiled {
    chip: u64,
    mode: u32,
    bitcode: *mut c_void,
    size: u64,
    log: *const c_char,
    log_len: u32,
    error: u32,
}
#[repr(C)]
struct Executable {
    kernels: i32,
    properties: *mut c_void,
    error: u32,
    log: *const c_char,
    unknown: [u8; 32],
    chip: u64,
    mode: u32,
}
#[repr(C)]
struct Handle {
    kind: u32,
    data: *mut c_void,
}
type Create = unsafe extern "C" fn() -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Compile = unsafe extern "C" fn(
    *mut c_void,
    u64,
    i32,
    *const c_char,
    i32,
    u64,
    u64,
    *const c_char,
    u64,
    u64,
    *mut c_void,
) -> *mut Handle;
type Link = unsafe extern "C" fn(
    *mut c_void,
    u64,
    i32,
    *const c_char,
    i32,
    *const *mut Handle,
) -> *mut Handle;
type Binary = unsafe extern "C" fn(*mut Handle, *mut *mut c_void, *mut u64);
type Free = unsafe extern "C" fn(*mut Handle);

/// Holds the library for longer than every ABI function pointer and owned handle.
pub struct Compiler {
    _library: Library,
    create: Create,
    instance: *mut c_void,
    destroy: Destroy,
    compile: Compile,
    link: Link,
    binary: Binary,
    free: Free,
    free_assembly: Destroy,
}
impl Compiler {
    pub fn open(path: &Path) -> Result<Self> {
        use std::io::Read;
        let mut provider = fs::File::open(path)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut total = 0usize;
        loop {
            let count = provider.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count)
                .ok_or("QCOM provider size overflow")?;
            if total > 64 * 1024 * 1024 {
                return Err("QCOM provider exceeds 64 MiB".into());
            }
            hash.update(&buffer[..count]);
        }
        if format!("{:x}", hash.finalize()) != PROVIDER_SHA256 {
            return Err("QCOM compiler provider SHA256 mismatch".into());
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let identity_path =
                std::path::PathBuf::from(format!("/proc/self/fd/{}", provider.as_raw_fd()));
            // SAFETY: the admitted open file identifies the reviewed ABI; retaining the
            // descriptor until dlopen finishes prevents a pathname swap after hashing.
            unsafe { Self::load(&identity_path) }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err("native A630 provider loading requires Linux".into())
        }
    }
    unsafe fn load(path: &Path) -> Result<Self> {
        // SAFETY: caller admits a provider implementing these exact function signatures.
        let library = unsafe { Library::new(path)? };
        unsafe {
            let create: Create = *library.get(b"cl_compiler_create_llvm_instance\0")?;
            let destroy = *library.get(b"cl_compiler_destroy_llvm_instance\0")?;
            let compile = *library.get(b"cl_compiler_compile_source\0")?;
            let link = *library.get(b"cl_compiler_link_program\0")?;
            let binary = *library.get(b"cl_compiler_handle_create_binary\0")?;
            let free = *library.get(b"cl_compiler_free_handle\0")?;
            let free_assembly = *library.get(b"cl_compiler_free_assembly\0")?;
            let instance = create();
            if instance.is_null() {
                return Err("QCOM compiler returned no instance".into());
            }
            Ok(Self {
                _library: library,
                create,
                instance,
                destroy,
                compile,
                link,
                binary,
                free,
                free_assembly,
            })
        }
    }
    pub fn compile(&mut self, source: &str) -> Result<Vec<u8>> {
        if self.instance.is_null() {
            return Err("QCOM compiler has no instance".into());
        }
        let result = self.compile_inner(source);
        if result.is_err() {
            // SAFETY: every temporary handle has dropped before replacing this instance.
            unsafe {
                (self.destroy)(self.instance);
                self.instance = (self.create)();
            }
        }
        result
    }
    fn compile_inner(&mut self, source: &str) -> Result<Vec<u8>> {
        if source.len() > 64 * 1024 * 1024 {
            return Err("QCOM kernel source exceeds 64 MiB".into());
        }
        let source = CString::new(source)?;
        // SAFETY: instance and C strings remain live for these synchronous provider calls.
        let compiled = unsafe {
            (self.compile)(
                self.instance,
                0x6030001,
                1,
                c"".as_ptr(),
                0,
                0,
                0,
                source.as_ptr(),
                0,
                0,
                ptr::null_mut(),
            )
        };
        let compiled = OwnedHandle {
            pointer: compiled,
            free: self.free,
        };
        checked(compiled.pointer, 1)?;
        // SAFETY: checked produced one live compiled handle; the single-element array matches count.
        let linked = unsafe {
            (self.link)(
                self.instance,
                0x6030001,
                1,
                ptr::null(),
                1,
                &compiled.pointer,
            )
        };
        let linked = OwnedHandle {
            pointer: linked,
            free: self.free,
        };
        checked(linked.pointer, 3)?;
        let mut data = ptr::null_mut();
        let mut size = 0;
        // SAFETY: linked handle and output variables remain live for the synchronous call.
        unsafe { (self.binary)(linked.pointer, &mut data, &mut size) };
        let assembly = OwnedAssembly {
            pointer: data,
            free: self.free_assembly,
        };
        if size == 0 || size > 512 * 1024 * 1024 || assembly.pointer.is_null() {
            return Err("QCOM compiler returned invalid binary size or pointer".into());
        }
        // SAFETY: admitted provider owns an allocation of exactly the returned size until free_assembly.
        Ok(
            unsafe { std::slice::from_raw_parts(assembly.pointer.cast::<u8>(), size as usize) }
                .to_vec(),
        )
    }
}
impl Drop for Compiler {
    fn drop(&mut self) {
        if !self.instance.is_null() {
            unsafe { (self.destroy)(self.instance) };
        }
    }
}
struct OwnedHandle {
    pointer: *mut Handle,
    free: Free,
}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe { (self.free)(self.pointer) };
        }
    }
}
struct OwnedAssembly {
    pointer: *mut c_void,
    free: Destroy,
}
impl Drop for OwnedAssembly {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe { (self.free)(self.pointer) };
        }
    }
}
fn checked(pointer: *mut Handle, expected: u32) -> Result<()> {
    if pointer.is_null() {
        return Err("QCOM compiler returned no handle".into());
    }
    // SAFETY: only synchronous admitted provider calls produce this pointer and its tagged data.
    unsafe {
        let handle = &*pointer;
        if handle.kind != expected || handle.data.is_null() {
            return Err("QCOM compiler returned invalid handle".into());
        }
        let (error, log) = if expected == 3 {
            let data = &*handle.data.cast::<Executable>();
            (data.error, data.log)
        } else {
            let data = &*handle.data.cast::<Compiled>();
            (data.error, data.log)
        };
        if error != 0 {
            let message = if log.is_null() {
                "no build log".into()
            } else {
                CStr::from_ptr(log).to_string_lossy().into_owned()
            };
            return Err(format!("QCOM compilation error: {message}").into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_identity_is_checked_before_loading() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("provider");
        fs::write(&path, b"changed").unwrap();
        assert!(Compiler::open(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("SHA256"));
        assert_eq!(std::mem::size_of::<Compiled>(), 48);
        assert_eq!(std::mem::size_of::<Executable>(), 80);
        assert_eq!(std::mem::size_of::<Handle>(), 16);
    }
    #[test]
    fn exact_calls_binary_and_failure_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("fixture.c");
        let library = temp.path().join("fixture.so");
        fs::write(&source, include_str!("qcom_compiler_test.c")).unwrap();
        assert!(std::process::Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(&library)
            .arg(&source)
            .status()
            .unwrap()
            .success());
        // SAFETY: this test-owned C implementation exports the exact reviewed ABI.
        let mut compiler = unsafe { Compiler::load(&library) }.unwrap();
        for (kernel, message, expected) in [
            ("good", "", [2, 1]),
            ("compile_error", "source failed", [3, 1]),
            ("link_error", "link failed", [5, 1]),
            ("empty", "invalid binary", [7, 2]),
            ("invalid_tag", "invalid handle", [8, 2]),
            ("null_handle", "no handle", [8, 2]),
            ("good", "", [10, 3]),
        ] {
            let result = compiler.compile(kernel);
            if message.is_empty() {
                assert_eq!(result.unwrap(), [0, 255, 17, 0]);
            } else {
                assert!(result.unwrap_err().to_string().contains(message));
            }
            // SAFETY: C fixture counter signature returns a live pointer to two static ints.
            let counts = unsafe {
                let function: libloading::Symbol<unsafe extern "C" fn() -> *const i32> =
                    compiler._library.get(b"counts\0").unwrap();
                std::slice::from_raw_parts(function(), 2)
            };
            assert_eq!(counts, expected);
        }
        assert!(compiler.compile("nul\0source").is_err());
        unsafe {
            let counts: libloading::Symbol<unsafe extern "C" fn() -> *const i32> =
                compiler._library.get(b"counts\0").unwrap();
            assert_eq!(std::slice::from_raw_parts(counts().add(2), 2), [7, 6]);
            let null_next: libloading::Symbol<unsafe extern "C" fn()> =
                compiler._library.get(b"null_next_create\0").unwrap();
            null_next();
        }
        assert!(compiler.compile("compile_error").is_err());
        assert!(compiler
            .compile("good")
            .unwrap_err()
            .to_string()
            .contains("no instance"));
        let keep_alive = unsafe { Library::new(&library) }.unwrap();
        drop(compiler);
        unsafe {
            let counts: libloading::Symbol<unsafe extern "C" fn() -> *const i32> =
                keep_alive.get(b"counts\0").unwrap();
            assert_eq!(std::slice::from_raw_parts(counts().add(2), 2), [8, 7]);
            let null_next: libloading::Symbol<unsafe extern "C" fn()> =
                keep_alive.get(b"null_next_create\0").unwrap();
            null_next();
            assert!(Compiler::load(&library)
                .err()
                .unwrap()
                .to_string()
                .contains("no instance"));
            assert_eq!(std::slice::from_raw_parts(counts().add(2), 2), [9, 7]);
        }
    }
}
