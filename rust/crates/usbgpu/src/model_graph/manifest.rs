use crate::Error;
use serde::Deserialize;
use std::{collections::BTreeSet, ops::Range};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub version: u32,
    pub backend: String,
    pub arch: String,
    pub weights_sha256: String,
    pub allocations: Vec<Allocation>,
    pub views: Vec<View>,
    pub inputs: Vec<Binding>,
    pub outputs: Vec<Binding>,
    pub kernels: Vec<Kernel>,
    pub calls: Vec<Call>,
    pub entrypoints: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Allocation {
    pub bytes: u64,
    pub weight_offset: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct View {
    pub allocation: usize,
    pub offset: u64,
    pub bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub name: String,
    pub view: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Kernel {
    pub name: String,
    pub binary_sha256: String,
    pub binary_bytes: u64,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Call {
    Kernel {
        kernel: usize,
        views: Vec<usize>,
        scalars: Vec<u32>,
        global: [u32; 3],
        local: [u32; 3],
    },
    Copy {
        source: usize,
        destination: usize,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub name: String,
    pub start: usize,
    pub end: usize,
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 64 << 20 {
            return Err(Error::Contract("AMD graph descriptor size limit"));
        }
        let graph: Self = serde_json::from_slice(bytes)?;
        if graph.version != 2
            || graph.backend != "amd-elf"
            || graph.arch != "gfx1200"
            || graph.allocations.is_empty()
            || graph.allocations.len() > 65536
            || graph.views.len() > 1_000_000
            || graph.calls.len() > 1_000_000
            || graph.kernels.is_empty()
            || graph.kernels.len() > 65536
            || !digest(&graph.weights_sha256)
        {
            return Err(Error::Contract("unsupported AMD model graph"));
        }
        if graph.kernels.iter().any(|kernel| {
            kernel.name.is_empty()
                || kernel.name.len() > 4096
                || !digest(&kernel.binary_sha256)
                || kernel.binary_bytes == 0
                || kernel.binary_bytes > 64 << 20
        }) {
            return Err(Error::Contract("AMD graph kernel metadata mismatch"));
        }
        let mut total = 0u64;
        for allocation in &graph.allocations {
            total = total
                .checked_add(allocation.bytes)
                .ok_or(Error::Contract("AMD graph allocation overflow"))?;
            if allocation.bytes == 0 || total > 4 << 30 {
                return Err(Error::Contract("AMD graph allocation limit"));
            }
        }
        for view in &graph.views {
            if view.bytes == 0
                || graph
                    .allocations
                    .get(view.allocation)
                    .is_none_or(|allocation| {
                        view.offset
                            .checked_add(view.bytes)
                            .is_none_or(|end| end > allocation.bytes)
                    })
            {
                return Err(Error::Contract("AMD graph view outside allocation"));
            }
        }
        for bindings in [&graph.inputs, &graph.outputs] {
            let mut names = BTreeSet::new();
            for binding in bindings {
                if binding.name.is_empty()
                    || !names.insert(&binding.name)
                    || binding.view >= graph.views.len()
                {
                    return Err(Error::Contract("AMD graph binding mismatch"));
                }
            }
        }
        for call in &graph.calls {
            match call {
                Call::Kernel {
                    kernel,
                    views,
                    scalars,
                    global,
                    local,
                } => {
                    if *kernel >= graph.kernels.len()
                        || views.iter().any(|view| *view >= graph.views.len())
                        || views.len() > 256
                        || scalars.len() > 256
                        || global.contains(&0)
                        || local.contains(&0)
                        || local
                            .iter()
                            .try_fold(1u32, |value, dim| value.checked_mul(*dim))
                            .is_none_or(|size| size > 1024)
                    {
                        return Err(Error::Contract("AMD graph kernel call mismatch"));
                    }
                }
                Call::Copy {
                    source,
                    destination,
                } => {
                    if graph
                        .views
                        .get(*source)
                        .zip(graph.views.get(*destination))
                        .is_none_or(|(source, destination)| source.bytes != destination.bytes)
                    {
                        return Err(Error::Contract("AMD graph copy mismatch"));
                    }
                }
            }
        }
        let mut end = 0;
        let mut names = BTreeSet::new();
        for entry in &graph.entrypoints {
            if entry.name.is_empty()
                || !names.insert(&entry.name)
                || entry.start != end
                || entry.start > entry.end
                || entry.end > graph.calls.len()
            {
                return Err(Error::Contract("AMD graph entrypoint partition"));
            }
            end = entry.end;
        }
        if graph.entrypoints.is_empty() || end != graph.calls.len() {
            return Err(Error::Contract("AMD graph entrypoints incomplete"));
        }
        Ok(graph)
    }
    pub fn entry(&self, name: &str) -> Result<Range<usize>, Error> {
        let entry = self
            .entrypoints
            .iter()
            .find(|entry| entry.name == name)
            .ok_or(Error::Contract("AMD graph entrypoint missing"))?;
        Ok(entry.start..entry.end)
    }
}
