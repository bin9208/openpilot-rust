use crate::{
    entrypoint::{self, Entrypoint},
    Error,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_MEMORY: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 1_000_000;
const MAX_MANIFEST: usize = 64 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub version: u32,
    pub backend: String,
    pub arch: String,
    pub weights_sha256: String,
    pub library_sha256: String,
    pub allocations: Vec<Allocation>,
    pub views: Vec<View>,
    pub inputs: Vec<Binding>,
    pub outputs: Vec<Binding>,
    pub kernels: Vec<Kernel>,
    pub calls: Vec<Call>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entrypoints: Vec<Entrypoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_code: Option<CachedCode>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CachedCode {
    pub certificate_sha256: String,
    pub kernels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub bytes: usize,
    pub weight_offset: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub allocation: usize,
    pub offset: usize,
    pub bytes: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub name: String,
    pub view: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Kernel {
    pub buffers: usize,
    pub scalars: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
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
        let entry_version = if self.version == 3 {
            if self.entrypoints.is_empty() {
                1
            } else {
                2
            }
        } else {
            self.version
        };
        entrypoint::validate(entry_version, &self.entrypoints, self.calls.len())?;
        let mode_valid = match self.backend.as_str() {
            "cpu-clang" | "cpu-llvm" => self.version != 3 && self.cached_code.is_none(),
            "cpu-cached-llvm" => {
                self.version == 3 && self.arch == "x86_64" && self.cached_code.is_some()
            }
            _ => false,
        };
        if !mode_valid || self.arch != std::env::consts::ARCH {
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
        if let Some(cached) = &self.cached_code {
            if cached.kernels.is_empty()
                || cached.kernels.len() > 4096
                || cached
                    .kernels
                    .iter()
                    .any(|index| *index >= self.kernels.len())
                || cached.kernels.iter().collect::<HashSet<_>>().len() != cached.kernels.len()
                || cached.certificate_sha256.len() != 64
                || !cached
                    .certificate_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(Error::Contract("cached graph certificate/roster"));
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

#[cfg(test)]
mod cached_schema_tests {
    use super::*;
    use serde_json::json;
    fn fresh() -> serde_json::Value {
        json!({"version":1,"backend":"cpu-llvm","arch":std::env::consts::ARCH,
            "weights_sha256":"0".repeat(64),"library_sha256":"0".repeat(64),
            "allocations":[],"views":[],"inputs":[],"outputs":[],"kernels":[],"calls":[]})
    }
    #[test]
    fn disabled_fresh_schema_serializes_without_cached_metadata() {
        let validated = Graph::parse(&serde_json::to_vec(&fresh()).unwrap()).unwrap();
        assert!(validated.graph.cached_code.is_none());
        let value = serde_json::to_value(&validated.graph).unwrap();
        assert!(value.get("cached_code").is_none());
        assert_eq!(value, fresh());
    }
    #[test]
    fn cached_metadata_cannot_be_added_to_a_fresh_backend_or_old_version() {
        let mut value = fresh();
        value["cached_code"] = json!({"certificate_sha256":"0".repeat(64),"kernels":[0]});
        assert!(Graph::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        value["backend"] = json!("cpu-cached-llvm");
        assert!(Graph::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}
