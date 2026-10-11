use super::manifest::Manifest;
use crate::Error;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub struct Bundle {
    pub(super) manifest: Manifest,
    pub(super) weights: File,
    pub(super) kernels: Vec<Vec<u8>>,
}

impl Bundle {
    pub fn open(root: &Path) -> Result<Self, Error> {
        let mut descriptor = File::open(root.join("graph.json"))?;
        if descriptor.metadata()?.len() > 64 << 20 {
            return Err(Error::Contract("AMD graph descriptor size limit"));
        }
        let mut bytes = Vec::new();
        descriptor.read_to_end(&mut bytes)?;
        let manifest = Manifest::parse(&bytes)?;
        let mut weights = File::open(root.join("weights.bin"))?;
        let size = weights.metadata()?.len();
        if size > 4 << 30 {
            return Err(Error::Contract("AMD graph weights size limit"));
        }
        let mut hash = Sha256::new();
        std::io::copy(&mut weights, &mut hash)?;
        if format!("{:x}", hash.finalize()) != manifest.weights_sha256 {
            return Err(Error::Contract("AMD graph weights checksum mismatch"));
        }
        for allocation in &manifest.allocations {
            if allocation.weight_offset.is_some_and(|offset| {
                offset
                    .checked_add(allocation.bytes)
                    .is_none_or(|end| end > size)
            }) {
                return Err(Error::Contract("AMD graph initial data range"));
            }
        }
        weights.seek(SeekFrom::Start(0))?;
        let mut kernels = Vec::with_capacity(manifest.kernels.len());
        let mut total = 0u64;
        for (index, kernel) in manifest.kernels.iter().enumerate() {
            total = total
                .checked_add(kernel.binary_bytes)
                .ok_or(Error::Contract("AMD graph kernel bytes overflow"))?;
            if total > 512 << 20 {
                return Err(Error::Contract("AMD graph kernel bytes limit"));
            }
            let mut file = File::open(root.join(format!("kernel-{index}.bin")))?;
            if file.metadata()?.len() != kernel.binary_bytes {
                return Err(Error::Contract("AMD graph kernel length mismatch"));
            }
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            if format!("{:x}", Sha256::digest(&bytes)) != kernel.binary_sha256 {
                return Err(Error::Contract("AMD graph kernel checksum mismatch"));
            }
            kernels.push(bytes);
        }
        Ok(Self {
            manifest,
            weights,
            kernels,
        })
    }

    pub fn input_size(&self, name: &str) -> Option<u64> {
        self.manifest
            .inputs
            .iter()
            .find(|binding| binding.name == name)
            .map(|binding| self.manifest.views[binding.view].bytes)
    }
    pub fn output_size(&self, name: &str) -> Option<u64> {
        self.manifest
            .outputs
            .iter()
            .find(|binding| binding.name == name)
            .map(|binding| self.manifest.views[binding.view].bytes)
    }
    pub fn entrypoints(&self) -> impl Iterator<Item = &str> {
        self.manifest
            .entrypoints
            .iter()
            .map(|entry| entry.name.as_str())
    }
}
