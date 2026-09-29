use crate::{
    entrypoint::{self, Entrypoint},
    Error,
};
use serde::Deserialize;
use std::collections::HashSet;

const MAX_MEMORY: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 1_000_000;
const MAX_MANIFEST: usize = 64 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub(crate) version: u32,
    pub(crate) backend: String,
    pub(crate) arch: String,
    pub(crate) weights_sha256: String,
    pub(crate) library_sha256: String,
    pub(crate) allocations: Vec<Allocation>,
    pub(crate) views: Vec<View>,
    pub(crate) inputs: Vec<Binding>,
    pub(crate) outputs: Vec<Binding>,
    pub(crate) kernels: Vec<Kernel>,
    pub(crate) calls: Vec<Call>,
    #[serde(default)]
    pub(crate) entrypoints: Vec<Entrypoint>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Allocation {
    pub bytes: usize,
    pub weight_offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct View {
    pub allocation: usize,
    pub offset: usize,
    pub bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub name: String,
    pub view: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Kernel {
    pub buffers: usize,
    pub scalars: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Call {
    pub kernel: usize,
    pub views: Vec<usize>,
    pub scalars: Vec<i32>,
    pub workers: u16,
    pub core_id: Option<usize>,
}

#[derive(Debug)]
pub struct ValidatedGraph {
    pub(crate) graph: Graph,
    allocation_bytes: u64,
}

impl ValidatedGraph {
    pub const fn allocation_bytes(&self) -> u64 {
        self.allocation_bytes
    }

    pub fn call_count(&self) -> usize {
        self.graph.calls.len()
    }
}

impl Graph {
    pub fn parse(bytes: &[u8]) -> Result<ValidatedGraph, Error> {
        if bytes.len() > MAX_MANIFEST {
            return Err(Error::Limit("manifest bytes"));
        }
        let graph: Self = serde_json::from_slice(bytes)?;
        let allocation_bytes = graph.validate()?;
        Ok(ValidatedGraph {
            graph,
            allocation_bytes,
        })
    }

    fn validate(&self) -> Result<u64, Error> {
        entrypoint::validate(self.version, &self.entrypoints, self.calls.len())?;
        if !matches!(self.backend.as_str(), "cpu-clang" | "cpu-llvm")
            || self.arch != std::env::consts::ARCH
        {
            return Err(Error::Contract("version/backend/architecture"));
        }
        for digest in [&self.weights_sha256, &self.library_sha256] {
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::Contract("SHA-256 encoding"));
            }
        }
        for count in [
            self.allocations.len(),
            self.views.len(),
            self.kernels.len(),
            self.calls.len(),
            self.inputs.len(),
            self.outputs.len(),
        ] {
            if count > MAX_ENTRIES {
                return Err(Error::Limit("entry count"));
            }
        }
        let mut total = 0_u64;
        for (index, allocation) in self.allocations.iter().enumerate() {
            let weight_range = match allocation.weight_offset {
                Some(offset) => offset
                    .checked_add(allocation.bytes)
                    .and_then(|end| u64::try_from(end).ok())
                    .is_some_and(|end| end <= MAX_MEMORY),
                None => true,
            };
            if allocation.bytes == 0 || !weight_range {
                return Err(Error::Invalid {
                    kind: "allocation",
                    index,
                });
            }
            total = total
                .checked_add(
                    u64::try_from(allocation.bytes)
                        .map_err(|_| Error::Limit("allocation bytes"))?,
                )
                .ok_or(Error::Limit("allocation sum"))?;
            if total > MAX_MEMORY {
                return Err(Error::Limit("allocation sum"));
            }
        }
        for (index, view) in self.views.iter().enumerate() {
            let valid = self.allocations.get(view.allocation).is_some_and(|a| {
                view.bytes > 0
                    && view
                        .offset
                        .checked_add(view.bytes)
                        .is_some_and(|end| end <= a.bytes)
            });
            if !valid {
                return Err(Error::Invalid {
                    kind: "view",
                    index,
                });
            }
        }
        for bindings in [&self.inputs, &self.outputs] {
            let mut names = HashSet::new();
            for (index, binding) in bindings.iter().enumerate() {
                if binding.name.is_empty()
                    || binding.view >= self.views.len()
                    || !names.insert(&binding.name)
                {
                    return Err(Error::Invalid {
                        kind: "binding",
                        index,
                    });
                }
            }
        }
        for (index, kernel) in self.kernels.iter().enumerate() {
            if kernel.buffers > 1024 || kernel.scalars > 1024 {
                return Err(Error::Invalid {
                    kind: "kernel signature",
                    index,
                });
            }
        }
        for (index, call) in self.calls.iter().enumerate() {
            let signature = self
                .kernels
                .get(call.kernel)
                .is_some_and(|k| k.buffers == call.views.len() && k.scalars == call.scalars.len());
            let core_id = match call.core_id {
                Some(id) => id < call.scalars.len(),
                None => call.workers == 1,
            };
            if !signature
                || !core_id
                || !(1..=1024).contains(&call.workers)
                || call.views.iter().any(|v| *v >= self.views.len())
            {
                return Err(Error::Invalid {
                    kind: "call",
                    index,
                });
            }
        }
        Ok(total)
    }
}
