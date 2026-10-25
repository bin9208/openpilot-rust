//! Process titles for fresh, owned GNU/Linux QA executables only.
//!
//! glibc supplies the real startup argv/envp to ELF preinitializers. Before DSO
//! constructors, Rust argument initialization, or application threads, we move
//! every string behind those pointer tables to stable process-lifetime storage.
//! The now-unreferenced original contiguous kernel region remains available for
//! a later title change. Merely linking/claiming this opt-in API does not rename
//! the process. `set` belongs at the source launcher's Name stage, after prepare.
//!
//! This deliberately uses neither /proc/self/mem nor guessed stack addresses.
//! It is not a general DSO API or a production-daemon title policy. The feature
//! performs relocation only in the explicit QA peer or `check-managed-entry
//! --managed-source` invocation. Other binaries/modes return before mutation,
//! even if Cargo unifies the feature into them. Supported QA executables use
//! the standard system allocator/startup ABI; the default build has no callback.
use std::{
    ffi::{c_char, c_int, CStr, CString},
    io,
    marker::PhantomData,
    ptr,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering},
};

static REGION: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());
static WINDOW: AtomicUsize = AtomicUsize::new(0);
static INITIAL_PID: AtomicU32 = AtomicU32::new(0);
static CLAIMED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    static mut environ: *mut *mut c_char;
    static mut program_invocation_name: *mut c_char;
    static mut program_invocation_short_name: *mut c_char;
    fn prctl(option: c_int, ...) -> c_int;
}

// Like the argc/argv extension used by Rust std on this platform, this callback
// receives glibc's startup pointers. Preinit precedes ordinary DSO constructors
// and std's .init_array.00099 argv registration, so there are no application
// references or application-created threads while pointer tables are changed.
#[used]
#[unsafe(link_section = ".preinit_array")]
static PREINITIALIZE: extern "C" fn(c_int, *mut *mut c_char, *mut *mut c_char) = initialize;

extern "C" fn initialize(argc: c_int, argv: *mut *mut c_char, envp: *mut *mut c_char) {
    if argc <= 0 || argv.is_null() || envp.is_null() {
        return;
    }
    // SAFETY: the GNU/Linux executable preinit ABI supplies writable arrays of
    // argc+1 argv pointers and NULL-terminated envp pointers, each non-NULL entry
    // a valid NUL-terminated startup string. Both strings and pointer tables live
    // for this process. This callback runs once, before application threads and
    // Rust's deferred argument readers can access them. No reference to the old
    // strings escapes this callback; all replacement strings are intentionally
    // leaked and remain immutable for the rest of the process lifetime.
    unsafe {
        let args = std::slice::from_raw_parts_mut(argv, argc as usize);
        if args.iter().any(|argument| argument.is_null()) {
            return;
        }
        // Cargo can unify this QA feature into unrelated workspace executables.
        // Before allocating, rebinding argv/envp, or changing any global pointer,
        // admit only our mechanical peer's naming mode or the real source child.
        // In particular the outer checker, collector and --request worker do not
        // initialize title storage even though they share the same executable.
        let name = CStr::from_ptr(args[0])
            .to_bytes()
            .rsplit(|byte| *byte == b'/')
            .next()
            .unwrap_or_default();
        let mode = args.get(1).map(|value| CStr::from_ptr(*value).to_bytes());
        let allowed = (name == b"qa-source-title-peer" && mode != Some(b"--other-mode"))
            || (name == b"check-managed-entry" && mode == Some(b"--managed-source"));
        if !allowed {
            return;
        }
        let first = args[0].cast::<u8>();
        let mut end = first as usize;
        for &argument in args.iter() {
            if argument as usize != end {
                break;
            }
            let length = CStr::from_ptr(argument).to_bytes_with_nul().len();
            let Some(next) = end.checked_add(length) else {
                return;
            };
            end = next;
        }
        let mut env_count = 0usize;
        let mut noenv = false;
        while !envp.add(env_count).read().is_null() {
            let bytes = CStr::from_ptr(envp.add(env_count).read()).to_bytes();
            if let Some(value) = bytes.strip_prefix(b"SPT_NOENV=") {
                noenv = !value.is_empty();
            }
            env_count += 1;
        }
        if !noenv {
            for index in 0..env_count {
                let value = envp.add(index).read();
                if value as usize == end {
                    let length = CStr::from_ptr(value).to_bytes_with_nul().len();
                    let Some(next) = end.checked_add(length) else {
                        return;
                    };
                    end = next;
                }
            }
        }
        let Some(window) = end.checked_sub(first as usize).filter(|value| *value > 0) else {
            return;
        };
        // Replace the original tables' entries as well as environ: libc's later
        // startup still receives these original table addresses, and std stores
        // the original argv table for deferred calls to std::env::args().
        for argument in args.iter_mut() {
            *argument = CStr::from_ptr(*argument).to_owned().into_raw();
        }
        for index in 0..env_count {
            let cell = envp.add(index);
            cell.write(CStr::from_ptr(cell.read()).to_owned().into_raw());
        }
        environ = envp;
        program_invocation_name = args[0];
        let basename = CStr::from_ptr(args[0])
            .to_bytes()
            .iter()
            .rposition(|byte| *byte == b'/')
            .map_or(0, |index| index + 1);
        program_invocation_short_name = args[0].add(basename);
        REGION.store(first, Ordering::Relaxed);
        INITIAL_PID.store(std::process::id(), Ordering::Relaxed);
        WINDOW.store(window, Ordering::Release);
    }
}

fn main_thread() -> io::Result<()> {
    let task = std::fs::read_link("/proc/thread-self")?;
    let tid = task
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.parse::<u32>().ok());
    if tid != Some(std::process::id()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "source process title belongs to the main thread",
        ));
    }
    Ok(())
}

/// Unique main-thread ownership of the quarantined startup title region.
///
/// This is neither Send nor Sync. Other application threads may safely read
/// argv/environment: startup has moved those readers to immutable copies.
/// Copies are deliberately retained until process exit, including after Drop.
/// Claiming does not change the observed cmdline/comm; dropping does not restore
/// an old name or alter any production daemon process.
pub struct SourceTitle {
    pid: u32,
    _main_thread: PhantomData<Rc<()>>,
}
impl SourceTitle {
    pub fn claim() -> io::Result<Self> {
        main_thread()?;
        let pid = std::process::id();
        if WINDOW.load(Ordering::Acquire) == 0 || INITIAL_PID.load(Ordering::Relaxed) != pid {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "source process title startup storage unavailable",
            ));
        }
        CLAIMED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| {
                io::Error::new(io::ErrorKind::AlreadyExists, "source title already owned")
            })?;
        Ok(Self {
            pid,
            _main_thread: PhantomData,
        })
    }

    /// Change the actual current process's kernel title and 15-byte comm.
    ///
    /// NUL is rejected before any effect. As in setproctitle1.3.7, the bounded
    /// contiguous startup window reserves its final two bytes for termination;
    /// SPT_NOENV limits that window to arguments. Truncation is by bytes.
    pub fn set(&mut self, title: &[u8]) -> io::Result<()> {
        let value = CString::new(title)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded null character"))?;
        main_thread()?;
        if self.pid != std::process::id() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "source title owner changed process",
            ));
        }
        let window = WINDOW.load(Ordering::Acquire);
        let size = value.as_bytes().len().min(window.saturating_sub(2));
        let display = CString::new(&value.as_bytes()[..size]).expect("validated NUL-free title");
        // SAFETY: display is a live NUL-terminated string for the syscall. PR_SET_NAME
        // copies at most 15 bytes into this calling main thread's comm. REGION was
        // obtained directly from writable startup argv, with its contiguous bounds
        // validated by initialize. That callback moved every application-visible
        // argv/env pointer away before any readers existed. Only this unique,
        // nontransferable owner writes the region; no Rust references to it exist.
        // It remains mapped through process exit. The validated size is within the
        // region and the independent CString allocation cannot overlap that region.
        unsafe {
            if prctl(15, display.as_ptr(), 0usize, 0usize, 0usize) != 0 {
                return Err(io::Error::last_os_error());
            }
            let region = REGION.load(Ordering::Relaxed);
            ptr::write_bytes(region, 0, window);
            ptr::copy_nonoverlapping(display.as_ptr().cast::<u8>(), region, size);
        }
        Ok(())
    }
}
impl Drop for SourceTitle {
    fn drop(&mut self) {
        CLAIMED.store(false, Ordering::Release);
    }
}
