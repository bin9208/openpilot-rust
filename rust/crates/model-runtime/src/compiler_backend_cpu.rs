//! Initial native ONNX-to-CPU artifact backend. Only expressions admitted by the
//! explicit elementwise renderer compile; unsupported graphs fail before spawning
//! a compiler or publishing a destination. This does not replace the full model,
//! image/fusion compiler or any Python consumer yet.
use crate::{
    compiler_onnx::{
        self,
        lower::{LoweredGraph, ScalarExpr},
        memory, render, schedule,
    },
    export::{self, Buffer, BufferTable},
    graph::{self, Graph},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::{CString, OsString},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub timeout: Duration,
    pub offline_bindings: bool,
    pub memory_planning: bool,
}
impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            program: "clang".into(),
            arguments: Vec::new(),
            timeout: Duration::from_secs(60),
            offline_bindings: false,
            memory_planning: true,
        }
    }
}
#[derive(Debug)]
pub struct Compilation {
    pub graph: Graph,
    pub model_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn require_space(path: &Path, growth: u64) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: statvfs initializes the output on success and retains no pointers.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: successful statvfs initialized the structure.
    let stats = unsafe { stats.assume_init() };
    let free = u128::from(stats.f_bavail) * u128::from(stats.f_frsize);
    if free < (25u128 << 30) + u128::from(growth) {
        return Err("native CPU compilation requires25GiB free plus estimated growth".into());
    }
    Ok(())
}
struct ChildGroup {
    child: Child,
}
impl Drop for ChildGroup {
    fn drop(&mut self) {
        if let Ok(pid) = i32::try_from(self.child.id()) {
            // SAFETY: the child was created in its own process group. Terminating that
            // group also cleans compiler/linker descendants; no pointer is involved.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn command(
    options: &CompileOptions,
    args: &[OsString],
    directory: &Path,
    output: &Path,
    limit: u64,
) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    require_space(
        directory,
        limit
            .checked_add(2 << 20)
            .ok_or("compiler growth overflow")?,
    )?;
    let stdout = directory.join("compiler.stdout");
    let stderr = directory.join("compiler.stderr");
    let mut process = Command::new(&options.program);
    process
        .args(&options.arguments)
        .args(args)
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).map_err(|e| e.to_string())?)
        .stderr(File::create(&stderr).map_err(|e| e.to_string())?)
        .process_group(0);
    let mut child = ChildGroup {
        child: process
            .spawn()
            .map_err(|e| format!("native CPU compiler process: {e}"))?,
    };
    let start = Instant::now();
    loop {
        for path in [&stdout, &stderr, output] {
            let length = fs::metadata(path).map(|v| v.len()).unwrap_or(0);
            let allowed = if path == output { limit } else { 1 << 20 };
            if length > allowed {
                return Err("native CPU compiler output limit exceeded".into());
            }
        }
        if let Some(status) = child.child.try_wait().map_err(|e| e.to_string())? {
            let error_bytes = read_bounded(&stderr, 1 << 20)?;
            let stdout_bytes = read_bounded(&stdout, 1 << 20)?;
            if !status.success() {
                let detail = if error_bytes.is_empty() {
                    &stdout_bytes
                } else {
                    &error_bytes
                };
                return Err(format!(
                    "native CPU compiler process {status}: {}",
                    String::from_utf8_lossy(detail)
                ));
            }
            let size = fs::metadata(output)
                .map_err(|e| format!("native CPU compiler output: {e}"))?
                .len();
            if size == 0 || size > limit {
                return Err("native CPU compiler returned empty or oversized output".into());
            }
            return Ok(());
        }
        if start.elapsed() >= options.timeout {
            return Err("native CPU compiler timeout".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("native CPU compiler output limit exceeded".into());
    }
    Ok(bytes)
}
fn part(bytes: &[u8], offset: usize, size: usize) -> Result<&[u8], String> {
    bytes
        .get(offset..offset.checked_add(size).ok_or("ELF range overflow")?)
        .ok_or("truncated native ELF output".into())
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        part(bytes, offset, 2)?.try_into().unwrap(),
    ))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        part(bytes, offset, 4)?.try_into().unwrap(),
    ))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        part(bytes, offset, 8)?.try_into().unwrap(),
    ))
}
fn elf_header(bytes: &[u8], kind: u16) -> Result<(), String> {
    let machine = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => return Err("native CPU compiler ELF architecture not implemented".into()),
    };
    if part(bytes, 0, 6)? != b"\x7fELF\x02\x01"
        || u16_at(bytes, 16)? != kind
        || u16_at(bytes, 18)? != machine
    {
        return Err("native CPU compiler output is not matching64-bit ELF".into());
    }
    part(bytes, 0, 64)?;
    Ok(())
}
fn verify_library(bytes: &[u8], kernels: usize) -> Result<(), String> {
    elf_header(bytes, 3)?;
    let start = usize::try_from(u64_at(bytes, 40)?).map_err(|_| "ELF section offset overflow")?;
    let count = u16_at(bytes, 60)? as usize;
    if count == 0 || u16_at(bytes, 58)? != 64 {
        return Err("unsupported native ELF section table".into());
    }
    let sections = part(
        bytes,
        start,
        count.checked_mul(64).ok_or("ELF section size overflow")?,
    )?;
    let mut symbols = BTreeSet::new();
    for header in sections.chunks_exact(64) {
        if u32_at(header, 4)? != 11 {
            continue;
        }
        if u64_at(header, 56)? != 24 {
            return Err("invalid native ELF dynamic symbols".into());
        }
        let link = u32_at(header, 40)? as usize;
        let strings_header = part(
            sections,
            link.checked_mul(64).ok_or("ELF string table overflow")?,
            64,
        )?;
        let strings = part(
            bytes,
            usize::try_from(u64_at(strings_header, 24)?)
                .map_err(|_| "ELF string offset overflow")?,
            usize::try_from(u64_at(strings_header, 32)?).map_err(|_| "ELF string size overflow")?,
        )?;
        let table = part(
            bytes,
            usize::try_from(u64_at(header, 24)?).map_err(|_| "ELF symbol offset overflow")?,
            usize::try_from(u64_at(header, 32)?).map_err(|_| "ELF symbol size overflow")?,
        )?;
        if table.len() % 24 != 0 || table.len() / 24 > 1_000_000 {
            return Err("native ELF symbol count limit".into());
        }
        for symbol in table.chunks_exact(24) {
            if symbol[4] & 15 != 2
                || symbol[4] >> 4 != 1
                || matches!(symbol[5] & 3, 1 | 2)
                || u16_at(symbol, 6)? == 0
            {
                continue;
            }
            let text = strings
                .get(u32_at(symbol, 0)? as usize..)
                .ok_or("ELF symbol name offset")?;
            let end = text
                .iter()
                .position(|v| *v == 0)
                .ok_or("unterminated ELF symbol name")?;
            symbols.insert(text[..end].to_vec());
        }
    }
    for index in 0..kernels {
        if !symbols.contains(format!("op_kernel_{index}").as_bytes()) {
            return Err(format!(
                "native CPU compiler output lacks op_kernel_{index}"
            ));
        }
    }
    Ok(())
}
fn initialized(graph: &LoweredGraph, id: usize) -> Option<&[u8]> {
    match &graph.plans[id].expression {
        ScalarExpr::Constant(v) => Some(v),
        _ => None,
    }
}
/// Compile a pinned ONNX object to the existing trusted CPU artifact ABI. All
/// compilation/linking is native and bounded; emission is atomic. Only embedded
/// tensor data is admitted: the protobuf pin cannot authorize external payloads.
pub fn compile_onnx(
    path: &Path,
    expected_sha256: &str,
    options: &CompileOptions,
    destination: &Path,
) -> Result<Compilation, String> {
    if options.program.as_os_str().is_empty()
        || options.arguments.iter().any(|v| v.is_empty())
        || options.timeout.is_zero()
        || options.timeout > Duration::from_secs(3600)
    {
        return Err("invalid native CPU compiler command/timeout".into());
    }
    if destination.exists() || destination.is_symlink() {
        return Err("CPU emission destination already exists".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(4 * 1024 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 * 1024 {
        return Err("ONNX source exceeds4GiB".into());
    }
    let identity = digest(&bytes);
    if identity != expected_sha256 {
        return Err("ONNX model identity differs from the pinned object".into());
    }
    // No origin is supplied intentionally: external_data requires separately pinned
    // payload identities, which this partial backend API does not yet accept.
    let model = compiler_onnx::Model::parse(&bytes)?;
    // Original CPU:LLVM NaN operand selection for the erf-based approximation
    // varies with layout/unrolling. Its polynomial's finite agreement alone is
    // insufficient to publish an equivalent artifact; retain fail-closed admission.
    if model.graph.nodes.iter().any(|node| node.op == "Gelu" && node.attributes.get("approximate").is_none_or(|attribute| matches!(attribute, compiler_onnx::Attribute::String(value) if value == "none"))) {
        return Err("CPU Gelu approximate=none layout-sensitive NaN propagation is not implemented".into());
    }
    let mut lowered = if options.offline_bindings {
        LoweredGraph::lower_offline(&model)
    } else {
        LoweredGraph::lower(&model)
    }?;
    if options.offline_bindings {
        compiler_onnx::storage::apply_cpu_image_aliases(&mut lowered, &model)?;
    }
    let physical = compiler_onnx::storage_half::physical_graph(&lowered)?;
    let schedule = schedule::schedule_plans(&physical, "CPU", true)?;
    let memory =
        memory::plan_memory_enabled(&schedule.buffers, &schedule.calls, options.memory_planning)?;
    let mut kernels = Vec::new();
    for (index, call) in schedule.calls.iter().enumerate() {
        let kernel = render::render_cpu_elementwise(
            &physical,
            call.buffers[0],
            &format!("native_elementwise_{index}"),
        )?;
        if kernel.inputs != call.buffers[1..] {
            return Err("renderer/schedule input order mismatch".into());
        }
        kernels.push(kernel);
    }
    if kernels.is_empty() {
        return Err("native CPU compilation currently requires a rendered expression".into());
    }
    let descriptors = (0..lowered.plans.len())
        .map(|id| {
            if let Some(view) = memory.views.get(&id) {
                Buffer {
                    base: view.arena as u64,
                    allocation_bytes: memory.arenas[view.arena].bytes,
                    offset: view.offset,
                    bytes: view.bytes,
                }
            } else {
                Buffer {
                    base: (memory.arenas.len() + id) as u64,
                    allocation_bytes: schedule.buffers[id].bytes,
                    offset: 0,
                    bytes: schedule.buffers[id].bytes,
                }
            }
        })
        .collect::<Vec<_>>();
    let mut table = BufferTable::default();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();

    for &id in &schedule.live {
        if let ScalarExpr::Input(name) = &lowered.plans[id].expression {
            let view = table.view(descriptors[id])?;
            table.write(descriptors[id])?;
            inputs.push(graph::Binding {
                name: name.clone(),
                view,
            });
        }
    }
    for (name, &id) in &lowered.outputs {
        let view = table.view(descriptors[id])?;
        outputs.push(graph::Binding {
            name: name.clone(),
            view,
        });
    }
    let mut calls = Vec::new();
    for (index, (call, kernel)) in schedule.calls.iter().zip(&kernels).enumerate() {
        let output = call.buffers[0];
        let mut args = vec![table.view(descriptors[output])?];
        for &id in &kernel.inputs {
            table.read_initialized(descriptors[id], initialized(&lowered, id))?;
            args.push(table.view(descriptors[id])?);
        }
        table.write(descriptors[output])?;
        calls.push(graph::Call {
            kernel: index,
            views: args,
            scalars: Vec::new(),
            workers: 1,
            core_id: None,
        });
    }
    for &id in lowered.outputs.values() {
        table.read_initialized(descriptors[id], initialized(&lowered, id))?;
    }
    let (allocations, weights) = table.weights();
    let graph = Graph {
        version: 1,
        backend: "cpu-clang".into(),
        arch: std::env::consts::ARCH.into(),
        weights_sha256: digest(&weights),
        library_sha256: "0".repeat(64),
        allocations: allocations
            .into_iter()
            .map(|a| graph::Allocation {
                bytes: a.bytes,
                weight_offset: a.weight_offset,
            })
            .collect(),
        views: table
            .views
            .into_iter()
            .map(|v| graph::View {
                allocation: v.allocation,
                offset: v.offset,
                bytes: v.bytes,
            })
            .collect(),
        inputs,
        outputs,
        kernels: kernels
            .iter()
            .map(|k| graph::Kernel {
                buffers: k.inputs.len() + 1,
                scalars: 0,
            })
            .collect(),
        calls,
        entrypoints: Vec::new(),
    };
    Graph::parse(&serde_json::to_vec(&graph).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut objects = Vec::new();
    let mut wrappers = Vec::new();
    let mut sources = Vec::new();
    let mut object_paths = Vec::new();
    let mut source_bytes = 0usize;
    for (index, kernel) in kernels.iter().enumerate() {
        let cpu_storage = |dtype| {
            if dtype == 10 {
                Ok("ushort")
            } else {
                render::storage_type(dtype)
            }
        };
        let output_type = cpu_storage(physical.plans[schedule.calls[index].buffers[0]].ty.dtype)?;
        let mut args = vec![format!("({output_type}*)buffers[0]")];
        for (slot, &input) in kernel.inputs.iter().enumerate() {
            args.push(format!(
                "(const {}*)buffers[{}]",
                cpu_storage(physical.plans[input].ty.dtype)?,
                slot + 1
            ));
        }
        let wrapper=format!("#include <stddef.h>\ntypedef unsigned short ushort; typedef unsigned char uchar;\n/* ONNX SHA256 {identity}; offline_bindings={}; memory_planning={} */\n#define __kernel\n#define __global\nstatic _Thread_local size_t native_index;\n#define get_global_id(axis) native_index\n{}\n__attribute__((visibility(\"default\"))) void op_kernel_{index}(void *const *buffers,const int *scalars){{(void)scalars;for(native_index=0;native_index<{};native_index++){}({});}}\n",options.offline_bindings,options.memory_planning,kernel.source,kernel.global[0],kernel.function,args.join(","));
        source_bytes = source_bytes
            .checked_add(wrapper.len())
            .filter(|v| *v <= 64 * 1024 * 1024)
            .ok_or("native CPU compiler sources exceed64MiB")?;
        let source = temporary.path().join(format!("kernel-{index}.c"));
        let object = temporary.path().join(format!("kernel-{index}.o"));
        require_space(temporary.path(), (64u64 << 20) + wrapper.len() as u64)?;
        fs::write(&source, &wrapper).map_err(|e| e.to_string())?;
        let args = [
            "-c",
            "-fPIC",
            "-O2",
            "-fno-fast-math",
            "-ffp-contract=off",
            "-x",
            "c",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([
            source.as_os_str().to_owned(),
            "-o".into(),
            object.as_os_str().to_owned(),
        ])
        .collect::<Vec<_>>();
        command(options, &args, temporary.path(), &object, 64 << 20)?;
        let object_bytes = read_bounded(&object, 64 << 20)?;
        elf_header(&object_bytes, 1)?;
        objects.push(object_bytes);
        object_paths.push(object);
        wrappers.push(wrapper);
        sources.push(kernel.source.clone());
    }
    let library = temporary.path().join("kernels.so");
    let args = [
        OsString::from("-shared"),
        "-o".into(),
        library.as_os_str().to_owned(),
    ]
    .into_iter()
    .chain(object_paths.iter().map(|v| v.as_os_str().to_owned()))
    .chain([OsString::from("-lm")])
    .collect::<Vec<_>>();
    command(options, &args, temporary.path(), &library, 512 << 20)?;
    let library = read_bounded(&library, 512 << 20)?;
    verify_library(&library, kernels.len())?;
    let mut graph = graph;
    graph.library_sha256 = digest(&library);
    export::emit_cpu(
        destination,
        &graph,
        &library,
        &weights,
        &sources,
        &objects,
        &wrappers,
    )
    .map_err(|e| e.to_string())?;
    Ok(Compilation {
        graph,
        model_sha256: identity,
    })
}
