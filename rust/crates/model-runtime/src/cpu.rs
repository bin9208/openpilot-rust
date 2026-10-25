use crate::{assets::read_verified, buffer::Buffer, entrypoint, graph::Binding, Error, Graph};
use libloading::Library;
use std::{ffi::c_void, fs, io::Read, path::Path};

type KernelFn = unsafe extern "C" fn(*const *mut c_void, *const i32);

#[cfg(all(
    test,
    feature = "cached-program-isa",
    target_arch = "x86_64",
    target_os = "linux"
))]
mod diagnostic_482_tests;

struct Invocation {
    function: KernelFn,
    buffers: Vec<*mut c_void>,
    scalars: Vec<i32>,
    workers: u16,
    core_id: Option<usize>,
}

pub struct CpuModel {
    graph: Graph,
    buffers: Vec<Buffer>,
    calls: Vec<Invocation>,
    _library: Library,
    #[cfg(unix)]
    _cached_library: Option<crate::cached_code::HeldLibrary>,
}

impl CpuModel {
    /// # Safety
    /// The directory must contain trusted kernels matching every manifest range,
    /// alignment, scalar and ABI. Kernels must finish synchronously, retain no
    /// pointers, never unwind, and only access supplied buffers within their views.
    /// The caller must prevent artifact modification until this model is dropped.
    /// Checksums detect corruption; they do not authenticate executable code.
    pub unsafe fn load(directory: &Path) -> Result<Self, Error> {
        // SAFETY: this method preserves its existing trusted/immutable contract.
        unsafe { Self::load_inner(directory, None) }
    }

    /// Load only an explicitly selected cached-code candidate.
    ///
    /// # Safety
    /// All of `load`'s kernel/immutability requirements apply. `admission` must
    /// come from independent original/current source and issuer authentication,
    /// never by copying expected identities from the candidate artifact. The
    /// ISA issuer must have decoded every complete image without executing it.
    #[cfg(unix)]
    pub unsafe fn load_cached(
        directory: &Path,
        admission: &crate::cached_code::Admission,
    ) -> Result<Self, Error> {
        // SAFETY: this explicit caller supplies load's contract plus the above
        // independent original/current admission; prepare runs before dlopen.
        unsafe { Self::load_inner(directory, Some(admission)) }
    }

    unsafe fn load_inner(
        directory: &Path,
        #[cfg(unix)] admission: Option<&crate::cached_code::Admission>,
        #[cfg(not(unix))] _admission: Option<&()>,
    ) -> Result<Self, Error> {
        if cfg!(target_endian = "big") {
            return Err(Error::Contract("little-endian CPU required"));
        }
        let mut manifest = Vec::new();
        fs::File::open(directory.join("graph.json"))?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut manifest)?;
        let graph = Graph::parse(&manifest)?.graph;
        #[cfg(unix)]
        let cached_library = match (graph.cached_code.as_ref(), admission) {
            (Some(_), Some(expected)) => Some(
                crate::cached_code::prepare(directory, &graph, expected)
                    .map_err(Error::CachedCode)?,
            ),
            (Some(_), None) => {
                return Err(Error::Contract("cached mode requires explicit admission"))
            }
            (None, Some(_)) => {
                return Err(Error::Contract("cached admission requires cached graph"))
            }
            (None, None) => None,
        };
        #[cfg(not(unix))]
        if graph.cached_code.is_some() {
            return Err(Error::Contract("cached mode requires x86_64 Linux"));
        }
        let weight_bytes = graph
            .allocations
            .iter()
            .filter_map(|allocation| {
                allocation
                    .weight_offset
                    .and_then(|offset| offset.checked_add(allocation.bytes))
            })
            .max()
            .unwrap_or(0);
        let weights = read_verified(
            &directory.join("weights.bin"),
            &graph.weights_sha256,
            weight_bytes,
        )?;
        if weights.len() != weight_bytes {
            return Err(Error::Contract("weight file length"));
        }
        let library_path = directory.join("kernels.so");
        read_verified(&library_path, &graph.library_sha256, 512 * 1024 * 1024)?;
        let mut buffers = Vec::with_capacity(graph.allocations.len());
        for allocation in &graph.allocations {
            let mut buffer = Buffer::new(allocation.bytes)?;
            if let Some(offset) = allocation.weight_offset {
                let end = offset
                    .checked_add(allocation.bytes)
                    .ok_or(Error::Limit("weight range"))?;
                let data = weights
                    .get(offset..end)
                    .ok_or(Error::Contract("weight range"))?;
                buffer.write(0, data)?;
            }
            buffers.push(buffer);
        }
        // SAFETY: the caller guarantees trusted code and immutable artifacts.
        #[cfg(unix)]
        let selected_library = cached_library.as_ref().map_or_else(
            || library_path.clone(),
            |held| held.descriptor_path().into(),
        );
        #[cfg(not(unix))]
        let selected_library = library_path;
        // SAFETY: immutable trusted kernels; cached mode additionally verifies
        // every bound image and ISA requirement and opens the held descriptor.
        let library = unsafe { Library::new(selected_library) }?;
        #[cfg(unix)]
        if let Some(held) = &cached_library {
            held.verify().map_err(Error::CachedCode)?;
        }
        let mut functions = Vec::with_capacity(graph.kernels.len());
        for index in 0..graph.kernels.len() {
            let name = format!("op_kernel_{index}\0");
            // SAFETY: load's contract guarantees the fixed wrapper ABI for each symbol.
            let function = unsafe { library.get::<KernelFn>(name.as_bytes()) }?;
            functions.push(*function);
        }
        let mut calls = Vec::with_capacity(graph.calls.len());
        for call in &graph.calls {
            let mut arguments = Vec::with_capacity(call.views.len());
            for id in &call.views {
                let view = &graph.views[*id];
                arguments.push(
                    buffers[view.allocation]
                        .view_ptr(view.offset, view.bytes)?
                        .cast(),
                );
            }
            calls.push(Invocation {
                function: functions[call.kernel],
                buffers: arguments,
                scalars: call.scalars.clone(),
                workers: call.workers,
                core_id: call.core_id,
            });
        }
        Ok(Self {
            graph,
            buffers,
            calls,
            _library: library,
            #[cfg(unix)]
            _cached_library: cached_library,
        })
    }

    pub fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        let view = &self.graph.views[find_binding(&self.graph.inputs, name)?];
        check_size(view.bytes, bytes.len())?;
        self.buffers[view.allocation].write(view.offset, bytes)
    }

    pub fn read_output(&self, name: &str, bytes: &mut [u8]) -> Result<(), Error> {
        let view = &self.graph.views[find_binding(&self.graph.outputs, name)?];
        check_size(view.bytes, bytes.len())?;
        self.buffers[view.allocation].read(view.offset, bytes)
    }

    pub fn output_size(&self, name: &str) -> Result<usize, Error> {
        Ok(self.graph.views[find_binding(&self.graph.outputs, name)?].bytes)
    }

    pub fn run(&mut self) {
        Self::execute(&mut self.calls);
    }

    pub fn run_entry(&mut self, name: &str) -> Result<(), Error> {
        let entry = &self.graph.entrypoints[entrypoint::find(&self.graph.entrypoints, name)?];
        Self::execute(&mut self.calls[entry.range()]);
        Ok(())
    }

    fn execute(calls: &mut [Invocation]) {
        for call in calls {
            for worker in 0..call.workers {
                if let Some(index) = call.core_id {
                    call.scalars[index] = i32::from(worker);
                }
                // SAFETY: validated arity/ranges, live storage/library, exclusive run,
                // and load's trusted-kernel contract preserve aliasing and ABI rules.
                unsafe { (call.function)(call.buffers.as_ptr(), call.scalars.as_ptr()) };
            }
        }
    }
}

fn find_binding(bindings: &[Binding], name: &str) -> Result<usize, Error> {
    bindings
        .iter()
        .find(|binding| binding.name == name)
        .map(|binding| binding.view)
        .ok_or_else(|| Error::Binding(name.to_owned()))
}

fn check_size(expected: usize, actual: usize) -> Result<(), Error> {
    if expected != actual {
        return Err(Error::Size { expected, actual });
    }
    Ok(())
}

#[cfg(all(test, unix, target_arch = "x86_64"))]
mod cached_admission_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn ordinary_load_rejects_cached_mode_before_any_library_or_sidecar_access() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = json!({"version":3,"backend":"cpu-cached-llvm","arch":"x86_64",
            "weights_sha256":"0".repeat(64),"library_sha256":"0".repeat(64),
            "allocations":[],"views":[],"inputs":[],"outputs":[],
            "kernels":[{"buffers":0,"scalars":0}],
            "calls":[{"kernel":0,"views":[],"scalars":[],"workers":1,"core_id":null}],
            "cached_code":{"certificate_sha256":"0".repeat(64),"kernels":[0]}});
        fs::write(
            directory.path().join("graph.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        // No weights, library or sidecar exists. Failure must identify the
        // disabled mode, rather than reach any file read/dlopen fallback.
        // SAFETY: this candidate rejects before reaching executable code.
        let result = unsafe { CpuModel::load(directory.path()) };
        assert!(matches!(
            result,
            Err(Error::Contract("cached mode requires explicit admission"))
        ));
    }
}
