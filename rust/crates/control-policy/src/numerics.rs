use crate::Error;
use libloading::Library;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, CStr},
    fs,
    path::Path,
};

type Sgemv = unsafe extern "C" fn(
    i32,
    i32,
    i64,
    i64,
    f32,
    *const f32,
    i64,
    *const f32,
    i64,
    f32,
    *mut f32,
    i64,
);
type Dgemv = unsafe extern "C" fn(
    i32,
    i32,
    i64,
    i64,
    f64,
    *const f64,
    i64,
    *const f64,
    i64,
    f64,
    *mut f64,
    i64,
);
type Sdot = unsafe extern "C" fn(i64, *const f32, i64, *const f32, i64) -> f32;
type Ddot = unsafe extern "C" fn(i64, *const f64, i64, *const f64, i64) -> f64;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    numpy: String,
    abi: String,
    library: String,
    files: Vec<Artifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    name: String,
    sha256: String,
}

pub struct Numerics {
    _library: Library,
    sgemv: Sgemv,
    dgemv: Dgemv,
    sdot: Sdot,
    ddot: Ddot,
}
impl Numerics {
    /// #553 전용: 계산/출력 flush 뒤 이미 소유한 Library에서만 관측한다.
    #[cfg(feature = "diagnostic-553")]
    pub fn diagnostic_553_getters(&self, directory: &Path) -> Result<(), Error> {
        #[cfg(target_os = "linux")]
        {
            diagnostic_553::observe(self, directory)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = directory;
            Err(Error::Contract("#553 getter requires Linux"))
        }
    }

    pub fn load(directory: &Path) -> Result<Self, Error> {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
        if manifest.format != 1
            || manifest.numpy != "2.5.3"
            || manifest.abi != "scipy_dgesdd_64_"
            || !manifest
                .files
                .iter()
                .any(|file| file.name == manifest.library)
        {
            return Err(Error::Contract(
                "NumPy 2.5.3 ILP64 numerical artifact required",
            ));
        }
        for file in &manifest.files {
            if Path::new(&file.name).components().count() != 1
                || file.name == "."
                || file.name == ".."
            {
                return Err(Error::Contract("numerical artifact filename"));
            }
            if format!(
                "{:x}",
                Sha256::digest(fs::read(directory.join(&file.name))?)
            ) != file.sha256
            {
                return Err(Error::Contract("numerical artifact hash mismatch"));
            }
        }
        // SAFETY: selected artifact is trusted native code; the owned library outlives
        // all function pointers, whose signatures match the pinned OpenBLAS ILP64 ABI.
        unsafe {
            let library = Library::new(directory.join(&manifest.library))?;
            let config = library.get::<unsafe extern "C" fn() -> *const c_char>(
                b"scipy_openblas_get_config64_\0",
            )?;
            let config = CStr::from_ptr(config()).to_string_lossy();
            if !config.contains("OpenBLAS 0.3.34.106.0") || !config.contains("USE64BITINT") {
                return Err(Error::Contract("numerical artifact version/ABI mismatch"));
            }
            let sgemv = *library.get::<Sgemv>(b"scipy_cblas_sgemv64_\0")?;
            let dgemv = *library.get::<Dgemv>(b"scipy_cblas_dgemv64_\0")?;
            let sdot = *library.get::<Sdot>(b"scipy_cblas_sdot64_\0")?;
            let ddot = *library.get::<Ddot>(b"scipy_cblas_ddot64_\0")?;
            Ok(Self {
                _library: library,
                sgemv,
                dgemv,
                sdot,
                ddot,
            })
        }
    }
    pub fn float_matrix(
        &self,
        input: &[f32],
        columns: usize,
        weights: &[f32],
    ) -> Result<Vec<f32>, Error> {
        let (rows, columns_i64) = dimensions(input.len(), columns, weights.len())?;
        let mut output = vec![0.; columns];
        // SAFETY: checked weights have rows*columns elements in Fortran order;
        // input rows and output columns are distinct owned buffers, unit strides.
        unsafe {
            if columns == 1 {
                output[0] = (self.sdot)(rows, input.as_ptr(), 1, weights.as_ptr(), 1);
            } else {
                (self.sgemv)(
                    102,
                    112,
                    rows,
                    columns_i64,
                    1.,
                    weights.as_ptr(),
                    rows,
                    input.as_ptr(),
                    1,
                    0.,
                    output.as_mut_ptr(),
                    1,
                );
            }
        }
        Ok(output)
    }
    pub fn double_matrix(
        &self,
        input: &[f64],
        columns: usize,
        weights: &[f64],
    ) -> Result<Vec<f64>, Error> {
        let (rows, columns_i64) = dimensions(input.len(), columns, weights.len())?;
        let mut output = vec![0.; columns];
        // SAFETY: checked weights have rows*columns elements in C order;
        // input rows and output columns are distinct owned buffers, unit strides.
        unsafe {
            if columns == 1 {
                output[0] = (self.ddot)(rows, input.as_ptr(), 1, weights.as_ptr(), 1);
            } else {
                (self.dgemv)(
                    101,
                    112,
                    rows,
                    columns_i64,
                    1.,
                    weights.as_ptr(),
                    columns_i64,
                    input.as_ptr(),
                    1,
                    0.,
                    output.as_mut_ptr(),
                    1,
                );
            }
        }
        Ok(output)
    }
}
fn dimensions(rows: usize, columns: usize, size: usize) -> Result<(i64, i64), Error> {
    if rows == 0
        || columns == 0
        || rows > 4096
        || columns > 4096
        || rows.checked_mul(columns) != Some(size)
    {
        return Err(Error::Contract("neural matrix dimensions"));
    }
    Ok((
        i64::try_from(rows).map_err(|_| Error::Contract("matrix rows"))?,
        i64::try_from(columns).map_err(|_| Error::Contract("matrix columns"))?,
    ))
}

// 기존 production loader/수치 본문에는 관측을 넣지 않는다.
#[cfg(all(feature = "diagnostic-553", target_os = "linux"))]
mod diagnostic_553 {
    use super::{Error, Numerics};
    use serde::Serialize;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        ffi::{c_char, c_int, c_void},
        fs::{self, File, OpenOptions},
        io::{Read, Write},
        os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt},
        path::Path,
    };

    const LIBRARY: &str = "libscipy_openblas64_-f48b354e.so";
    const PIN: &str = "432e1c1739731468ba305771af96773939e43016643e2d630f58cb45b6670e96";
    const STRING_BOUND: usize = 4096;
    const RECORD_BOUND: usize = 64 * 1024;

    #[repr(C)]
    struct DlInfo {
        name: *const c_char,
        base: *mut c_void,
        symbol: *const c_char,
        address: *mut c_void,
    }
    // Linux/glibc 고정 ABI. 추가 dlopen/ptrace/BLAS 계산은 하지 않는다.
    unsafe extern "C" {
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
        fn gnu_dev_major(device: u64) -> u32;
        fn gnu_dev_minor(device: u64) -> u32;
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    struct Mapping {
        start: usize,
        end: usize,
        permissions: String,
        offset: usize,
        major: u32,
        minor: u32,
        inode: u64,
        path: String,
    }

    fn check(ok: bool, message: &'static str) -> Result<(), Error> {
        if ok {
            Ok(())
        } else {
            Err(Error::Contract(message))
        }
    }
    fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Error> {
        let mut raw = Vec::new();
        File::open(path)?
            .take(limit as u64 + 1)
            .read_to_end(&mut raw)?;
        check(raw.len() <= limit, "#553 proc input bound")?;
        Ok(raw)
    }
    fn maps() -> Result<Vec<Mapping>, Error> {
        let raw = bounded(Path::new("/proc/self/maps"), 2 * 1024 * 1024)?;
        let text = std::str::from_utf8(&raw).map_err(|_| Error::Contract("#553 maps UTF8"))?;
        text.lines()
            .map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                check(fields.len() >= 5, "#553 maps fields")?;
                let (start, end) = fields[0]
                    .split_once('-')
                    .ok_or(Error::Contract("#553 maps range"))?;
                let (major, minor) = fields[3]
                    .split_once(':')
                    .ok_or(Error::Contract("#553 maps device"))?;
                let number = |value| {
                    usize::from_str_radix(value, 16).map_err(|_| Error::Contract("#553 maps hex"))
                };
                let start = number(start)?;
                let end = number(end)?;
                check(
                    start < end && fields[1].len() == 4,
                    "#553 maps range/permissions",
                )?;
                Ok(Mapping {
                    start,
                    end,
                    permissions: fields[1].into(),
                    offset: number(fields[2])?,
                    major: u32::try_from(number(major)?)
                        .map_err(|_| Error::Contract("#553 major"))?,
                    minor: u32::try_from(number(minor)?)
                        .map_err(|_| Error::Contract("#553 minor"))?,
                    inode: fields[4]
                        .parse()
                        .map_err(|_| Error::Contract("#553 maps inode"))?,
                    path: fields.get(5..).unwrap_or_default().join(" "),
                })
            })
            .collect()
    }
    fn identity(info: &fs::Metadata) -> Value {
        json!([
            info.dev(),
            info.ino(),
            info.mode(),
            info.uid(),
            info.gid(),
            info.size(),
            info.nlink(),
            info.mtime(),
            info.mtime_nsec(),
            info.ctime(),
            info.ctime_nsec()
        ])
    }
    fn digest(file: &File) -> Result<String, Error> {
        let mut hash = Sha256::new();
        let mut block = [0_u8; 64 * 1024];
        let mut offset = 0;
        loop {
            let count = file.read_at(&mut block, offset)?;
            if count == 0 {
                break;
            }
            offset += count as u64;
            check(offset <= 32 * 1024 * 1024, "#553 provider file bound")?;
            hash.update(&block[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    fn owner<'a>(
        rows: &'a [Mapping],
        address: usize,
        path: &str,
        device: (u32, u32),
        inode: u64,
        executable: bool,
    ) -> Result<&'a Mapping, Error> {
        let selected = rows
            .iter()
            .filter(|row| row.start <= address && address < row.end)
            .collect::<Vec<_>>();
        check(selected.len() == 1, "#553 ambiguous/unmapped pointer")?;
        let row = selected[0];
        check(
            row.path == path
                && (row.major, row.minor) == device
                && row.inode == inode
                && row.permissions.as_bytes()[if executable { 2 } else { 0 }]
                    == if executable { b'x' } else { b'r' },
            "#553 pointer provider identity",
        )?;
        Ok(row)
    }
    fn base(address: usize) -> Result<usize, Error> {
        let mut info = DlInfo {
            name: std::ptr::null(),
            base: std::ptr::null_mut(),
            symbol: std::ptr::null(),
            address: std::ptr::null_mut(),
        };
        // SAFETY: dladdr only queries an address already selected from an owned Library.
        check(
            unsafe { dladdr(address as *const c_void, &mut info) } != 0 && !info.base.is_null(),
            "#553 dladdr failed",
        )?;
        Ok(info.base as usize)
    }
    fn nul(raw: &[u8]) -> Result<Vec<u8>, Error> {
        let end = raw
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(Error::Contract("#553 getter string not terminated"))?;
        check(end > 0, "#553 empty getter string")?;
        Ok(raw[..end].to_vec())
    }
    fn string(
        pointer: *const c_char,
        rows: &[Mapping],
        path: &str,
        device: (u32, u32),
        inode: u64,
        library_base: usize,
    ) -> Result<Vec<u8>, Error> {
        check(!pointer.is_null(), "#553 NULL getter string")?;
        let selected = rows
            .iter()
            .filter(|row| row.start <= pointer as usize && (pointer as usize) < row.end)
            .collect::<Vec<_>>();
        check(
            selected.len() == 1
                && selected[0].permissions.as_bytes()[0] == b'r'
                && base(pointer as usize)? == library_base,
            "#553 getter string instance/mapping",
        )?;
        let row = selected[0];
        // ELF BSS는 inode0 익명 map일 수 있다. dladdr가 같은 image base를 증명해야 한다.
        if row.inode != 0 || !row.path.is_empty() {
            owner(rows, pointer as usize, path, device, inode, false)?;
        }
        let length = STRING_BOUND.min(row.end - pointer as usize);
        // SAFETY: pinned owned Library stays alive; readable mapping bounds were checked.
        nul(unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) })
    }
    fn executable(rows: &[Mapping]) -> Vec<&Mapping> {
        rows.iter()
            .filter(|row| row.permissions.as_bytes()[2] == b'x' && row.path.starts_with('/'))
            .collect()
    }
    fn tasks() -> Result<Vec<(u32, String)>, Error> {
        let mut rows = Vec::new();
        for entry in fs::read_dir("/proc/self/task")? {
            check(rows.len() < 1024, "#553 task roster bound")?;
            let path = entry?.path();
            let tid = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.parse().ok())
                .ok_or(Error::Contract("#553 task id"))?;
            let raw = bounded(&path.join("stat"), 64 * 1024)?;
            let text = std::str::from_utf8(&raw).map_err(|_| Error::Contract("#553 stat UTF8"))?;
            let (_, rest) = text
                .rsplit_once(") ")
                .ok_or(Error::Contract("#553 stat fields"))?;
            let start = rest
                .split_whitespace()
                .nth(19)
                .ok_or(Error::Contract("#553 stat starttime"))?;
            check(
                start.bytes().all(|b| b.is_ascii_digit()),
                "#553 stat starttime",
            )?;
            rows.push((tid, start.into()));
        }
        rows.sort_unstable();
        Ok(rows)
    }
    fn collect(kernel: &Numerics, directory: &Path) -> Result<Value, Error> {
        check(
            directory.is_absolute() && directory.canonicalize()? == directory,
            "#553 canonical numerics directory",
        )?;
        let path = directory.join(LIBRARY);
        check(path.canonicalize()? == path, "#553 canonical provider")?;
        // Linux O_NOFOLLOW|O_NONBLOCK; std File also owns a CLOEXEC descriptor.
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(0x20000 | 0x800)
            .open(&path)?;
        let metadata = file.metadata()?;
        let before = identity(&metadata);
        check(
            metadata.is_file()
                && identity(&fs::symlink_metadata(&path)?) == before
                && digest(&file)? == PIN,
            "#553 provider FD/pin",
        )?;
        // SAFETY: these fixed glibc helpers only decode the recorded dev_t.
        let device = unsafe { (gnu_dev_major(metadata.dev()), gnu_dev_minor(metadata.dev())) };
        let path_text = path.to_str().ok_or(Error::Contract("#553 provider path"))?;
        let before_maps = maps()?;
        let before_tasks = tasks()?;
        // SAFETY: ABI from pinned OpenBLAS exports/header. No new Library is opened.
        let (config, core, threads) = unsafe {
            (
                *kernel
                    ._library
                    .get::<unsafe extern "C" fn() -> *const c_char>(
                        b"scipy_openblas_get_config64_\0",
                    )?,
                *kernel
                    ._library
                    .get::<unsafe extern "C" fn() -> *const c_char>(
                        b"scipy_openblas_get_corename64_\0",
                    )?,
                *kernel._library.get::<unsafe extern "C" fn() -> c_int>(
                    b"scipy_openblas_get_num_threads64_\0",
                )?,
            )
        };
        let addresses = [
            kernel.sgemv as usize,
            kernel.dgemv as usize,
            kernel.sdot as usize,
            kernel.ddot as usize,
            config as usize,
            core as usize,
            threads as usize,
        ];
        let library_base = base(addresses[0])?;
        for address in addresses {
            owner(
                &before_maps,
                address,
                path_text,
                device,
                metadata.ino(),
                true,
            )?;
            check(
                base(address)? == library_base,
                "#553 different loaded provider instance",
            )?;
        }
        check(
            before_maps
                .iter()
                .any(|row| row.start == library_base && row.offset == 0 && row.path == path_text),
            "#553 provider base mapping",
        )?;
        // Calls are deliberately after all case calculations and output flush.
        let config_bytes = string(
            unsafe { config() },
            &before_maps,
            path_text,
            device,
            metadata.ino(),
            library_base,
        )?;
        let core_bytes = string(
            unsafe { core() },
            &before_maps,
            path_text,
            device,
            metadata.ino(),
            library_base,
        )?;
        let thread_count = unsafe { threads() };
        check(thread_count > 0, "#553 invalid thread count")?;
        let after_maps = maps()?;
        let after_tasks = tasks()?;
        check(
            executable(&before_maps) == executable(&after_maps),
            "#553 executable mapping roster changed",
        )?;
        check(
            identity(&file.metadata()?) == before
                && identity(&fs::symlink_metadata(&path)?) == before
                && digest(&file)? == PIN,
            "#553 provider changed after getters",
        )?;
        let pid = std::process::id();
        let start = before_tasks
            .iter()
            .find(|(tid, _)| *tid == pid)
            .ok_or(Error::Contract("#553 missing own task"))?;
        check(
            after_tasks.contains(start),
            "#553 own task identity changed",
        )?;
        Ok(
            json!({"library_handle_source":"borrowed Numerics::_library", "new_library_opened":false,
            "provider":{"path":path,"identity":before,"sha256":PIN,"load_base":library_base},
            "symbol_addresses":addresses,"config_bytes":config_bytes,"core_bytes":core_bytes,"threads":thread_count,
            "executable_mappings_before":executable(&before_maps),"executable_mappings_after":executable(&after_maps),
            "tasks_before":before_tasks,"tasks_after":after_tasks,
            "provider_fd_held_through_observation":true,"executable_mapping_roster_equal":true,
            "process_start_ticks":start.1,"thread_lifecycle_complete":false,"load_already_called_config":true,
            "cpu_environment": (["OPENBLAS_NUM_THREADS","OMP_NUM_THREADS","OPENBLAS_CORETYPE","NPY_DISABLE_CPU_FEATURES"].map(|key|(key,std::env::var(key).ok())).into_iter().collect::<BTreeMap<_,_>>()),
            "string_pointer_scope":"same dladdr image base; readable file mapping or anonymous BSS"}),
        )
    }
    pub(super) fn observe(kernel: &Numerics, directory: &Path) -> Result<(), Error> {
        let mut result = collect(kernel, directory);
        let mut record = json!({"schema":553,"diagnostic_only":true,"observer":"native-tapped",
            "phase":"after-output-flush","pid":std::process::id(),"source":std::env::var("GITHUB_SHA").ok(),
            "actual_kernel_observed":false,"actual_blas_call_observed":false,"complete_provider_closure":false,
            "historical_provider_closure_recorded":false,"full_normal40_acceptance":false,"retirement_credit":0,
            "original_expected_regenerated":false,"pass":result.is_ok()});
        match &result {
            Ok(value) => record["observation"] = value.clone(),
            Err(error) => record["error"] = error.to_string().into(),
        }
        let mut raw = serde_json::to_vec(&record)?;
        if raw.len() > RECORD_BOUND {
            record
                .as_object_mut()
                .ok_or(Error::Contract("#553 getter record"))?
                .remove("observation");
            record["pass"] = false.into();
            record["error"] = "#553 getter record bound".into();
            result = Err(Error::Contract("#553 getter record bound"));
            raw = serde_json::to_vec(&record)?;
        }
        check(
            raw.len() <= RECORD_BOUND,
            "#553 getter failure record bound",
        )?;
        let mut stderr = std::io::stderr().lock();
        stderr.write_all(b"CONTROLSD_553_GETTERS ")?;
        stderr.write_all(&raw)?;
        stderr.write_all(b"\n")?;
        stderr.flush()?;
        result.map(|_| ())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn getter_pointer_requires_unique_mapping_and_exact_provider_identity() {
            let row = Mapping {
                start: 4096,
                end: 8192,
                permissions: "r-xp".into(),
                offset: 0,
                major: 8,
                minor: 1,
                inode: 7,
                path: "/owned/lib.so".into(),
            };
            assert!(owner(
                std::slice::from_ref(&row),
                5000,
                "/owned/lib.so",
                (8, 1),
                7,
                true
            )
            .is_ok());
            for (path, device, inode) in [
                ("/other/lib.so", (8, 1), 7),
                ("/owned/lib.so", (8, 2), 7),
                ("/owned/lib.so", (8, 1), 8),
            ] {
                assert!(
                    owner(std::slice::from_ref(&row), 5000, path, device, inode, true).is_err()
                );
            }
            assert!(owner(
                &[row.clone(), row.clone()],
                5000,
                "/owned/lib.so",
                (8, 1),
                7,
                true
            )
            .is_err());
            assert!(owner(&[row], 8192, "/owned/lib.so", (8, 1), 7, true).is_err());
        }
        #[test]
        fn getter_strings_require_nonempty_bounded_terminator() {
            assert_eq!(nul(b"ZEN\0ignored").unwrap(), b"ZEN");
            assert!(nul(b"\0").is_err());
            assert!(nul(&vec![b'x'; STRING_BOUND]).is_err());
        }
    }
}
