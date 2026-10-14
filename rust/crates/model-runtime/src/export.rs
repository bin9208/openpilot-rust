//! Native artifact compiler storage accounting. Derived from model_export/buffers.py.
//! Tensor lowering must provide stable base identities and real initializer bytes.
use serde::Serialize;
use std::collections::BTreeMap;
type Result<T> = std::result::Result<T, String>;
const LIMIT: usize = 4 * 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug)]
pub struct Buffer {
    pub base: u64,
    pub allocation_bytes: usize,
    pub offset: usize,
    pub bytes: usize,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Allocation {
    pub bytes: usize,
    pub weight_offset: Option<usize>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct View {
    pub allocation: usize,
    pub offset: usize,
    pub bytes: usize,
}
#[derive(Default)]
pub struct BufferTable {
    bases: BTreeMap<u64, usize>,
    views_by_key: BTreeMap<(usize, usize, usize), usize>,
    pub views: Vec<View>,
    initial: Vec<Vec<u8>>,
    written: Vec<Vec<(usize, usize)>>,
    total: usize,
}
impl BufferTable {
    pub fn view(&mut self, buffer: Buffer) -> Result<usize> {
        let end = buffer
            .offset
            .checked_add(buffer.bytes)
            .ok_or("buffer range overflow")?;
        if end > buffer.allocation_bytes {
            return Err("buffer view exceeds allocation".into());
        }
        let allocation = if let Some(&index) = self.bases.get(&buffer.base) {
            if self.initial[index].len() != buffer.allocation_bytes {
                return Err("base identity changed size".into());
            }
            index
        } else {
            let total = self
                .total
                .checked_add(buffer.allocation_bytes)
                .ok_or("allocation sum overflow")?;
            if total > LIMIT {
                return Err("compiler allocation limit exceeded".into());
            }
            let index = self.initial.len();
            self.bases.insert(buffer.base, index);
            self.initial.push(vec![0; buffer.allocation_bytes]);
            self.written.push(Vec::new());
            self.total = total;
            index
        };
        let key = (allocation, buffer.offset, buffer.bytes);
        if let Some(&index) = self.views_by_key.get(&key) {
            return Ok(index);
        }
        let index = self.views.len();
        self.views.push(View {
            allocation,
            offset: buffer.offset,
            bytes: buffer.bytes,
        });
        self.views_by_key.insert(key, index);
        Ok(index)
    }
    fn remaining(&mut self, buffer: Buffer) -> Result<(usize, Vec<(usize, usize)>)> {
        let index = self.view(buffer)?;
        let view = &self.views[index];
        let mut remaining = vec![(view.offset, view.offset + view.bytes)];
        for &(start, end) in &self.written[view.allocation] {
            remaining = remaining
                .into_iter()
                .flat_map(|(low, high)| [(low, high.min(start)), (low.max(end), high)])
                .filter(|(a, b)| a < b)
                .collect();
        }
        Ok((view.allocation, remaining))
    }
    /// CPU capture bytes belong to this view, including its base offset.
    pub fn read(&mut self, buffer: Buffer, data: &[u8]) -> Result<()> {
        let (allocation, remaining) = self.remaining(buffer)?;
        if remaining.is_empty() {
            return Ok(());
        }
        if data.len() != buffer.bytes {
            return Err("captured view byte length mismatch".into());
        }
        for (low, high) in remaining {
            self.initial[allocation][low..high]
                .copy_from_slice(&data[low - buffer.offset..high - buffer.offset]);
        }
        Ok(())
    }
    /// Offline capture admits only real, complete base initializer bytes when needed.
    pub fn read_initialized(&mut self, buffer: Buffer, data: Option<&[u8]>) -> Result<()> {
        let (allocation, remaining) = self.remaining(buffer)?;
        if remaining.is_empty() {
            return Ok(());
        }
        let data = data
            .filter(|data| data.len() == buffer.allocation_bytes)
            .ok_or("offline storage has no real initialized bytes")?;
        for (low, high) in remaining {
            self.initial[allocation][low..high].copy_from_slice(&data[low..high]);
        }
        Ok(())
    }
    pub fn write(&mut self, buffer: Buffer) -> Result<()> {
        let index = self.view(buffer)?;
        let view = &self.views[index];
        self.written[view.allocation].push((view.offset, view.offset + view.bytes));
        Ok(())
    }
    pub fn weights(&self) -> (Vec<Allocation>, Vec<u8>) {
        let mut packed = Vec::new();
        let allocations = self
            .initial
            .iter()
            .map(|data| {
                let offset = if data.iter().any(|byte| *byte != 0) {
                    let offset = packed.len();
                    packed.extend_from_slice(data);
                    Some(offset)
                } else {
                    None
                };
                Allocation {
                    bytes: data.len(),
                    weight_offset: offset,
                }
            })
            .collect();
        (allocations, packed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn buffer(offset: usize, bytes: usize) -> Buffer {
        Buffer {
            base: 1,
            allocation_bytes: 8,
            offset,
            bytes,
        }
    }
    #[test]
    fn aliases_capture_only_source_bytes_read_before_any_write() {
        for start in 0..=8 {
            for end in start..=8 {
                let mut table = BufferTable::default();
                table.write(buffer(start, end - start)).unwrap();
                table.read(buffer(0, 8), &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
                let (allocations, weights) = table.weights();
                let expected = (0..8)
                    .map(|i| {
                        if (start..end).contains(&i) {
                            0
                        } else {
                            i as u8 + 1
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    allocations[0].weight_offset,
                    if expected.iter().all(|x| *x == 0) {
                        None
                    } else {
                        Some(0)
                    }
                );
                assert_eq!(
                    weights,
                    if expected.iter().all(|x| *x == 0) {
                        Vec::new()
                    } else {
                        expected
                    }
                );
            }
        }
    }
    #[test]
    fn view_offsets_zero_weights_and_base_encounter_order_are_preserved() {
        let mut table = BufferTable::default();
        table.read(buffer(3, 2), &[9, 10]).unwrap();
        assert_eq!(table.view(buffer(3, 2)).unwrap(), 0);
        let second = Buffer {
            base: 0,
            allocation_bytes: 4,
            offset: 0,
            bytes: 4,
        };
        table.read(second, &[0; 4]).unwrap();
        let (allocations, weights) = table.weights();
        assert_eq!(weights, [0, 0, 0, 9, 10, 0, 0, 0]);
        assert_eq!(
            allocations,
            vec![
                Allocation {
                    bytes: 8,
                    weight_offset: Some(0)
                },
                Allocation {
                    bytes: 4,
                    weight_offset: None
                }
            ]
        );
    }
    #[test]
    fn offline_unknown_reads_and_malformed_aliases_are_rejected() {
        let mut table = BufferTable::default();
        assert!(table.read_initialized(buffer(0, 8), None).is_err());
        table.write(buffer(0, 8)).unwrap();
        table.read_initialized(buffer(0, 8), None).unwrap();
        assert!(table.view(buffer(7, 2)).is_err());
        assert!(table
            .view(Buffer {
                allocation_bytes: 7,
                ..buffer(0, 1)
            })
            .is_err());
        assert!(table
            .view(Buffer {
                offset: usize::MAX,
                ..buffer(0, 1)
            })
            .is_err());
    }
}

pub use crate::entrypoint::Entrypoint;
/// CPU schema reused directly from the runtime validator; serialization keeps original field order.
pub use crate::graph::{
    Allocation as GraphAllocation, Binding, Call, Graph as CpuGraph, Kernel, View as GraphView,
};

/// Emit the same kernels/weights/graph files as the original CPU exporter.
/// Lowering and compiling kernels precedes this boundary; no model execution occurs here.
pub fn emit_cpu(
    destination: &std::path::Path,
    graph: &CpuGraph,
    library: &[u8],
    weights: &[u8],
    sources: &[String],
    objects: &[Vec<u8>],
    wrappers: &[String],
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    if graph.kernels.len() != sources.len()
        || sources.len() != objects.len()
        || sources.len() != wrappers.len()
    {
        return Err("CPU kernel emission lengths disagree".into());
    }
    if graph.weights_sha256 != format!("{:x}", Sha256::digest(weights))
        || graph.library_sha256 != format!("{:x}", Sha256::digest(library))
    {
        return Err("CPU emitted artifact digest differs".into());
    }
    let json = python_pretty(graph)?;
    crate::Graph::parse(json.as_bytes())?;
    for allocation in &graph.allocations {
        if let Some(offset) = allocation.weight_offset {
            if offset
                .checked_add(allocation.bytes)
                .is_none_or(|end| end > weights.len())
            {
                return Err("CPU allocation exceeds emitted weights".into());
            }
        }
    }
    if destination.exists() || destination.is_symlink() {
        return Err("CPU emission destination already exists".into());
    }
    let parent = destination
        .parent()
        .ok_or("CPU emission destination has no parent")?;
    let growth = library
        .len()
        .checked_add(weights.len())
        .and_then(|value| value.checked_add(json.len()))
        .and_then(|value| {
            sources
                .iter()
                .chain(wrappers)
                .try_fold(value, |sum, item| sum.checked_add(item.len()))
        })
        .and_then(|value| {
            objects
                .iter()
                .try_fold(value, |sum, item| sum.checked_add(item.len()))
        })
        .ok_or("CPU artifact size overflow")?;
    let mut existing = parent;
    while !existing.exists() {
        existing = existing.parent().ok_or("CPU artifact parent is absent")?;
    }
    require_space(existing, growth)?;
    std::fs::create_dir_all(parent)?;
    let temporary = tempfile::tempdir_in(parent)?;
    std::fs::write(
        temporary.path().join("kernels.c"),
        format!(
            "#include <stdint.h>\n#include <string.h>\n{}",
            wrappers.join("\n")
        ),
    )?;
    for (index, (source, object)) in sources.iter().zip(objects).enumerate() {
        std::fs::write(temporary.path().join(format!("kernel-{index}.txt")), source)?;
        if !object.is_empty() {
            std::fs::write(temporary.path().join(format!("kernel-{index}.o")), object)?;
        }
    }
    std::fs::write(temporary.path().join("kernels.so"), library)?;
    std::fs::write(temporary.path().join("weights.bin"), weights)?;
    std::fs::write(temporary.path().join("graph.json"), json)?;
    std::fs::rename(temporary.path(), destination)?;
    Ok(())
}

/// Python json.dumps(..., indent=2) ASCII encoding used by original artifact hashes.
pub fn python_pretty(value: &impl Serialize) -> std::result::Result<String, serde_json::Error> {
    let encoded = serde_json::to_string_pretty(value)?;
    let mut result = String::new();
    for character in encoded.chars() {
        if character as u32 >= 0x7f {
            for unit in character.encode_utf16(&mut [0; 2]) {
                use std::fmt::Write;
                write!(&mut result, "\\u{unit:04x}").expect("writing to String cannot fail");
            }
        } else {
            result.push(character);
        }
    }
    result.push('\n');
    Ok(result)
}

#[cfg(unix)]
fn require_space(
    path: &std::path::Path,
    growth: usize,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: statvfs initializes the output on success and retains neither pointer.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: the successful call initialized every field in stats.
    let stats = unsafe { stats.assume_init() };
    let free = u128::from(stats.f_bavail) * u128::from(stats.f_frsize);
    if free < (25u128 << 30) + growth as u128 {
        return Err("CPU artifact emission requires 25 GiB free plus artifact growth".into());
    }
    Ok(())
}
#[cfg(not(unix))]
fn require_space(
    _: &std::path::Path,
    _: usize,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Err("native model export requires Unix disk accounting".into())
}

#[cfg(test)]
mod emission_tests {
    use super::*;
    #[test]
    fn original_cpu_manifest_bytes_and_atomic_artifacts_match() {
        for fixture in [
            include_str!("../tests/fixtures/export/cpu-v1.json"),
            include_str!("../tests/fixtures/export/cpu-v2.json"),
        ] {
            let fixture = fixture.replace("x86_64", std::env::consts::ARCH);
            let graph: CpuGraph = serde_json::from_str(&fixture).unwrap();
            assert_eq!(python_pretty(&graph).unwrap(), fixture);
            let temp = tempfile::tempdir().unwrap();
            let destination = temp.path().join("bundle");
            emit_cpu(
                &destination,
                &graph,
                b"fixture-library",
                &[1, 2, 3, 4],
                &["source".into()],
                &[vec![1, 2]],
                &["wrapper".into()],
            )
            .unwrap();
            assert_eq!(
                std::fs::read_to_string(destination.join("graph.json")).unwrap(),
                fixture
            );
            assert_eq!(
                std::fs::read(destination.join("kernel-0.o")).unwrap(),
                [1, 2]
            );
            assert_eq!(
                std::fs::read_to_string(destination.join("kernels.c")).unwrap(),
                "#include <stdint.h>\n#include <string.h>\nwrapper"
            );
            assert!(emit_cpu(
                &destination,
                &graph,
                b"fixture-library",
                &[1, 2, 3, 4],
                &["source".into()],
                &[vec![1, 2]],
                &["wrapper".into()]
            )
            .is_err());
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        }
    }
    #[test]
    fn mismatched_weights_and_nonpartitioning_entrypoints_emit_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("bundle");
        let mut graph: CpuGraph = serde_json::from_str(
            &include_str!("../tests/fixtures/export/cpu-v2.json")
                .replace("x86_64", std::env::consts::ARCH),
        )
        .unwrap();
        assert!(emit_cpu(
            &destination,
            &graph,
            b"fixture-library",
            &[9; 4],
            &["source".into()],
            &[Vec::new()],
            &["wrapper".into()]
        )
        .is_err());
        graph.entrypoints[1].start = 1;
        assert!(emit_cpu(
            &destination,
            &graph,
            b"fixture-library",
            &[1, 2, 3, 4],
            &["source".into()],
            &[Vec::new()],
            &["wrapper".into()]
        )
        .is_err());
        assert!(!destination.exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    #[test]
    fn python_ascii_encoding_preserves_control_bmp_and_surrogate_pairs() {
        assert_eq!(
            python_pretty(&"\x7f한😀").unwrap(),
            "\"\\u007f\\ud55c\\ud83d\\ude00\"\n"
        );
    }
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub kind: String,
    pub camera: [u32; 2],
    pub sources: BTreeMap<String, String>,
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn files(
    root: &std::path::Path,
) -> std::result::Result<BTreeMap<std::path::PathBuf, String>, Box<dyn std::error::Error>> {
    fn visit(
        root: &std::path::Path,
        current: &std::path::Path,
        result: &mut BTreeMap<std::path::PathBuf, String>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let path = entry?.path();
            if path.is_symlink() {
                return Err(std::io::Error::other(
                    "immutable model generation contains symlink",
                ));
            }
            if path.is_dir() {
                visit(root, &path, result)?;
            } else if path.is_file() {
                result.insert(
                    path.strip_prefix(root)
                        .expect("walk stays within root")
                        .to_owned(),
                    file_digest(&path)?,
                );
            } else {
                return Err(std::io::Error::other("unexpected immutable model node"));
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result)?;
    Ok(result)
}
/// Publish freshly compiled bundles as one immutable generation. The emitter must
/// implement tensor lowering; this transaction does not replace that compiler.
pub fn publish(
    root: &std::path::Path,
    selections: &[Selection],
    backend: &str,
    mut emit: impl FnMut(
        &Selection,
        &std::path::Path,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>,
) -> std::result::Result<String, Box<dyn std::error::Error>> {
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    if selections.is_empty() || selections.len() > 16 || !["cpu", "qcom"].contains(&backend) {
        return Err("empty/excessive model selection or unsupported backend".into());
    }
    let mut keys = std::collections::BTreeSet::new();
    for selection in selections {
        if !["driving", "driver"].contains(&selection.kind.as_str())
            || ![[1928, 1208], [1344, 760]].contains(&selection.camera)
            || !keys.insert((&selection.kind, selection.camera))
            || selection.sources.is_empty()
            || selection
                .sources
                .iter()
                .any(|(role, digest)| role.is_empty() || !valid_digest(digest))
        {
            return Err("unsupported/duplicate model selection or missing source identity".into());
        }
    }
    let mut existing = root;
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or("model catalog has no existing ancestor")?;
    }
    require_space(existing, 256 * 1024 * 1024)?;
    std::fs::create_dir_all(root)?;
    let stage = tempfile::Builder::new()
        .prefix(".build-")
        .tempdir_in(root)?;
    let generation = stage.path().join("generation");
    std::fs::create_dir(&generation)?;
    let mut entries = Vec::new();
    for selection in selections {
        let directory = format!(
            "{}-{}x{}",
            selection.kind, selection.camera[0], selection.camera[1]
        );
        let destination = generation.join(&directory);
        emit(selection, &destination)?;
        if destination.is_symlink() {
            return Err("compiled model bundle is a symlink".into());
        }
        let graph = std::fs::read(destination.join("graph.json"))?;
        let descriptor = std::fs::read(destination.join("pipeline.json"))?;
        entries.push(json!({"kind":selection.kind,"camera":selection.camera,"directory":directory,"graph_sha256":format!("{:x}",Sha256::digest(graph)),"pipeline_sha256":format!("{:x}",Sha256::digest(descriptor))}));
    }
    let index = python_pretty(&json!({"version":1,"bundles":entries}))?;
    std::fs::write(generation.join("index.json"), &index)?;
    let digest = format!("{:x}", Sha256::digest(index));
    let destination = root.join(&digest);
    let expected = files(&generation)?;
    if destination.is_symlink() {
        return Err("existing generation is a symlink".into());
    }
    if destination.exists() {
        if files(&destination)? != expected {
            return Err("existing generation disagrees with immutable artifact".into());
        }
    } else {
        std::fs::rename(&generation, &destination)?;
    }
    let pointer = stage.path().join("current.json");
    let mut output = std::fs::File::create(&pointer)?;
    writeln!(output, "{{\"generation\": \"{digest}\", \"version\": 1}}")?;
    output.sync_all()?;
    drop(output);
    std::fs::rename(&pointer, root.join("current.json"))?;
    Ok(digest)
}

fn file_digest(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    fn selection() -> Selection {
        Selection {
            kind: "driver".into(),
            camera: [1344, 760],
            sources: BTreeMap::from([("onnx".into(), "12".repeat(32))]),
        }
    }
    fn emit(
        _: &Selection,
        path: &std::path::Path,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir(path)?;
        std::fs::write(path.join("graph.json"), "graph")?;
        std::fs::write(path.join("pipeline.json"), "pipeline")?;
        Ok(())
    }
    #[test]
    fn original_index_pointer_bytes_and_immutable_reuse_match() {
        let temp = tempfile::tempdir().unwrap();
        let digest = publish(temp.path(), &[selection()], "cpu", emit).unwrap();
        assert_eq!(
            std::fs::read_to_string(temp.path().join(&digest).join("index.json")).unwrap(),
            include_str!("../tests/fixtures/export/catalog-index.json")
        );
        let pointer = std::fs::read(temp.path().join("current.json")).unwrap();
        assert_eq!(
            pointer,
            include_bytes!("../tests/fixtures/export/catalog-current.json")
        );
        assert_eq!(
            publish(temp.path(), &[selection()], "cpu", emit).unwrap(),
            digest
        );
        assert_eq!(
            std::fs::read(temp.path().join("current.json")).unwrap(),
            pointer
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
    }
    #[test]
    fn duplicate_failed_emission_corruption_and_extra_files_preserve_pointer() {
        let temp = tempfile::tempdir().unwrap();
        let digest = publish(temp.path(), &[selection()], "cpu", emit).unwrap();
        let pointer = std::fs::read(temp.path().join("current.json")).unwrap();
        assert!(publish(temp.path(), &[selection(), selection()], "cpu", emit).is_err());
        assert!(publish(temp.path(), &[selection()], "cpu", |_, _| Err(
            "compile failed".into()
        ))
        .is_err());
        let bundle = temp.path().join(&digest).join("driver-1344x760");
        std::fs::write(bundle.join("extra"), "unexpected").unwrap();
        assert!(publish(temp.path(), &[selection()], "cpu", emit).is_err());
        std::fs::remove_file(bundle.join("extra")).unwrap();
        std::fs::write(bundle.join("graph.json"), "corrupted").unwrap();
        assert!(publish(temp.path(), &[selection()], "cpu", emit).is_err());
        assert_eq!(
            std::fs::read(temp.path().join("current.json")).unwrap(),
            pointer
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
    }
}
