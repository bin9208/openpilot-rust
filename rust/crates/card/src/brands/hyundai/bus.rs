//! Hyundai harness selection, including the source's second-Panda offset.
pub type Fingerprints = [(u8, Vec<(u32, usize)>)];

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CanBus {
    pub ecan: u8,
    pub acan: u8,
    pub cam: u8,
}

impl CanBus {
    pub fn fingerprint(fingerprints: &Fingerprints, hda2: bool, camera_scc: i32) -> Self {
        let offset = fingerprints
            .iter()
            .filter(|(_, entries)| !entries.is_empty())
            .map(|(bus, _)| *bus / 4 * 4)
            .max()
            .unwrap_or(0);
        Self::with_offset(offset, hda2, camera_scc)
    }

    pub const fn with_offset(offset: u8, hda2: bool, camera_scc: i32) -> Self {
        let swapped = hda2 && camera_scc == 0;
        Self {
            ecan: offset + if swapped { 1 } else { 0 },
            acan: offset + if swapped { 0 } else { 1 },
            cam: offset + 2,
        }
    }
}

pub fn size(fingerprints: &Fingerprints, bus: u8, address: u32) -> Option<usize> {
    fingerprints
        .iter()
        .find(|(known, _)| *known == bus)
        .and_then(|(_, entries)| entries.iter().find(|(known, _)| *known == address))
        .map(|(_, size)| *size)
}

pub fn contains(fingerprints: &Fingerprints, bus: u8, address: u32) -> bool {
    size(fingerprints, bus, address).is_some()
}
