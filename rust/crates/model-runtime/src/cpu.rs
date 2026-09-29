use crate::{assets::read_verified, buffer::Buffer, entrypoint, graph::Binding, Error, Graph};
use libloading::Library;
use std::{ffi::c_void, fs, io::Read, path::Path};

type KernelFn = unsafe extern "C" fn(*const *mut c_void, *const i32);

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
}

impl CpuModel {
    /// # Safety
    /// The directory must contain trusted kernels matching every manifest range,
    /// alignment, scalar and ABI. Kernels must finish synchronously, retain no
    /// pointers, never unwind, and only access supplied buffers within their views.
    /// The caller must prevent artifact modification until this model is dropped.
    /// Checksums detect corruption; they do not authenticate executable code.
    pub unsafe fn load(directory: &Path) -> Result<Self, Error> {
        if cfg!(target_endian = "big") {
            return Err(Error::Contract("little-endian CPU required"));
        }
        let mut manifest = Vec::new();
        fs::File::open(directory.join("graph.json"))?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut manifest)?;
        let graph = Graph::parse(&manifest)?.graph;
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
        let library = unsafe { Library::new(library_path) }?;
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
