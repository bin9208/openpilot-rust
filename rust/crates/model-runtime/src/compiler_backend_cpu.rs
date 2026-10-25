//! Bounded native ONNX-to-CPU artifact backend. Source admission precedes
//! contraction and physical scheduling; explicitly configured LLVM providers
//! can compile selected packed IMAGE residuals and final-output matrices. Unsupported graphs fail without
//! publishing a destination. This does not replace the full model compiler or
//! any Python consumer yet.
pub mod activation;
mod contraction;
pub mod convolution;
mod fused_mean;
pub mod image;
pub mod llvm;
pub mod matrix;
pub mod normalization;
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
    collections::{BTreeMap, BTreeSet},
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
    /// Explicit provider and isolated worker for IMAGE LLVM kernels. No host
    /// CPU/library guessing occurs when this optional backend is selected.
    pub llvm: Option<(llvm::Config, PathBuf)>,
    /// Explicit original CPU thread count for an experimental, source-proved
    /// fused Conv/Gelu/Mean candidate. None retains the structural fence.
    pub fused_mean_workers: Option<usize>,
}
impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            program: "clang".into(),
            arguments: Vec::new(),
            timeout: Duration::from_secs(60),
            offline_bindings: false,
            memory_planning: true,
            llvm: None,
            fused_mean_workers: None,
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
    if free < u128::from(growth) {
        return Err("native CPU compilation requires estimated operation growth".into());
    }
    Ok(())
}
struct ChildGroup {
    child: Child,
    reaped: bool,
}
impl ChildGroup {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        if self.reaped {
            return self.child.try_wait();
        }
        let mut status = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        // SAFETY: the output is writable; WNOWAIT observes without reaping, so
        // the child still reserves the process-group ID during cleanup.
        if unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                status.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } != 0
        {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ECHILD) {
                self.reaped = true;
            }
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(error);
        }
        // SAFETY: waitid initialized status; a zero PID means no exited child.
        if unsafe { status.assume_init().si_pid() } == 0 {
            return Ok(None);
        }
        self.terminate_group();
        let result = self.child.try_wait();
        if matches!(result, Ok(Some(_)))
            || result
                .as_ref()
                .err()
                .is_some_and(|error| error.raw_os_error() == Some(libc::ECHILD))
        {
            // A reaped leader no longer reserves its PID/process-group ID.
            // Never signal that number again after losing child ownership.
            self.reaped = true;
        }
        result
    }
    fn terminate_group(&self) {
        if let Ok(pid) = i32::try_from(self.child.id()) {
            // SAFETY: this unreaped child owns its dedicated process-group ID.
            // Kill descendants before reaping the leader releases that ID.
            unsafe { libc::kill(-pid, libc::SIGKILL) };
        }
    }
}
impl Drop for ChildGroup {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        self.terminate_group();
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
        reaped: false,
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
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
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
    // Whole-input Slice cancellation may leave a published output bound directly
    // to input/constant storage. Retain its deliberate movement kernel while
    // keeping the unselected Concat branches dead. Computed-source aliases keep
    // their existing materialized producer; arbitrary empty artifacts stay invalid.
    for node in model.graph.nodes.iter().filter(|node| node.op == "Slice") {
        for name in &node.outputs {
            let Some(&source) = lowered.outputs.get(name) else {
                continue;
            };
            if !matches!(
                lowered.plans[source].expression,
                ScalarExpr::Input(_) | ScalarExpr::Constant(_)
            ) {
                continue;
            }
            let output = lowered.plans.len();
            lowered.plans.push(compiler_onnx::lower::Plan {
                ty: lowered.plans[source].ty.clone(),
                expression: ScalarExpr::Load {
                    plan: source,
                    index: compiler_onnx::lower::IndexExpr::Index,
                },
            });
            lowered.outputs.insert(name.clone(), output);
        }
    }
    let live = schedule::schedule_plans(&lowered, "CPU", true)?.live;
    for node in model
        .graph
        .nodes
        .iter()
        .filter(|node| matches!(node.op.as_str(), "Concat" | "Relu"))
    {
        if !node
            .outputs
            .iter()
            .filter_map(|name| lowered.names.get(name))
            .any(|id| live.contains(id))
        {
            continue;
        }
        let inputs_proved = node.inputs.iter().all(|name| {
            lowered.names.get(name).is_some_and(|id| {
                let dtype = lowered.plans[*id].ty.dtype;
                dtype == 1 || node.op == "Concat" && dtype == 10
            })
        });
        let outputs_proved = node.outputs.iter().all(|name| {
            lowered.names.get(name).is_some_and(|id| {
                let dtype = lowered.plans[*id].ty.dtype;
                dtype == 1 || node.op == "Concat" && dtype == 10
            })
        });
        if !inputs_proved || !outputs_proved {
            return Err(
                "CPU conditional graph requires proved output and supported logical input dtypes"
                    .into(),
            );
        }
    }
    let matrices = if options.llvm.is_some() && options.offline_bindings {
        matrix::graph::closures(&lowered, &model)?
    } else {
        Vec::new()
    };
    let gemms = if options.llvm.is_some() && options.offline_bindings {
        matrix::gemm::closures(&lowered, &model)?
    } else {
        Vec::new()
    };
    let chains = if options.llvm.is_some() && options.offline_bindings {
        matrix::chain::closures(&lowered, &model)?
    } else {
        Vec::new()
    };
    let parallel = if options.llvm.is_some() && options.offline_bindings {
        contraction::parallel::closures(&lowered, &model)?
    } else {
        Vec::new()
    };
    let fused_means = if options.llvm.is_some() && options.offline_bindings {
        options
            .fused_mean_workers
            .map(|workers| fused_mean::closures(&lowered, &model, workers))
            .transpose()?
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for chain in &chains {
        if gemms.iter().any(|g| !g.owned.is_disjoint(&chain.owned))
            || matrices
                .iter()
                .any(|m| !chain.owned.is_disjoint(&m.owned()))
        {
            return Err("overlapping configured LLVM matrix chain ownership".into());
        }
    }
    let mut kernel_plans = matrices
        .iter()
        .flat_map(|m| m.owned())
        .chain(gemms.iter().flat_map(|g| g.owned.iter().copied()))
        .chain(chains.iter().flat_map(|g| g.owned.iter().copied()))
        .collect::<BTreeSet<_>>();
    for closure in &parallel {
        if !kernel_plans.is_disjoint(&closure.owned) {
            return Err("overlapping configured LLVM parallel Conv ownership".into());
        }
        kernel_plans.extend(&closure.owned);
    }
    for closure in &fused_means {
        if !kernel_plans.is_disjoint(&closure.owned) {
            return Err("overlapping configured LLVM fused Mean ownership".into());
        }
        kernel_plans.extend(&closure.owned);
    }
    if options.offline_bindings {
        compiler_onnx::storage::apply_cpu_image_aliases_with_matrices(
            &mut lowered,
            &model,
            &kernel_plans,
        )?;
    }
    let mut physical = compiler_onnx::storage_half::physical_graph(&lowered)?;
    // Source admission precedes optimization: contracting a computed divisor
    // must not obscure its original unsupported arithmetic ancestry.
    let source_schedule = schedule::schedule_plans(&physical, "CPU", true)?;
    for (index, call) in source_schedule.calls.iter().enumerate() {
        if kernel_plans.contains(&call.buffers[0]) {
            continue;
        }
        render::render_cpu_elementwise(&physical, call.buffers[0], &format!("admit_{index}"))?;
    }
    contraction::apply(&mut physical, &model, &kernel_plans)?;
    fused_mean::validate_physical(&fused_means, &physical)?;
    compiler_onnx::storage::admit_cpu_observable_fusion(
        &physical,
        &model,
        &fused_means.iter().map(|c| c.output()).collect(),
    )?;
    let mut schedule = schedule::schedule_plans(&physical, "CPU", true)?;
    let mut image_calls = BTreeMap::new();
    let mut extra_initial = BTreeMap::new();
    let mut scratch_buffers = BTreeSet::new();
    if options.llvm.is_some() && options.offline_bindings {
        for closure in image::residual_closures(&lowered, &model)? {
            // External residuals and independently held convolutions require
            // distinct original signatures; leave their established path intact.
            if closure.residual_storage != image::ResidualStorage::PackedHalf
                || closure.retain_convolution
            {
                continue;
            }
            let layout = closure.image.layout()?;
            let Some(raw_weights) = initialized(&lowered, closure.weights) else {
                return Err("IMAGE packed weights require initialized f32 source".into());
            };
            let mut packed = vec![0; layout.weight_elements * 2];
            closure.image.pack_weights_into(raw_weights, &mut packed)?;
            let scratch = schedule.buffers.len();
            schedule.buffers.push(memory::BufferSpec {
                device: "CPU".into(),
                bytes: layout.input_elements * 2,
                held: false,
                supports_offset: true,
            });
            scratch_buffers.insert(scratch);
            let weight = schedule.buffers.len();
            schedule.buffers.push(memory::BufferSpec {
                device: "CPU".into(),
                bytes: packed.len(),
                held: true,
                supports_offset: true,
            });
            extra_initial.insert(weight, packed);
            schedule
                .calls
                .retain(|c| c.buffers[0] != closure.convolution);
            let call = schedule
                .calls
                .iter_mut()
                .find(|c| c.buffers[0] == closure.output)
                .ok_or("IMAGE residual has no scheduled output call")?;
            call.buffers = vec![closure.output, closure.input, scratch, weight];
            image_calls.insert(closure.output, closure.image);
        }
    }
    let parallel_staged =
        contraction::parallel::insert(&parallel, &lowered, &mut schedule, &mut extra_initial)?;
    let fused_staged =
        fused_mean::insert(&fused_means, &lowered, &mut schedule, &mut extra_initial)?;
    let depthwise = if options.llvm.is_some() && options.offline_bindings {
        let fused_owned = fused_means
            .iter()
            .flat_map(|closure| closure.owned.iter().copied())
            .collect();
        convolution::stage(
            &physical,
            &model,
            &mut schedule,
            &mut extra_initial,
            &fused_owned,
        )?
    } else {
        Vec::new()
    };
    // Matrix scratch has separate scheduled producers. Unlike the bundled
    // IMAGE trampoline's private scratch, it must be accounted as produced
    // output before any later read; never pre-mark these buffers written.
    let mut llvm_calls = matrix::graph::stage_with_chains(
        &matrices,
        &gemms,
        &chains,
        &lowered,
        &mut schedule,
        &mut extra_initial,
        &mut BTreeSet::new(),
    )?;
    if options.llvm.is_some() {
        for (index, call) in contraction::parallel::scheduled(&parallel_staged, &schedule)? {
            if llvm_calls.insert(index, call).is_some() {
                return Err("overlapping configured LLVM parallel Conv schedule".into());
            }
        }
        for (index, call) in fused_mean::scheduled(&fused_staged, &schedule)? {
            if llvm_calls.insert(index, call).is_some() {
                return Err("overlapping configured LLVM fused Mean schedule".into());
            }
        }
        for (index, call) in convolution::scheduled(&depthwise, &schedule)? {
            if llvm_calls.insert(index, call).is_some() {
                return Err("overlapping configured LLVM depthwise ownership".into());
            }
        }
        for (index, call) in contraction::target::scheduled(&physical, &model, &schedule)? {
            if llvm_calls.insert(index, call).is_some() {
                return Err("overlapping configured LLVM contraction ownership".into());
            }
        }
        for (index, call) in activation::scheduled(&physical, &model, &schedule)? {
            if llvm_calls.insert(index, call).is_some() {
                return Err("overlapping configured LLVM kernel ownership".into());
            }
        }
    }
    let memory =
        memory::plan_memory_enabled(&schedule.buffers, &schedule.calls, options.memory_planning)?;
    let mut kernels = Vec::new();
    for (index, call) in schedule.calls.iter().enumerate() {
        if let Some(matrix) = llvm_calls.get(&index) {
            kernels.push(render::Kernel {
                function: format!("op_kernel_{index}"),
                source: matrix.source.clone(),
                inputs: call.buffers[1..].to_vec(),
                global: [1, 1, 1],
            });
            continue;
        }
        if let Some(image) = image_calls.get(&call.buffers[0]) {
            let kernel = image.logical_artifact(
                &format!("native_image_{index}"),
                &format!("op_kernel_{index}"),
            )?;
            kernels.push(render::Kernel {
                function: format!("native_image_{index}"),
                source: kernel.source,
                inputs: call.buffers[1..].to_vec(),
                global: [1, 1, 1],
            });
            continue;
        }
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
    let descriptors = (0..schedule.buffers.len())
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

    // Original exporters retain every declared input binding independently of
    // executable call liveness. Pruning a branch must not change the caller ABI.
    for (id, plan) in lowered.plans.iter().enumerate() {
        if let ScalarExpr::Input(name) = &plan.expression {
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
        if image_calls.contains_key(&output) || llvm_calls.contains_key(&index) {
            // LLVM's original IMAGE arguments promise noalias and 32-byte
            // alignment. Verify the actual planned views, including scratch,
            // before allowing those promises into a loadable artifact.
            let bytes = if let Some(matrix) = llvm_calls.get(&index) {
                matrix.bytes.clone()
            } else {
                image_calls[&output]
                    .logical_artifact(
                        &format!("native_image_{index}"),
                        &format!("op_kernel_{index}"),
                    )?
                    .bytes
                    .to_vec()
            };
            if call.buffers.len() != bytes.len()
                || call
                    .buffers
                    .iter()
                    .zip(bytes)
                    .any(|(&id, bytes)| descriptors[id].bytes != bytes)
            {
                return Err("IMAGE LLVM argument storage differs from typed ABI".into());
            }
            for (position, &id) in call.buffers.iter().enumerate() {
                let a = descriptors[id];
                if !a.offset.is_multiple_of(32) {
                    return Err("IMAGE LLVM view is not 32-byte aligned".into());
                }
                for &other in &call.buffers[position + 1..] {
                    let b = descriptors[other];
                    if a.base == b.base
                        && a.offset < b.offset + b.bytes
                        && b.offset < a.offset + a.bytes
                    {
                        return Err("IMAGE LLVM argument views overlap".into());
                    }
                }
            }
        }
        let mut args = vec![table.view(descriptors[output])?];
        for &id in &kernel.inputs {
            if scratch_buffers.contains(&id) {
                table.write(descriptors[id])?;
            }
            let initial = extra_initial.get(&id).map(Vec::as_slice).or_else(|| {
                lowered
                    .plans
                    .get(id)
                    .and_then(|_| initialized(&lowered, id))
            });
            table.read_initialized(descriptors[id], initial)?;
            args.push(table.view(descriptors[id])?);
        }
        table.write(descriptors[output])?;
        calls.push(graph::Call {
            kernel: index,
            views: args,
            scalars: vec![0; llvm_calls.get(&index).map_or(0, |c| c.scalars)],
            workers: u16::try_from(llvm_calls.get(&index).map_or(1, |c| c.workers))
                .map_err(|_| "matrix worker count exceeds artifact ABI")?,
            core_id: llvm_calls
                .get(&index)
                .and_then(|c| (c.scalars == 1).then_some(0)),
        });
    }
    for &id in lowered.outputs.values() {
        table.read_initialized(descriptors[id], initialized(&lowered, id))?;
    }
    let (allocations, weights) = table.weights();
    let graph = Graph {
        version: 1,
        cached_code: None,
        backend: if image_calls.is_empty() && llvm_calls.is_empty() {
            "cpu-clang"
        } else {
            "cpu-llvm"
        }
        .into(),
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
            .enumerate()
            .map(|(index, k)| graph::Kernel {
                buffers: k.inputs.len() + 1,
                scalars: llvm_calls.get(&index).map_or(0, |c| c.scalars),
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
        if llvm_calls.contains_key(&index) {
            let (config, worker) = options
                .llvm
                .as_ref()
                .ok_or("configured LLVM provider missing")?;
            source_bytes = source_bytes
                .checked_add(kernel.source.len())
                .filter(|n| *n <= 64 * 1024 * 1024)
                .ok_or("native CPU compiler sources exceed64MiB")?;
            let compiled = llvm::compile_isolated(worker, config, &kernel.source, options.timeout)?;
            elf_header(&compiled.object, 1)?;
            let object = temporary.path().join(format!("matrix-{index}.o"));
            require_space(temporary.path(), compiled.object.len() as u64 + (64 << 20))?;
            fs::write(&object, &compiled.object).map_err(|e| e.to_string())?;
            wrappers.push(format!(
                "/* configured LLVM receipt: {} */",
                serde_json::to_string(&compiled.receipt).map_err(|e| e.to_string())?
            ));
            sources.push(kernel.source.clone());
            objects.push(compiled.object);
            object_paths.push(object);
            continue;
        }
        if let Some(image) = image_calls.get(&schedule.calls[index].buffers[0]) {
            let (config, worker) = options.llvm.as_ref().ok_or("IMAGE LLVM provider missing")?;
            let generated = image.logical_artifact(
                &format!("native_image_{index}"),
                &format!("op_kernel_{index}"),
            )?;
            source_bytes = source_bytes
                .checked_add(generated.source.len())
                .and_then(|n| n.checked_add(generated.staging_source.len()))
                .filter(|n| *n <= 64 * 1024 * 1024)
                .ok_or("native CPU compiler sources exceed64MiB")?;
            let packed =
                llvm::compile_isolated(worker, config, &generated.source, options.timeout)?;
            let staged =
                llvm::compile_isolated(worker, config, &generated.staging_source, options.timeout)?;
            let packed_path = temporary.path().join(format!("image-{index}.o"));
            let stage_path = temporary.path().join(format!("image-{index}-stage.o"));
            let object = temporary.path().join(format!("kernel-{index}.o"));
            require_space(
                temporary.path(),
                (packed.object.len() + staged.object.len()) as u64 + (64 << 20),
            )?;
            fs::write(&packed_path, &packed.object).map_err(|e| e.to_string())?;
            fs::write(&stage_path, &staged.object).map_err(|e| e.to_string())?;
            let args = [
                OsString::from("-r"),
                packed_path.into_os_string(),
                stage_path.into_os_string(),
                "-o".into(),
                object.as_os_str().to_owned(),
            ];
            command(options, &args, temporary.path(), &object, 64 << 20)?;
            let object_bytes = read_bounded(&object, 64 << 20)?;
            elf_header(&object_bytes, 1)?;
            wrappers.push(format!("/* IMAGE LLVM packed receipt: {}\n IMAGE LLVM staging receipt: {}\n Staging module:\n{}\n*/",serde_json::to_string(&packed.receipt).map_err(|e|e.to_string())?,serde_json::to_string(&staged.receipt).map_err(|e|e.to_string())?,generated.staging_source));
            sources.push(generated.source);
            objects.push(object_bytes);
            object_paths.push(object);
            continue;
        }
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

#[cfg(test)]
mod process_lifecycle_tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    fn descendant_terminated(pid_file: &Path) {
        let started = Instant::now();
        let descendant: u32 = fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        loop {
            // An orphan may await init's reap; a zombie has already terminated.
            let alive = fs::read_to_string(format!("/proc/{descendant}/stat"))
                .ok()
                .and_then(|s| s.rsplit_once(") ").map(|(_, rest)| rest.starts_with('Z')))
                == Some(false);
            if !alive {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "compiler descendant survived leader cleanup"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn exited_leader_still_owns_group_during_descendant_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("descendant.pid");
        let child = Command::new("/bin/sh")
            .args(["-c", "sleep 60 & echo $! > \"$1\"", "compiler-test"])
            .arg(&pid_file)
            .process_group(0)
            .spawn()
            .unwrap();
        let mut owned = ChildGroup {
            child,
            reaped: false,
        };
        let started = Instant::now();
        loop {
            if let Some(status) = owned.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(owned.reaped);
        assert!(owned.try_wait().unwrap().unwrap().success());
        drop(owned);
        descendant_terminated(&pid_file);
    }

    #[test]
    fn timeout_stops_and_reaps_compiler_with_background_descendant() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("descendant.pid");
        let options = CompileOptions {
            program: "/bin/sh".into(),
            arguments: vec![
                "-c".into(),
                "sleep 60 & echo $! > \"$1\"; wait".into(),
                "compiler-test".into(),
                pid_file.as_os_str().to_owned(),
            ],
            timeout: Duration::from_millis(100),
            ..CompileOptions::default()
        };
        let error = command(
            &options,
            &[],
            directory.path(),
            &directory.path().join("output"),
            1024,
        )
        .unwrap_err();
        assert!(error.contains("timeout"), "{error}");
        descendant_terminated(&pid_file);
    }
}
