use super::{graph::Call, Argument, Dispatch, ProgramImage, QcomGraph};
use crate::{assets::read_verified, Error};
use std::{fs, io::Read, path::Path};

pub struct QcomBundle {
    pub(super) graph: QcomGraph,
    pub(super) programs: Vec<ProgramImage>,
    pub(super) weights: Vec<u8>,
}

impl QcomBundle {
    pub fn load(directory: &Path) -> Result<Self, Error> {
        let mut manifest = Vec::new();
        fs::File::open(directory.join("graph.json"))?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut manifest)?;
        let graph = QcomGraph::parse(&manifest)?;
        let weight_bytes = graph
            .allocations
            .iter()
            .filter_map(|allocation| {
                allocation
                    .weight_offset
                    .map(|offset| offset + allocation.bytes)
            })
            .max()
            .unwrap_or(0);
        let weights = read_verified(
            &directory.join("weights.bin"),
            &graph.weights_sha256,
            weight_bytes,
        )?;
        if weights.len() != weight_bytes {
            return Err(Error::Contract("QCOM weight length"));
        }
        let mut programs = Vec::with_capacity(graph.kernels.len());
        for (index, kernel) in graph.kernels.iter().enumerate() {
            let binary = read_verified(
                &directory.join(format!("kernel-{index}.bin")),
                &kernel.binary_sha256,
                kernel.binary_bytes,
            )?;
            if binary.len() != kernel.binary_bytes {
                return Err(Error::Contract("QCOM kernel length"));
            }
            programs.push(ProgramImage::parse(&kernel.name, &binary)?);
        }
        for call in &graph.calls {
            if let Call::Kernel {
                kernel,
                scalars,
                global,
                local,
                ..
            } = call
            {
                let arguments = graph.kernels[*kernel]
                    .arguments
                    .iter()
                    .flatten()
                    .map(|spec| spec.bind(0))
                    .collect::<Vec<_>>();
                programs[*kernel].arguments(&arguments, scalars)?;
                programs[*kernel].dispatch(&Dispatch {
                    program: 0,
                    stack: 0,
                    border: 0,
                    dummy: 0,
                    args: 0,
                    global: *global,
                    local: *local,
                })?;
            }
        }
        Ok(Self {
            graph,
            programs,
            weights,
        })
    }

    pub fn allocation_bytes(&self) -> usize {
        self.graph.allocation_bytes()
    }
    pub fn call_count(&self) -> usize {
        self.graph.call_count()
    }
    pub fn kernel_count(&self) -> usize {
        self.programs.len()
    }
}

impl super::graph::ArgumentSpec {
    pub(super) fn bind(&self, address: u64) -> Argument {
        match *self {
            Self::Buffer => Argument::Buffer { address },
            Self::Image {
                width,
                height,
                pitch,
                element_bytes,
            } => Argument::Image {
                address,
                width,
                height,
                pitch,
                element_bytes,
            },
        }
    }
}
