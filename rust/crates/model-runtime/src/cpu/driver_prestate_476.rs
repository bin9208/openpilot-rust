//! #476: 실제 loaded cached-driver storage의 명시적 읽기 전 관측. 수학/초기화 변경 없음.
use super::{CpuModel, Invocation, KernelFn};
use crate::{assets::read_verified, buffer::Buffer, cached_code, Error, Graph};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const PHASES: [&str; 5] = [
    "loaded",
    "warmup-prepare",
    "warmup-model",
    "frame-0-prepare",
    "frame-0-model",
];

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn owned_directory(path: &Path, private: bool) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path)?;
    // SAFETY: geteuid takes no pointers and has no failure mode.
    let uid = unsafe { libc::geteuid() };
    if !path.is_absolute()
        || fs::canonicalize(path)? != path
        || !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.mode() & (if private { 0o077 } else { 0o022 }) != 0
    {
        return Err(Error::Contract("driver476 owned canonical0700 directory"));
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() > limit as u64 {
        return Err(Error::Limit("driver476 immutable metadata"));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() as u64 != before.len()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(Error::Contract("driver476 metadata changed"));
    }
    Ok(bytes)
}

fn actual_bytes(graph: &Graph, buffers: &[Buffer]) -> Result<Vec<Vec<u8>>, Error> {
    if buffers.len() != graph.allocations.len() || buffers.len() > 4096 {
        return Err(Error::Contract("driver476 actual allocation roster"));
    }
    let mut total = 0usize;
    let mut result = Vec::new();
    for (allocation, buffer) in graph.allocations.iter().zip(buffers) {
        total = total
            .checked_add(allocation.bytes)
            .ok_or(Error::Limit("driver476 allocation sum"))?;
        if total > MAX_BYTES {
            return Err(Error::Limit("driver476 actual bytes64MiB"));
        }
        let mut bytes = vec![0; allocation.bytes];
        buffer.read(0, &mut bytes)?;
        result.push(bytes);
    }
    Ok(result)
}

fn actual_calls(graph: &Graph, buffers: &[Buffer], calls: &[Invocation]) -> Result<Value, Error> {
    if calls.len() != graph.calls.len() || calls.len() > 4096 || graph.views.len() > 4096 {
        return Err(Error::Contract("driver476 actual call/view roster"));
    }
    let mut rows = Vec::new();
    for (index, (source, actual)) in graph.calls.iter().zip(calls).enumerate() {
        if actual.buffers.len() != source.views.len()
            || actual.workers != source.workers
            || actual.core_id != source.core_id
            || actual.scalars.len() != source.scalars.len()
        {
            return Err(Error::Contract("driver476 stored invocation ABI"));
        }
        let mut pointers = Vec::new();
        for (view, pointer) in source.views.iter().zip(&actual.buffers) {
            let v = &graph.views[*view];
            let expected = buffers[v.allocation].view_ptr(v.offset, v.bytes)?.cast();
            if *pointer != expected {
                return Err(Error::Contract("driver476 actual pointer/view alias"));
            }
            pointers.push(*pointer as usize);
        }
        for (slot, value) in actual.scalars.iter().enumerate() {
            if Some(slot) != actual.core_id && *value != source.scalars[slot] {
                return Err(Error::Contract("driver476 stored scalar changed"));
            }
        }
        let workers: Vec<_> = (0..actual.workers)
            .map(|worker| {
                let mut scalars = actual.scalars.clone();
                if let Some(slot) = actual.core_id {
                    scalars[slot] = i32::from(worker);
                }
                json!({"worker":worker,"scalars":scalars})
            })
            .collect();
        rows.push(
            json!({"call":index,"kernel":source.kernel,"views":source.views,
            "actual_pointers":pointers,"actual_function":actual.function as usize,
            "workers":workers,"core_id":actual.core_id,
            "pointer_bytes":std::mem::size_of::<*mut u8>(),"scalar_bytes":4}),
        );
    }
    Ok(json!(rows))
}

fn execute_observed(calls: &mut [Invocation], start: usize) -> Result<Value, Error> {
    let mut rows = Vec::new();
    for (index, call) in calls.iter_mut().enumerate() {
        for worker in 0..call.workers {
            if let Some(slot) = call.core_id {
                call.scalars[slot] = i32::from(worker);
            }
            if rows.len() >= 4096 {
                return Err(Error::Limit("driver476 actual dispatch4096"));
            }
            rows.push(json!({"call":start+index,"worker":worker,
                "actual_function":call.function as usize,
                "actual_pointers":call.buffers.iter().map(|p|*p as usize).collect::<Vec<_>>(),
                "scalars":call.scalars,"pointer_bytes":std::mem::size_of::<*mut u8>(),"scalar_bytes":4}));
            // SAFETY: exactly CpuModel::execute's validated, live, synchronous kernel ABI.
            unsafe { (call.function)(call.buffers.as_ptr(), call.scalars.as_ptr()) };
        }
    }
    Ok(json!(rows))
}

impl CpuModel {
    /// Observe the real post-core_id arguments and invoke the same live kernels once.
    pub fn run_driver_entry_476(
        &mut self,
        bundle: &Path,
        output: &Path,
        phase: &str,
        entry: &str,
        expected: &cached_code::Admission,
    ) -> Result<(), Error> {
        if !matches!(entry, "prepare" | "model") || !phase.ends_with(&format!("-{entry}")) {
            return Err(Error::Contract("driver476 observed entry/phase"));
        }
        self.write_driver_prestate_476(bundle, output, phase, expected)?;
        let index = crate::entrypoint::find(&self.graph.entrypoints, entry)?;
        let entry = &self.graph.entrypoints[index];
        let rows = execute_observed(&mut self.calls[entry.range()], entry.start)?;
        self._cached_library
            .as_ref()
            .ok_or(Error::Contract("driver476 cached library"))?
            .verify()
            .map_err(Error::CachedCode)?;
        let bytes = serde_json::to_vec_pretty(&rows)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(Error::Limit("driver476 dispatch4MiB"));
        }
        write_new(&output.join(phase).join("actual-dispatch.json"), &bytes)
    }

    /// Snapshot the actual loaded buffers before a driver entry executes.
    /// Requires the same independently authenticated admission as load_cached.
    /// No model bytes, inputs, pointers, scalars or execution order are changed.
    pub fn write_driver_prestate_476(
        &self,
        bundle: &Path,
        output: &Path,
        phase: &str,
        expected: &cached_code::Admission,
    ) -> Result<(), Error> {
        if !PHASES.contains(&phase)
            || self.graph.backend != "cpu-cached-llvm"
            || expected.programs.len() != 44
            || self.output_size("model")? != 2212
        {
            return Err(Error::Contract(
                "driver476 explicit whole44 cached driver required",
            ));
        }
        let held = self
            ._cached_library
            .as_ref()
            .ok_or(Error::Contract("driver476 loaded cached library required"))?;
        held.verify().map_err(Error::CachedCode)?;
        let descriptor: Value =
            serde_json::from_slice(&bounded(&bundle.join("pipeline.json"), 4 * 1024 * 1024)?)?;
        if descriptor["kind"] != "driver" {
            return Err(Error::Contract("driver476 driver descriptor required"));
        }
        let raw_graph = bounded(&bundle.join("graph.json"), 64 * 1024 * 1024)?;
        let disk_graph = Graph::parse(&raw_graph)?.graph;
        if serde_json::to_value(&disk_graph)? != serde_json::to_value(&self.graph)? {
            return Err(Error::Contract("driver476 loaded graph binding"));
        }
        let cached = self
            .graph
            .cached_code
            .as_ref()
            .ok_or(Error::Contract("driver476 cached graph absent"))?;
        let raw_certificate = read_verified(
            &bundle.join("cached-code.json"),
            &cached.certificate_sha256,
            4 * 1024 * 1024,
        )?;
        let certificate: cached_code::Certificate = serde_json::from_slice(&raw_certificate)?;
        if certificate.programs.len() != 44
            || certificate.original_model_sha256 != expected.original_model_sha256
            || certificate.original_manifest_sha256 != expected.original_manifest_sha256
            || certificate.current != expected.current
            || certificate
                .programs
                .iter()
                .any(|p| expected.programs.get(p.isa_proof_index) != Some(&p.original))
        {
            return Err(Error::Contract("driver476 independent whole44 binding"));
        }
        let bytes = actual_bytes(&self.graph, &self.buffers)?;
        let calls = actual_calls(&self.graph, &self.buffers, &self.calls)?;
        for (source, actual) in self.graph.calls.iter().zip(&self.calls) {
            let symbol = format!("op_kernel_{}\0", source.kernel);
            // SAFETY: load_cached already established this immutable library's fixed kernel ABI.
            let expected = unsafe { self._library.get::<KernelFn>(symbol.as_bytes()) }?;
            if actual.function as usize != *expected as usize {
                return Err(Error::Contract("driver476 actual function/source kernel"));
            }
        }
        if phase == "loaded" {
            owned_directory(
                output
                    .parent()
                    .ok_or(Error::Contract("driver476 output parent"))?,
                false,
            )?;
            DirBuilder::new().mode(0o700).create(output)?;
        }
        owned_directory(output, true)?;
        let directory = output.join(phase);
        DirBuilder::new().mode(0o700).create(&directory)?;
        let mut allocations = Vec::new();
        for (index, (allocation, raw)) in self.graph.allocations.iter().zip(&bytes).enumerate() {
            let filename = format!("allocation-{index}.bin");
            write_new(&directory.join(&filename), raw)?;
            allocations.push(json!({"allocation":index,"bytes":raw.len(),"file":filename,
                "sha256":sha(raw),"weight_offset":allocation.weight_offset,
                "actual_base":self.buffers[index].view_ptr(0, raw.len())? as usize}));
        }
        let views: Vec<_> = self.graph.views.iter().enumerate().map(|(index, view)| {
            json!({"view":index,"allocation":view.allocation,"offset":view.offset,"bytes":view.bytes,
                "sha256":sha(&bytes[view.allocation][view.offset..view.offset+view.bytes])})
        }).collect();
        let receipt = json!({"schema":"openpilot.driver476.actual-prestate.v1","phase":phase,
            "graph_sha256":sha(&raw_graph),"certificate_sha256":sha(&raw_certificate),
            "original_model_sha256":expected.original_model_sha256,
            "original_manifest_sha256":expected.original_manifest_sha256,
            "module_source_sha256":sha(include_bytes!("driver_prestate_476.rs")),
            "cpu_source_sha256":sha(include_bytes!("../cpu.rs")),
            "programs":certificate.programs,"allocations":allocations,"views":views,
            "calls":calls,"inputs":self.graph.inputs,"outputs":self.graph.outputs,
            "entrypoints":self.graph.entrypoints,"weights_sha256":self.graph.weights_sha256,
            "actual_loaded_storage":true,"kernels_executed_by_capture":0,"numeric_acceptance":false});
        let serialized = serde_json::to_vec_pretty(&receipt)?;
        if serialized.len() > 4 * 1024 * 1024 {
            return Err(Error::Limit("driver476 receipt4MiB"));
        }
        held.verify().map_err(Error::CachedCode)?;
        write_new(&directory.join("receipt.json"), &serialized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> Graph {
        serde_json::from_value(json!({"version":2,"backend":"cpu-llvm","arch":"x86_64",
            "weights_sha256":"0".repeat(64),"library_sha256":"0".repeat(64),
            "allocations":[{"bytes":8,"weight_offset":null}],
            "views":[{"allocation":0,"offset":0,"bytes":8},{"allocation":0,"offset":2,"bytes":4}],
            "inputs":[],"outputs":[],"kernels":[],"calls":[],"entrypoints":[]}))
        .unwrap()
    }

    #[test]
    fn snapshots_live_bytes_after_initialization_and_input_write_without_mutation() {
        let graph = graph();
        let mut buffers = vec![Buffer::new(8).unwrap()];
        assert_eq!(actual_bytes(&graph, &buffers).unwrap(), vec![vec![0; 8]]);
        buffers[0].write(2, &[3, 5, 7, 9]).unwrap();
        assert_eq!(
            actual_bytes(&graph, &buffers).unwrap(),
            vec![vec![0, 0, 3, 5, 7, 9, 0, 0]]
        );
        let mut after = [0; 8];
        buffers[0].read(0, &mut after).unwrap();
        assert_eq!(after, [0, 0, 3, 5, 7, 9, 0, 0]);
        assert!(actual_bytes(&graph, &[]).is_err());
    }

    #[test]
    fn byte_bound_and_duplicate_output_are_failures() {
        let mut graph = graph();
        graph.allocations[0].bytes = MAX_BYTES + 1;
        assert!(actual_bytes(&graph, &[Buffer::new(8).unwrap()]).is_err());
        let root = tempfile::tempdir().unwrap();
        write_new(&root.path().join("raw"), &[1, 2]).unwrap();
        assert!(write_new(&root.path().join("raw"), &[3]).is_err());
        assert_eq!(fs::read(root.path().join("raw")).unwrap(), [1, 2]);
    }

    #[test]
    fn aliases_and_worker_scalar_substitution_use_the_stored_invocation() {
        unsafe extern "C" fn never_called(_: *const *mut std::ffi::c_void, _: *const i32) {}
        let mut graph = graph();
        graph.calls.push(crate::graph::Call {
            kernel: 0,
            views: vec![0, 1],
            scalars: vec![0],
            workers: 2,
            core_id: Some(0),
        });
        let buffers = vec![Buffer::new(8).unwrap()];
        let mut calls = vec![Invocation {
            function: never_called,
            buffers: vec![
                buffers[0].view_ptr(0, 8).unwrap().cast(),
                buffers[0].view_ptr(2, 4).unwrap().cast(),
            ],
            scalars: vec![1],
            workers: 2,
            core_id: Some(0),
        }];
        let receipt = actual_calls(&graph, &buffers, &calls).unwrap();
        assert_eq!(
            receipt[0]["workers"],
            json!([{"worker":0,"scalars":[0]},{"worker":1,"scalars":[1]}])
        );
        assert_eq!(
            receipt[0]["actual_pointers"][1].as_u64().unwrap()
                - receipt[0]["actual_pointers"][0].as_u64().unwrap(),
            2
        );
        calls[0].buffers[1] = calls[0].buffers[0];
        assert!(actual_calls(&graph, &buffers, &calls).is_err());
    }

    #[test]
    fn actual_dispatch_preserves_worker_order_and_observes_post_substitution_arguments() {
        unsafe extern "C" fn write_worker(
            buffers: *const *mut std::ffi::c_void,
            scalars: *const i32,
        ) {
            // SAFETY: the test supplies one live8-byte buffer and one scalar, workers0/1 only.
            unsafe {
                let worker = *scalars;
                (*buffers)
                    .cast::<i32>()
                    .add(worker as usize)
                    .write(11 + worker);
            }
        }
        let buffer = Buffer::new(8).unwrap();
        let pointer = buffer.view_ptr(0, 8).unwrap().cast();
        let mut calls = vec![Invocation {
            function: write_worker,
            buffers: vec![pointer],
            scalars: vec![-1],
            workers: 2,
            core_id: Some(0),
        }];
        let observed = execute_observed(&mut calls, 3).unwrap();
        assert_eq!(observed[0]["call"], 3);
        assert_eq!(observed[0]["scalars"], json!([0]));
        assert_eq!(observed[1]["scalars"], json!([1]));
        let mut bytes = [0; 8];
        buffer.read(0, &mut bytes).unwrap();
        assert_eq!(i32::from_ne_bytes(bytes[..4].try_into().unwrap()), 11);
        assert_eq!(i32::from_ne_bytes(bytes[4..].try_into().unwrap()), 12);
    }
}
