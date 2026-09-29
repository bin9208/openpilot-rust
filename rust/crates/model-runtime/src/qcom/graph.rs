use crate::{
    graph::{Allocation, Binding, View},
    Error,
};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QcomGraph {
    pub(super) version: u32,
    pub(super) backend: String,
    pub(super) arch: String,
    pub(super) weights_sha256: String,
    pub(super) allocations: Vec<Allocation>,
    pub(super) views: Vec<View>,
    pub(super) inputs: Vec<Binding>,
    pub(super) outputs: Vec<Binding>,
    pub(super) kernels: Vec<Kernel>,
    pub(super) calls: Vec<Call>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Kernel {
    pub name: String,
    pub binary_sha256: String,
    pub binary_bytes: usize,
    pub arguments: Vec<Vec<ArgumentSpec>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ArgumentSpec {
    Buffer,
    Image {
        width: u32,
        height: u32,
        pitch: u32,
        element_bytes: u32,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Call {
    Kernel {
        kernel: usize,
        views: Vec<usize>,
        scalars: Vec<u32>,
        global: [f64; 3],
        local: [u32; 3],
    },
    Copy {
        source: usize,
        destination: usize,
    },
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl QcomGraph {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(Error::Limit("QCOM manifest bytes"));
        }
        let graph: Self = serde_json::from_slice(bytes)?;
        graph.validate()?;
        Ok(graph)
    }

    pub fn allocation_bytes(&self) -> usize {
        self.allocations
            .iter()
            .map(|allocation| allocation.bytes)
            .sum()
    }
    pub fn call_count(&self) -> usize {
        self.calls.len()
    }

    fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || self.backend != "qcom-cl"
            || self.arch != "a630"
            || !digest(&self.weights_sha256)
        {
            return Err(Error::Contract(
                "QCOM version/backend/architecture/checksum",
            ));
        }
        if [
            self.allocations.len(),
            self.views.len(),
            self.inputs.len(),
            self.outputs.len(),
            self.kernels.len(),
            self.calls.len(),
        ]
        .into_iter()
        .any(|count| count > 65536)
        {
            return Err(Error::Limit("QCOM graph entries"));
        }
        let mut total = 0_u64;
        for allocation in &self.allocations {
            total = total
                .checked_add(allocation.bytes as u64)
                .ok_or(Error::Limit("QCOM allocation sum"))?;
            if allocation.bytes == 0
                || total > 4 * 1024 * 1024 * 1024
                || allocation.weight_offset.is_some_and(|offset| {
                    offset
                        .checked_add(allocation.bytes)
                        .is_none_or(|end| end as u64 > 4 * 1024 * 1024 * 1024)
                })
            {
                return Err(Error::Limit("QCOM allocation range"));
            }
        }
        for view in &self.views {
            if !self
                .allocations
                .get(view.allocation)
                .is_some_and(|allocation| {
                    view.bytes > 0
                        && view
                            .offset
                            .checked_add(view.bytes)
                            .is_some_and(|end| end <= allocation.bytes)
                })
            {
                return Err(Error::Contract("QCOM buffer view"));
            }
        }
        for bindings in [&self.inputs, &self.outputs] {
            let mut names = HashSet::new();
            for binding in bindings {
                if binding.name.is_empty()
                    || !names.insert(&binding.name)
                    || binding.view >= self.views.len()
                {
                    return Err(Error::Contract("QCOM binding"));
                }
            }
        }
        let mut binary_bytes = 0_usize;
        for kernel in &self.kernels {
            binary_bytes = binary_bytes
                .checked_add(kernel.binary_bytes)
                .ok_or(Error::Limit("QCOM kernel binary sum"))?;
            if kernel.name.is_empty()
                || !kernel.name.is_ascii()
                || kernel.name.len() > 4096
                || !digest(&kernel.binary_sha256)
                || kernel.binary_bytes == 0
                || kernel.binary_bytes > 64 * 1024 * 1024
                || binary_bytes > 512 * 1024 * 1024
                || kernel.arguments.len() > 256
                || kernel
                    .arguments
                    .iter()
                    .any(|arguments| arguments.is_empty() || arguments.len() > 256)
            {
                return Err(Error::Contract("QCOM kernel contract"));
            }
            for argument in kernel.arguments.iter().flatten() {
                if let ArgumentSpec::Image {
                    width,
                    height,
                    pitch,
                    element_bytes,
                } = *argument
                {
                    if !(1..=32767).contains(&width)
                        || !(1..=32767).contains(&height)
                        || ![2, 4].contains(&element_bytes)
                        || pitch < width * 4 * element_bytes
                        || pitch > 0x3fffff
                        || pitch % 64 != 0
                    {
                        return Err(Error::Contract("QCOM image layout"));
                    }
                }
            }
        }
        for call in &self.calls {
            match call {
                Call::Copy {
                    source,
                    destination,
                } => {
                    if self
                        .views
                        .get(*source)
                        .zip(self.views.get(*destination))
                        .is_none_or(|(source, destination)| source.bytes != destination.bytes)
                    {
                        return Err(Error::Contract("QCOM copy range"));
                    }
                }
                Call::Kernel {
                    kernel,
                    views,
                    scalars,
                    global,
                    local,
                } => {
                    let kernel = self
                        .kernels
                        .get(*kernel)
                        .ok_or(Error::Contract("QCOM kernel index"))?;
                    if kernel.arguments.len() != views.len()
                        || scalars.len() > 256
                        || local.contains(&0)
                        || global.iter().any(|value| {
                            !value.is_finite()
                                || *value <= 0.0
                                || value.ceil() > f64::from(u32::MAX)
                        })
                        || local
                            .iter()
                            .try_fold(1_u32, |total, value| total.checked_mul(*value))
                            .is_none_or(|threads| threads > 1024)
                    {
                        return Err(Error::Contract("QCOM launch contract"));
                    }
                    for (index, arguments) in views.iter().zip(&kernel.arguments) {
                        let view = self
                            .views
                            .get(*index)
                            .ok_or(Error::Contract("QCOM call view"))?;
                        for argument in arguments {
                            if let ArgumentSpec::Image { height, pitch, .. } = *argument {
                                if u64::from(height) * u64::from(pitch) > view.bytes as u64
                                    || view.offset % 64 != 0
                                {
                                    return Err(Error::Contract("QCOM image buffer range"));
                                }
                            }
                        }
                    }
                    if global.iter().zip(local).any(|(global, local)| {
                        !(1.0..=f64::from(u32::MAX)).contains(&(global * f64::from(*local)))
                    }) {
                        return Err(Error::Limit("QCOM grid dimensions"));
                    }
                }
            }
        }
        Ok(())
    }
}
