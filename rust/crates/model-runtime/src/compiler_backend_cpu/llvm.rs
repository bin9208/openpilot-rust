//! Pinned LLVM C API provider, intended only for the isolated compiler worker.
//! The caller selects every target and pass option; no host defaults or fallback.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    fs::File,
    io::Read,
    path::PathBuf,
};
pub const MAX_SOURCE: usize = 16 * 1024 * 1024;
pub const MAX_OUTPUT: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub library: PathBuf,
    pub library_sha256: String,
    pub triple: String,
    pub cpu: String,
    pub features: String,
    pub passes: String,
    pub loop_unrolling: bool,
    pub loop_vectorization: bool,
    pub slp_vectorization: bool,
    pub verify_each: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub config: Config,
    pub llvm_version: [u32; 3],
    pub source_sha256: String,
    pub object_sha256: String,
    pub optimized_ir_sha256: String,
}
pub struct Compiled {
    pub object: Vec<u8>,
    pub optimized_ir: String,
    pub receipt: Receipt,
}
type Ref = *mut c_void;
type Dispose = unsafe extern "C" fn(Ref);
struct Owned {
    ptr: Ref,
    dispose: Dispose,
}
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: each guard owns one live LLVM object and its matching destructor.
        unsafe { (self.dispose)(self.ptr) }
    }
}
fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn string(value: &str) -> Result<CString, String> {
    if value.len() > 32 * 1024 {
        return Err("LLVM configuration string exceeds bound".into());
    }
    CString::new(value).map_err(|_| "LLVM configuration contains NUL".into())
}
/// Compile only inside a dedicated child process. A pinned LLVM may abort on
/// malformed IR/pass configurations; the parent must enforce timeout/exit status.
pub fn require_output_space(path: &std::path::Path) -> Result<(), String> {
    super::require_space(path, (2 * MAX_OUTPUT + MAX_SOURCE + 65536) as u64)
}
pub fn compile_worker(config: &Config, source: &str) -> Result<Compiled, String> {
    if source.len() > MAX_SOURCE || source.is_empty() {
        return Err("LLVM source length outside bound".into());
    }
    if config.library_sha256.len() != 64
        || !config
            .library_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("invalid LLVM library SHA256".into());
    }
    let triple = string(&config.triple)?;
    let cpu = string(&config.cpu)?;
    let features = string(&config.features)?;
    let passes = string(&config.passes)?;
    if config.cpu.is_empty() || config.triple.is_empty() || config.passes.is_empty() {
        return Err("LLVM target/CPU/pass must be explicit".into());
    }
    // Hold the exact verified file open and dlopen its descriptor, eliminating a
    // path replacement race between SHA verification and library selection.
    let mut file = File::open(&config.library).map_err(|e| format!("LLVM library: {e}"))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 2u64 << 30 {
        return Err("LLVM library is not a bounded regular file".into());
    }
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    if format!("{:x}", hash.finalize()) != config.library_sha256 {
        return Err("LLVM library SHA256 mismatch".into());
    }
    use std::os::fd::AsRawFd;
    static USED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if USED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("LLVM worker accepts exactly one compilation".into());
    }
    let selected = format!("/proc/self/fd/{}", file.as_raw_fd());
    // SAFETY: only the explicitly SHA-pinned native library is loaded. All symbols
    // below have the signatures of the stable LLVM C API (LLVM18 and LLVM20).
    let library = unsafe { libloading::Library::new(selected) }.map_err(|e| e.to_string())?;
    macro_rules! api {
        ($name:literal,$ty:ty) => {{
            // SAFETY: symbol signatures are declared from llvm-c headers, and copied
            // function pointers never outlive `library` in this function.
            unsafe {
                *library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|e| e.to_string())?
            }
        }};
    }
    let dispose_message = api!("LLVMDisposeMessage", unsafe extern "C" fn(*mut c_char));
    let message = |ptr: *mut c_char| -> String {
        if ptr.is_null() {
            return "LLVM returned no diagnostic".into();
        }
        // SAFETY: LLVM diagnostics are owned NUL-terminated messages.
        let result = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: the diagnostic is released once with its specified destructor.
        unsafe { dispose_message(ptr) };
        result
    };
    let version = api!(
        "LLVMGetVersion",
        unsafe extern "C" fn(*mut u32, *mut u32, *mut u32)
    );
    let mut llvm_version = [0; 3];
    // SAFETY: three disjoint live u32 outputs.
    unsafe {
        version(
            &mut llvm_version[0],
            &mut llvm_version[1],
            &mut llvm_version[2],
        )
    };
    let prefix = if config.triple.starts_with("x86_64") {
        "X86"
    } else if config.triple.starts_with("aarch64") {
        "AArch64"
    } else {
        return Err("unsupported LLVM target architecture".into());
    };
    for suffix in [
        "TargetInfo",
        "Target",
        "TargetMC",
        "AsmParser",
        "AsmPrinter",
    ] {
        let symbol = CString::new(format!("LLVMInitialize{prefix}{suffix}")).unwrap();
        // SAFETY: registered target initialization functions take no arguments.
        let init = unsafe {
            *library
                .get::<unsafe extern "C" fn()>(symbol.as_bytes_with_nul())
                .map_err(|e| e.to_string())?
        };
        // SAFETY: single library/context per compiler worker.
        unsafe { init() };
    }
    let context_create = api!("LLVMContextCreate", unsafe extern "C" fn() -> Ref);
    let context_dispose = api!("LLVMContextDispose", Dispose);
    // SAFETY: returns a new independently owned context.
    let context = Owned {
        ptr: unsafe { context_create() },
        dispose: context_dispose,
    };
    if context.ptr.is_null() {
        return Err("LLVM context allocation failed".into());
    }
    let target_from = api!(
        "LLVMGetTargetFromTriple",
        unsafe extern "C" fn(*const c_char, *mut Ref, *mut *mut c_char) -> i32
    );
    let mut target = std::ptr::null_mut();
    let mut error = std::ptr::null_mut();
    // SAFETY: valid NUL string and writable outputs, context retains nothing.
    if unsafe { target_from(triple.as_ptr(), &mut target, &mut error) } != 0 {
        return Err(message(error));
    }
    let machine_create = api!(
        "LLVMCreateTargetMachine",
        unsafe extern "C" fn(
            Ref,
            *const c_char,
            *const c_char,
            *const c_char,
            i32,
            i32,
            i32,
        ) -> Ref
    );
    let machine_dispose = api!("LLVMDisposeTargetMachine", Dispose);
    // SAFETY: LLVM-owned target, valid strings; O2, PIC, default code model match
    // the original provider's CodeGenDefault/RelocPIC/CodeModelDefault settings.
    let machine = Owned {
        ptr: unsafe {
            machine_create(
                target,
                triple.as_ptr(),
                cpu.as_ptr(),
                features.as_ptr(),
                2,
                2,
                0,
            )
        },
        dispose: machine_dispose,
    };
    if machine.ptr.is_null() {
        return Err("LLVM target machine allocation failed".into());
    }
    let options_create = api!(
        "LLVMCreatePassBuilderOptions",
        unsafe extern "C" fn() -> Ref
    );
    let options_dispose = api!("LLVMDisposePassBuilderOptions", Dispose);
    // SAFETY: new owned pass options.
    let options = Owned {
        ptr: unsafe { options_create() },
        dispose: options_dispose,
    };
    if options.ptr.is_null() {
        return Err("LLVM pass options allocation failed".into());
    }
    for (name, value) in [
        (
            "LLVMPassBuilderOptionsSetLoopUnrolling",
            config.loop_unrolling,
        ),
        (
            "LLVMPassBuilderOptionsSetLoopVectorization",
            config.loop_vectorization,
        ),
        (
            "LLVMPassBuilderOptionsSetSLPVectorization",
            config.slp_vectorization,
        ),
        ("LLVMPassBuilderOptionsSetVerifyEach", config.verify_each),
    ] {
        let name = CString::new(name).unwrap();
        // SAFETY: all four C API setters have the same options/LLVMBool signature.
        let setter = unsafe {
            *library
                .get::<unsafe extern "C" fn(Ref, i32)>(name.as_bytes_with_nul())
                .map_err(|e| e.to_string())?
        };
        // SAFETY: live options, bool represented by LLVM's i32 LLVMBool.
        unsafe { setter(options.ptr, i32::from(value)) };
    }
    let buffer_copy = api!(
        "LLVMCreateMemoryBufferWithMemoryRangeCopy",
        unsafe extern "C" fn(*const c_char, usize, *const c_char) -> Ref
    );
    let parse = api!(
        "LLVMParseIRInContext",
        unsafe extern "C" fn(Ref, Ref, *mut Ref, *mut *mut c_char) -> i32
    );
    let module_dispose = api!("LLVMDisposeModule", Dispose);
    // SAFETY: bounded source bytes are copied before this call returns.
    let buffer = unsafe { buffer_copy(source.as_ptr().cast(), source.len(), c"src".as_ptr()) };
    if buffer.is_null() {
        return Err("LLVM source buffer allocation failed".into());
    }
    let mut module = std::ptr::null_mut();
    error = std::ptr::null_mut();
    // SAFETY: parser consumes the source buffer on both success and failure.
    if unsafe { parse(context.ptr, buffer, &mut module, &mut error) } != 0 {
        return Err(message(error));
    }
    let module = Owned {
        ptr: module,
        dispose: module_dispose,
    };
    let verify = api!(
        "LLVMVerifyModule",
        unsafe extern "C" fn(Ref, i32, *mut *mut c_char) -> i32
    );
    error = std::ptr::null_mut();
    // SAFETY: ReturnStatusAction=2 prevents verifier-triggered process aborts.
    let invalid = unsafe { verify(module.ptr, 2, &mut error) };
    if invalid != 0 {
        return Err(message(error));
    }
    if !error.is_null() {
        let _ = message(error);
    }
    let passes_run = api!(
        "LLVMRunPasses",
        unsafe extern "C" fn(Ref, *const c_char, Ref, Ref) -> Ref
    );
    let error_message = api!(
        "LLVMGetErrorMessage",
        unsafe extern "C" fn(Ref) -> *mut c_char
    );
    let error_dispose = api!("LLVMDisposeErrorMessage", unsafe extern "C" fn(*mut c_char));
    // SAFETY: all resources/strings live for the duration of the pass pipeline.
    let pass_error = unsafe { passes_run(module.ptr, passes.as_ptr(), machine.ptr, options.ptr) };
    if !pass_error.is_null() {
        // SAFETY: GetErrorMessage consumes the error; its message has a separate
        // destructor from LLVMDisposeMessage.
        let ptr = unsafe { error_message(pass_error) };
        if ptr.is_null() {
            return Err("LLVM pass failed without diagnostic".into());
        }
        // SAFETY: owned NUL-terminated LLVM error message.
        let result = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: exactly one release of the owned error message.
        unsafe { error_dispose(ptr) };
        return Err(result);
    }
    let print = api!(
        "LLVMPrintModuleToString",
        unsafe extern "C" fn(Ref) -> *mut c_char
    );
    // SAFETY: module remains live throughout printing.
    let ptr = unsafe { print(module.ptr) };
    if ptr.is_null() {
        return Err("LLVM IR printing failed".into());
    }
    // SAFETY: printer returns a NUL-terminated LLVM-owned string.
    let raw = unsafe { CStr::from_ptr(ptr) }.to_bytes();
    let optimized_ir = if raw.len() > MAX_OUTPUT {
        Err("optimized LLVM IR exceeds bound".to_string())
    } else {
        String::from_utf8(raw.to_vec()).map_err(|e| e.to_string())
    };
    // SAFETY: printer result is released once even when length/UTF8 validation fails.
    unsafe { dispose_message(ptr) };
    let optimized_ir = optimized_ir?;
    let emit = api!(
        "LLVMTargetMachineEmitToMemoryBuffer",
        unsafe extern "C" fn(Ref, Ref, i32, *mut *mut c_char, *mut Ref) -> i32
    );
    let buffer_dispose = api!("LLVMDisposeMemoryBuffer", Dispose);
    let buffer_size = api!("LLVMGetBufferSize", unsafe extern "C" fn(Ref) -> usize);
    let buffer_start = api!("LLVMGetBufferStart", unsafe extern "C" fn(Ref) -> *const u8);
    let mut object = std::ptr::null_mut();
    error = std::ptr::null_mut();
    // SAFETY: output arguments are writable; LLVMObjectFile=1.
    if unsafe { emit(machine.ptr, module.ptr, 1, &mut error, &mut object) } != 0 {
        return Err(message(error));
    }
    let object = Owned {
        ptr: object,
        dispose: buffer_dispose,
    };
    // SAFETY: successful emission initialized the owned memory buffer.
    let size = unsafe { buffer_size(object.ptr) };
    if size == 0 || size > MAX_OUTPUT {
        return Err("LLVM object size outside bound".into());
    }
    // SAFETY: buffer exposes size bytes until object is dropped.
    let object_bytes =
        unsafe { std::slice::from_raw_parts(buffer_start(object.ptr), size) }.to_vec();
    if !object_bytes.starts_with(b"\x7fELF") {
        return Err("LLVM target did not emit an ELF object".into());
    }
    let receipt = Receipt {
        config: config.clone(),
        llvm_version,
        source_sha256: digest(source.as_bytes()),
        object_sha256: digest(&object_bytes),
        optimized_ir_sha256: digest(optimized_ir.as_bytes()),
    };
    Ok(Compiled {
        object: object_bytes,
        optimized_ir,
        receipt,
    })
}

/// Compile through a bounded worker and validate the complete receipt before
/// returning bytes. Temporary products are private and never publish artifacts.
pub fn compile_isolated(
    worker: &std::path::Path,
    config: &Config,
    source: &str,
    timeout: std::time::Duration,
) -> Result<Compiled, String> {
    use std::{
        fs,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
        time::Instant,
    };
    if source.is_empty() || source.len() > MAX_SOURCE {
        return Err("LLVM source length outside bound".into());
    }
    let root = std::env::temp_dir();
    require_output_space(&root)?;
    let temp = tempfile::tempdir_in(root).map_err(|e| e.to_string())?;
    let path = temp.path();
    fs::write(path.join("source"), source).map_err(|e| e.to_string())?;
    fs::write(
        path.join("config"),
        serde_json::to_vec(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut command = Command::new(worker);
    for (flag, name) in [
        ("--config", "config"),
        ("--object", "object"),
        ("--receipt", "receipt"),
        ("--optimized-ir", "optimized"),
    ] {
        command.arg(flag).arg(path.join(name));
    }
    command
        .stdin(File::open(path.join("source")).map_err(|e| e.to_string())?)
        .stdout(Stdio::from(
            File::create(path.join("stdout")).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            File::create(path.join("stderr")).map_err(|e| e.to_string())?,
        ))
        .process_group(0);
    let mut child = super::ChildGroup {
        child: command.spawn().map_err(|e| e.to_string())?,
        reaped: false,
    };
    let start = Instant::now();
    loop {
        for (name, limit) in [
            ("stdout", 1 << 20),
            ("stderr", 1 << 20),
            ("receipt", 65536),
            ("object", MAX_OUTPUT),
            ("optimized", MAX_OUTPUT),
        ] {
            if fs::metadata(path.join(name))
                .map(|m| m.len() > limit as u64)
                .unwrap_or(false)
            {
                return Err("LLVM worker output exceeds bound".into());
            }
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!(
                    "LLVM worker {status}: {}",
                    String::from_utf8_lossy(&super::read_bounded(&path.join("stderr"), 1 << 20)?)
                ));
            }
            break;
        }
        if start.elapsed() >= timeout {
            return Err("LLVM worker timeout".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let receipt: Receipt =
        serde_json::from_slice(&super::read_bounded(&path.join("receipt"), 65536)?)
            .map_err(|e| e.to_string())?;
    let object = super::read_bounded(&path.join("object"), MAX_OUTPUT as u64)?;
    let optimized_ir = String::from_utf8(super::read_bounded(
        &path.join("optimized"),
        MAX_OUTPUT as u64,
    )?)
    .map_err(|e| e.to_string())?;
    if receipt.config != *config
        || receipt.source_sha256 != digest(source.as_bytes())
        || receipt.object_sha256 != digest(&object)
        || receipt.optimized_ir_sha256 != digest(optimized_ir.as_bytes())
        || object.is_empty()
        || !object.starts_with(b"\x7fELF")
    {
        return Err("LLVM worker receipt/payload identity mismatch".into());
    }
    // The selected library remains an artifact input at publication time, not
    // merely a claim copied from the worker's JSON.
    let mut file = File::open(&config.library).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != config.library_sha256 {
        return Err("LLVM library changed during compilation".into());
    }
    Ok(Compiled {
        object,
        optimized_ir,
        receipt,
    })
}
