#!/usr/bin/env python3
"""#476 Actions 전용 retained driver 후보. 임시 Python 실행 경계는 pending.

기본 진단은 이미 실패한 fresh gate의 status를 항상 반환한다. 후보 준비는 수용이 아니다.
실제 loaded storage와 독립 original prestate/ABI를 비교하며 metadata로 대체하지 않는다.
별도 --verify-retained-transport는 고정 운송 인증만 수행하며 모델/SDK를 실행하지 않는다.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import os
from pathlib import Path
import re
import resource
import signal
import stat
import struct
import subprocess
import sys
import time
import traceback
import types
import threading

MIB, GIB = 1024**2, 1024**3
COMMON = "tools/ci/controlsd_normal_523.py"
COMMON_SHA = "8a5da6caaf97dc00a0672e271130e0ce0ead8c055bee9883f7e503b9904b85a7"
LLVM_SHA = "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
MANIFEST_SHA = "1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380"
COLLECTOR_SHA = "532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795"
ORIGINAL_RAW = {
    "warmup": "ace8ee6c6ff7cf8aa6f1881dd9eae6dc9cba15bb75bcc7bd70a5af03ddef500e",
    "frame-0": "99b8b2fe7aa17098a366d1bb5e40a2b5a40561f0186ab4dcd33fb29a6dc358fc",
}
SELECTOR = "cached_driver_uses_actual_model_run_and_daemon_before_full_retirement"
ELFS = {
    "prepare-model-family-catalog", "offline-cached-code", "offline-llvm-guarded-add",
    "publish-model-catalog", "model_family_retirement", "check-driver-daemon",
    "openpilot-dmonitoringmodeld", "openpilot-logmessaged", "model-run",
}

# #476 기존 자연 host sample0의 원문 object witness. 기대 image는 이 archive가 아니라
# 고정 retired PKL에서만 읽는다. 현재 native decoder/host 검사 없이 실행하지 않는다.
RETAINED_MODEL_SHA = "33a8ae999035383fd33954483a2e7292a7193fad0baf7126b14a4061ce3e405c"
RETAINED_OBJECTS = {
    "artifact": 12393290469, "run": 39700512883, "bytes": 20550808,
    "workflow_source": "0e522bf2a968082195f08ffa4692a3f2ecf6d775",
    "checkout_source": "a29832fd0d046253465621a05f02a91e9cfdb1d1",
    "sha256": "a318335767594bfe8bcd08878f15e4f6a03bb0d8d13541d56f3b83243b19f461",
    "bundle": "driver-catalog/catalog/1dc28498bdbd261b337f9872f30d68fa9361ab79ebef816737d38a42e897a191/driver-1344x760/",
}

# 운송 provenance는 원본의 workflow/checkout provenance를 대체하지 않는다.
RETAINED_TRANSPORT = {
    "artifact": 12617590637, "name": "driver476-object-witness-12393290469.zip",
    "run": 40241602401, "attempt": 1,
    "source": "972468fab5279a9897d2d2bda736938c89a81a9a",
    "proof_artifact": 12618005602, "proof_name": "driver476-witness-provenance-40241602401-1",
    "proof_bytes": 9261,
    "proof_sha256": "f8d72c81b3bdb266e1e98b6d04fbfe242a9db62213c1e4a15ef727bfa337ff57",
}

# 기존 공개 Rust API만 연결하는 GHA 전용 비실행 leaf. Cargo/mod/WF 외 Rust 소스는
# 추가하지 않는다. 이 원문은 helper/source index와 컴파일 시 SHA로 결합된다.
RETAINED_ISA_BRIDGE = r'''
use openpilot_model_runtime::{cached_code, cached_elf, cached_isa};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Identity = (u64, u64, u32, u32, u32, u64, u64, i64, i64, i64, i64);
fn identity(m: &fs::Metadata) -> Identity {
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.uid(),
        m.gid(),
        m.nlink(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
struct Held {
    path: PathBuf,
    file: File,
    before: Identity,
    raw: Vec<u8>,
}
fn hold(path: &Path, limit: usize) -> Result<Held> {
    if !path.is_absolute() || fs::canonicalize(path)? != path {
        return Err("canonical retained input".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    let m = file.metadata()?;
    // SAFETY: geteuid takes no pointers and cannot fail.
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o022 != 0
        || m.len() > limit as u64
    {
        return Err("owned bounded retained input".into());
    }
    let before = identity(&m);
    let mut raw = Vec::new();
    Read::by_ref(&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut raw)?;
    let held = Held {
        path: path.to_owned(),
        file,
        before,
        raw,
    };
    if held.raw.len() as u64 != m.len()
        || identity(&held.file.metadata()?) != before
        || identity(&fs::symlink_metadata(path)?) != before
    {
        return Err("retained input changed".into());
    }
    Ok(held)
}
fn input(row: &Value, limit: usize, held: &mut Vec<Held>) -> Result<Vec<u8>> {
    let keys: Vec<_> = row
        .as_object()
        .ok_or("input pin object")?
        .keys()
        .map(String::as_str)
        .collect();
    if keys.len() != 3
        || !keys
            .iter()
            .all(|k| matches!(*k, "path" | "bytes" | "sha256"))
    {
        return Err("input pin exact keys".into());
    }
    let value = hold(Path::new(row["path"].as_str().ok_or("input path")?), limit)?;
    if row["bytes"].as_u64() != Some(value.raw.len() as u64)
        || row["sha256"].as_str() != Some(sha(&value.raw).as_str())
    {
        return Err("retained pin differs".into());
    }
    let result = value.raw.clone();
    held.push(value);
    Ok(result)
}
fn write_new(path: &Path, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("retained receipt4MiB".into());
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    Ok(())
}
fn run() -> Result<()> {
    for (key, value) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("GITHUB_JOB", "model-family-retirement"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", "X64"),
    ] {
        if env::var(key).as_deref() != Ok(value) {
            return Err("actual Actions identity".into());
        }
    }
    if std::env::consts::OS != "linux" || std::env::consts::ARCH != "x86_64" {
        return Err("actual native host".into());
    }
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("collection SHA output required".into());
    }
    let request = hold(Path::new(&args[0]), 4 * 1024 * 1024)?;
    if args[1].to_str() != Some(sha(&request.raw).as_str()) {
        return Err("collection explicit SHA".into());
    }
    let q: Value = serde_json::from_slice(&request.raw)?;
    let mut held = vec![request];
    if q["schema"] != 1
        || q["source_commit"].as_str() != Some(env::var("GITHUB_SHA")?.as_str())
        || q["model_PROGRAMs_executed"] != 0
        || q["source_compilations"] != 0
        || q["expected_from_native_fresh"] != false
    {
        return Err("inert original collection source".into());
    }
    let model = input(&q["model"], 128 * 1024 * 1024, &mut held)?;
    if model.len() != 13056699
        || sha(&model) != "33a8ae999035383fd33954483a2e7292a7193fad0baf7126b14a4061ce3e405c"
    {
        return Err("fixed retired driver PKL".into());
    }
    let warp_model = input(&q["warp_model"], 2 * 1024 * 1024, &mut held)?;
    if warp_model.len() != 1413261
        || sha(&warp_model) != "a860848b8e91f9739496b21b6cb1b7155212831aca288dd84963d05a3624df2d"
    {
        return Err("fixed retained driver warp PKL".into());
    }
    let manifest = input(&q["manifest"], 4 * 1024 * 1024, &mut held)?;
    if sha(&manifest) != "1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380" {
        return Err("fixed original manifest".into());
    }
    input(&q["initial_weights"], 64 * 1024 * 1024, &mut held)?;
    let rows = q["programs"].as_array().ok_or("PROGRAM rows")?;
    let warp_rows = q["warp_programs"].as_array().ok_or("warp PROGRAM rows")?;
    if rows.len() != 44 || warp_rows.len() != 1 {
        return Err("whole44 plus one original warp required".into());
    }
    let mut originals = Vec::new();
    let mut images = Vec::new();
    let mut texts = Vec::new();
    for (index, row) in rows.iter().chain(warp_rows).enumerate() {
        let local_index = if index < 44 { index } else { index - 44 };
        if row["index"].as_u64() != Some(local_index as u64) {
            return Err("PROGRAM index/order".into());
        }
        let source = input(&row["source"], 16 * 1024 * 1024, &mut held)?;
        let image = input(&row["image"], 16 * 1024 * 1024, &mut held)?;
        let object = input(&row["object"], 16 * 1024 * 1024, &mut held)?;
        let reconstructed = cached_elf::reconstruct(&object)?;
        if reconstructed.image != image {
            return Err(format!("PROGRAM{index} complete frozen image/object differs").into());
        }
        let original = cached_code::OriginalProgram {
            source_sha256: sha(&source),
            image_sha256: sha(&image),
            reconstruction_object_sha256: sha(&object),
            image_bytes: image.len(),
            image_alignment: reconstructed.alignment,
            text_bytes: reconstructed.text_bytes,
            pointers: row["pointers"].as_u64().ok_or("pointers")? as usize,
            scalars: row["scalars"].as_u64().ok_or("scalars")? as usize,
            global_slots: serde_json::from_value(row["global_slots"].clone())?,
            workers: u16::try_from(row["workers"].as_u64().ok_or("workers")?)?,
            core_id: serde_json::from_value(row["core_id"].clone())?,
        };
        cached_code::verify_original_program(&original, &source, &image, &object)?;
        texts.push(original.text_bytes);
        images.push(image);
        originals.push(original);
    }
    let references: Vec<_> = images
        .iter()
        .zip(&texts)
        .map(|(image, text)| (image.as_slice(), *text))
        .collect();
    let host = cached_isa::current_host();
    let proofs = cached_isa::admit_all(&references, 45, &host)?;
    let programs: Vec<_> = proofs
        .iter()
        .zip(&originals)
        .enumerate()
        .map(|(index, (proof, original))| {
            json!({
        "index":if index<44 {index} else {index-44},"source_sha256":original.source_sha256,"image_sha256":original.image_sha256,
        "text_bytes":proof.text_bytes,"instructions":proof.instructions,"required":proof.required})
        })
        .collect();
    for guard in &mut held {
        guard.file.seek(SeekFrom::Start(0))?;
        let mut after = Vec::new();
        Read::by_ref(&mut guard.file)
            .take(guard.raw.len() as u64 + 1)
            .read_to_end(&mut after)?;
        if after != guard.raw
            || identity(&guard.file.metadata()?) != guard.before
            || identity(&fs::symlink_metadata(&guard.path)?) != guard.before
        {
            return Err("held original/request changed".into());
        }
    }
    let output = Path::new(&args[2]);
    let metadata = fs::symlink_metadata(output)?;
    if !output.is_absolute()
        || fs::canonicalize(output)? != output
        || !metadata.is_dir()
        || metadata.mode() & 0o077 != 0
    {
        return Err("private receipt directory".into());
    }
    write_new(
        &output.join("isa.json"),
        &json!({"status":"ALL44_INERT_ISA_ADMISSION","generated_code_executed":false,
        "ABI_or_numeric_acceptance":false,"host_features":host,"programs":programs[..44]}),
    )?;
    write_new(
        &output.join("original-programs.json"),
        &json!(originals[..44]),
    )?;
    write_new(
        &output.join("warp-isa.json"),
        &json!({"status":"COMPLETE_INERT_ISA_ADMISSION","generated_code_executed":false,
        "ABI_or_numeric_acceptance":false,"host_features":host,"programs":programs[44..]}),
    )?;
    write_new(
        &output.join("native-api-binding.json"),
        &json!({"source_commit":env::var("GITHUB_SHA")?,
        "bridge_source_sha256":env!("DRIVER476_ADMISSION_SOURCE_SHA256"),"collection_sha256":sha(&held[0].raw),
        "model_sha256":sha(&model),"warp_model_sha256":sha(&warp_model),"warp_programs":originals[44..],
        "manifest_sha256":sha(&manifest),"initial_weights":q["initial_weights"],
        "whole44_triplets":true,"actual_host_isa":true,"generated_code_executed":false,"numeric_acceptance":false}),
    )?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("driver476 retained API: {error}");
        std::process::exit(1)
    }
}
'''


def retained_collection_worker(request, expected_sha):
    """고정 PKL의 SOURCE/BINARY/ABI/초기 weights 수집. PROGRAM 실행/컴파일은 금지한다."""
    root, job = canonical(os.environ['CI296_SOURCE'], True), canonical(os.environ['CI_JOB_TEMP'], True)
    need(os.environ.get('GITHUB_ACTIONS') == 'true'
         and os.environ.get('GITHUB_REPOSITORY') == 'bin9208/openpilot-rust'
         and os.environ.get('GITHUB_JOB') == 'model-family-retirement'
         and os.environ.get('DRIVER476_SOURCE_ALLOWED') == 'true'
         and os.environ.get('RUNNER_ENVIRONMENT') == 'github-hosted'
         and os.environ.get('RUNNER_OS') == 'Linux' and os.environ.get('RUNNER_ARCH') == 'X64'
         and os.uname().machine == 'x86_64' and not os.environ.get('LD_PRELOAD'), 'collection Actions trust')
    owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    need((job/'owner').read_text().strip() == owner
         and (root.parent/'.ci296-owner').read_text().strip() == owner, 'collection owner binding')
    os.umask(0o077)
    m = common(root)
    raw = m.read(canonical(request), 4*MIB)
    need(digest(raw) == expected_sha, 'collection request explicit SHA')
    q = m.unique_json(raw)
    need(set(q) == {'source_commit', 'preparation'} and q['source_commit'] == os.environ['GITHUB_SHA'], 'collection source')
    preparation = canonical(q['preparation'], True)
    need(preparation == job/'driver476-fresh', 'collection fixed owned fixture')
    ledger_raw = m.read(root/'rust/crates/dev-tools/tests/fixtures/driver-daemon-original/fixture.json', 4*MIB)
    need(digest(ledger_raw) == 'c55233339671d092fc7d67485e59c639b6fab1ffb03b1c4ece5138e4e9547a33', 'independent driver ledger')
    ledger = m.unique_json(ledger_raw)
    models = preparation/'driver-fixture/no-collector/models'
    holds = m.Holds()
    try:
        for name, pin in ledger['datasets']['no-collector']['models'].items():
            data = m.read(models/name, 16*MIB)
            need(len(data) == pin['bytes'] and digest(data) == pin['sha256'], 'collection immutable model roster')
            holds.add(models/name, pin['sha256'])
        need(ledger['datasets']['no-collector']['models']['dmonitoring_model_tinygrad.pkl']['sha256']
             == RETAINED_MODEL_SHA, 'retired PKL identity')
        manifest = preparation/'driver-fixture/no-collector/observations/reference-manifest.json'
        holds.add(manifest, MANIFEST_SHA)
        for path in (root, root/'tinygrad_repo', root/'rust/tools'): sys.path.insert(0, str(path))
        os.environ.update(DEV='CPU:LLVM', JIT='1', JIT_BATCH_SIZE='0', OPENPILOT_HACKS='1', CPU_COUNT='2',
                          CACHEDB=str(job/'driver476-frozen/collection-cache.db'))
        from tinygrad.device import Compiler
        from tinygrad.engine.jit import _prepare_jit_inputs
        from tinygrad.engine.realize import resolve_params
        from tinygrad.runtime.ops_cpu import CPUProgram, CPUComputeQueue
        from tinygrad.runtime.support.compiler_cpu import LLVMCompiler, ClangCompiler
        from tinygrad.uop.ops import Ops
        from model_export.binding_order import ordered_inputs
        from model_export.buffers import BufferTable, host_buffer
        from model_export.original import DriverArtifacts, driver
        from model_export.pipeline import linear_calls

        def forbidden(*_args, **_kwargs):
            raise RuntimeError('retained collection attempted PROGRAM construction/execution or source compilation')
        Compiler.compile_cached = forbidden
        LLVMCompiler.compile_to_obj = forbidden
        ClangCompiler.compile_to_obj = forbidden
        CPUProgram.__init__ = forbidden
        CPUComputeQueue._exec = forbidden
        CPUComputeQueue.exec = forbidden
        original = driver(DriverArtifacts(models/'dmonitoring_model_tinygrad.pkl',
            models/'dm_warp_1344x760_tinygrad.pkl', models/'dmonitoring_model_metadata.pkl'))
        destination = job/'driver476-original-request'
        destination.mkdir(mode=0o700)

        def spec(path, data):
            m.write_new(path, data)
            return {'path': str(path), 'bytes': len(data), 'sha256': digest(data)}
        def collect_programs(stage, prefix):
            operations, programs, identities = {}, [], set()
            for call in linear_calls(stage.jit.captured.linear):
                op = call.src[0]
                need(op.op in (Ops.PROGRAM, Ops.COPY, Ops.SLICE), 'collection known operations')
                if op.op is not Ops.PROGRAM or op in operations: continue
                p = op.arg
                need(all(v.expr == 'core_id' for v in p.vars)
                     and all(type(v) is int for v in p.global_size) and tuple(p.global_size[1:]) == (1, 1)
                     and (p.local_size is None or tuple(p.local_size) == (1, 1, 1)), 'collection original static ABI')
                source, image = op.src[3].arg, op.src[4].arg
                need(isinstance(source, str) and isinstance(image, bytes) and 0 < len(image) <= 16*MIB
                     and 0 < len(source.encode()) <= 16*MIB, 'complete original SOURCE/BINARY')
                identity = (digest(source.encode()), digest(image), tuple(p.globals), len(p.vars),
                            p.global_size[0], p.runtimevars.get('core_id'))
                need(identity not in identities, 'ambiguous original PROGRAM identity')
                identities.add(identity)
                index = len(programs)
                operations[op] = index
                programs.append({'index': index, 'source': spec(destination/f'{prefix}source-{index}.ll', source.encode()),
                    'image': spec(destination/f'{prefix}image-{index}.bin', image), 'pointers': len(p.globals),
                    'scalars': len(p.vars), 'global_slots': list(p.globals), 'workers': p.global_size[0],
                    'core_id': p.runtimevars.get('core_id')})
            return operations, programs
        operations, programs = collect_programs(original.stages[1], '')
        _, warp_programs = collect_programs(original.stages[0], 'warp-')
        need(len(programs) == 44 and len(warp_programs) == 1, 'complete44 model and one retained warp PROGRAM')
        table = BufferTable()
        inputs, outputs = [], []
        for bindings, rows, base in ((original.bindings.inputs, inputs, True), (original.bindings.outputs, outputs, False)):
            for name, tensor in bindings.items():
                buffer = host_buffer(tensor.uop.base.buffer if base else tensor.uop.buffer)
                table.read(buffer)
                rows.append({'name': name, 'view': table.view(buffer)})
        captured = original.pipeline.jit.captured
        input_uops, variables, _, info = _prepare_jit_inputs(ordered_inputs(captured.expected_names, original.bindings.inputs), {})
        need(not variables and info == captured.expected_input_info, 'original pipeline input binding')
        calls, kernels = [], {}
        for call in captured.linear.src:
            op = call.src[0]
            buffers = [host_buffer(u.buffer) for u in resolve_params(call, tuple(input_uops))]
            if op.op is Ops.SLICE:
                need(len(buffers) == 2 and buffers[0].base is buffers[1].base, 'collection SLICE alias')
                table.view(buffers[0]); continue
            if op.op is Ops.COPY:
                need(len(buffers) == 2 and buffers[0].nbytes == buffers[1].nbytes, 'collection COPY extent')
                key = ('copy', buffers[0].nbytes)
                reads, writes, slots, workers, scalars, core_id = [1], [0], [0, 1], 1, [], None
            else:
                need(op.op is Ops.PROGRAM, 'collection supported composed operation')
                p = op.arg
                key = ('cached', operations[op]) if op in operations else ('fresh', op.src[3].arg)
                reads, writes, slots = p.ins, p.outs, p.globals
                workers, scalars, core_id = p.global_size[0], [0]*len(p.vars), p.runtimevars.get('core_id')
            kernels.setdefault(key, len(kernels))
            for i in reads: table.read(buffers[i])
            for i in writes: table.write(buffers[i])
            calls.append({'kernel': kernels[key], 'views': [table.view(buffers[i]) for i in slots],
                          'scalars': scalars, 'workers': workers, 'core_id': core_id})
        allocations, weights = table.weights()
        need(sum(a.bytes for a in allocations) <= 64*MIB and len(weights) <= 64*MIB, 'original initial storage64MiB')
        weight_input = spec(destination/'initial-weights.bin', weights)
        holds.verify()
        imported = {}
        base = Path(os.environ['ORIGINAL_PYTHON_BASE'])
        for name, module in sorted(sys.modules.items()):
            filename = getattr(module, '__file__', None)
            if filename:
                path = Path(filename).resolve(strict=True)
                need(path.is_relative_to(root) or path.is_relative_to(job/'venv') or path.is_relative_to(base), 'collection import roots')
                imported[name] = {'path': str(path), **m.regular(path)}
        m.save(job/'proof/retained-collection-imports.json', imported)
        result = {'schema': 1, 'source_commit': q['source_commit'],
            'model': {'path': str(models/'dmonitoring_model_tinygrad.pkl'), **ledger['datasets']['no-collector']['models']['dmonitoring_model_tinygrad.pkl']},
            'manifest': {'path': str(manifest), 'bytes': manifest.stat().st_size, 'sha256': MANIFEST_SHA},
            'warp_model': {'path': str(models/'dm_warp_1344x760_tinygrad.pkl'), **ledger['datasets']['no-collector']['models']['dm_warp_1344x760_tinygrad.pkl']},
            'programs': programs, 'warp_programs': warp_programs, 'initial_weights': weight_input,
            'allocations': [{'bytes': a.bytes, 'weight_offset': a.weight_offset} for a in allocations],
            'views': [{'allocation': v.allocation, 'offset': v.offset, 'bytes': v.bytes} for v in table.views],
            'inputs': inputs, 'outputs': outputs, 'calls': calls,
            'entrypoints': [{'name': e.name, 'start': e.start, 'end': e.end} for e in original.pipeline.entrypoints],
            'model_PROGRAMs_executed': 0, 'source_compilations': 0, 'expected_from_native_fresh': False,
            'original_compiler_history_proven': False, 'ledger_sha256': digest(ledger_raw)}
        m.save(destination/'collection.json', result)
    finally:
        holds.close()


def need(value, message):
    if not value:
        raise RuntimeError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def canonical(path, directory=False, provider=False):
    path = Path(path)
    need(path.is_absolute() and path.resolve(strict=True) == path, f"canonical path required: {path}")
    info = path.lstat()
    need(info.st_uid in ({0, os.getuid()} if provider else {os.getuid()}) and not info.st_mode & 0o022
         and (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)),
         f"owned nonshared path required: {path}")
    return path


def interrupted(number, _frame):
    raise InterruptedError(f"driver476 interrupted by signal {number}")


def common(root):
    path = canonical(root / COMMON)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        before = os.fstat(fd)
        raw = os.pread(fd, 128*1024 + 1, 0)
        identity = lambda m: (m.st_dev, m.st_ino, m.st_mode, m.st_size, m.st_mtime_ns, m.st_ctime_ns)
        need(len(raw) == before.st_size <= 128*1024 and digest(raw) == COMMON_SHA
             and identity(before) == identity(os.fstat(fd)) == identity(path.lstat()), "common pin/identity")
    finally:
        os.close(fd)
    module = types.ModuleType("_driver476_owned")
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def authenticate_retained_transport(m, command, tools, frozen, holds):
    """고정 API 운송만 인증한다. 원본/모델/SDK/PROGRAM은 실행하지 않는다."""
    import io
    import zipfile
    from datetime import datetime
    from pathlib import PurePosixPath

    origin, transport = RETAINED_OBJECTS, RETAINED_TRANSPORT
    frozen = Path(frozen)
    canonical(frozen.parent, directory=True)
    frozen.mkdir(mode=0o700)
    canonical(frozen, directory=True)
    api = 'repos/bin9208/openpilot-rust/actions/'

    def get(endpoint, label):
        raw = command([tools['gh'], 'api', api+endpoint], label, 60)
        need(len(raw) <= MIB, 'transport API metadata1MiB bound')
        path = frozen/(label+'.json')
        m.write_new(path, raw)
        holds.add(path, digest(raw))
        return m.unique_json(raw)

    def epoch(value):
        need(type(value) is str and re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ', value), 'API UTC timestamp')
        return datetime.fromisoformat(value.replace('Z', '+00:00')).timestamp()

    def run_identity(row, source, run, attempt, event, workflow, branch):
        numbers = (row['id'], row['run_attempt'], row['repository']['id'], row['head_repository']['id'])
        need(all(type(n) is int for n in numbers)
             and row['id'] == run and row['head_sha'] == source and row['run_attempt'] == attempt
             and row['event'] == event and row['path'] == workflow and row['head_branch'] == branch
             and row['repository']['full_name'] == row['head_repository']['full_name'] == 'bin9208/openpilot-rust'
             and row['repository']['id'] == row['head_repository']['id'] == 1395501963, 'exact retained run identity')

    runmeta = get('runs/'+str(transport['run']), 'object-run-auth')
    run_identity(runmeta, transport['source'], transport['run'], transport['attempt'], 'workflow_dispatch',
                 '.github/workflows/python-rust-523-capacity.yml', 'codex/feat-523-python-rust')
    need(runmeta['status'] == 'completed' and runmeta['conclusion'] == 'success', 'completed successful preservation run')
    run_created = epoch(runmeta['created_at'])

    def artifact(row, identifier, name, size, sha):
        link = row['workflow_run']
        numbers = (row['id'], row['size_in_bytes'], link['id'], link['repository_id'], link['head_repository_id'])
        need(all(type(n) is int for n in numbers)
             and row['id'] == identifier and row['name'] == name and row['size_in_bytes'] == size
             and row['digest'] == 'sha256:'+sha and row['expired'] is False
             and link['id'] == transport['run'] and link['head_sha'] == transport['source']
             and link['head_branch'] == 'codex/feat-523-python-rust'
             and link['repository_id'] == link['head_repository_id'] == 1395501963, 'exact retained transport artifact')
        created, expires = epoch(row['created_at']), epoch(row['expires_at'])
        need(run_created <= created <= expires and run_created+90*86400 <= expires <= run_created+90*86400+600
             and time.time() < expires, 'live configured90d run-anchor retention')

    payload = get('artifacts/'+str(transport['artifact']), 'object-auth')
    proofmeta = get('artifacts/'+str(transport['proof_artifact']), 'object-proof-auth')
    artifact(payload, transport['artifact'], transport['name'], origin['bytes'], origin['sha256'])
    artifact(proofmeta, transport['proof_artifact'], transport['proof_name'], transport['proof_bytes'], transport['proof_sha256'])

    def download(identifier, name, size, sha, label):
        path = frozen/name
        # 별도 stdout 파일을 ZIP 복제본으로 남기거나 후속 proof upload에 섞지 않는다.
        command([tools['bash'], '-c',
                 'set -euo pipefail; umask 077; ulimit -f "$3"; set -o noclobber; exec "$1" api "$2" > "$4"',
                 label, tools['gh'], api+'artifacts/'+str(identifier)+'/zip', str((size+1023)//1024), path], label, 120)
        raw = m.read(path, size)
        need(len(raw) == size and digest(raw) == sha, 'whole retained download bytes/SHA')
        holds.add(path, sha)
        return raw

    proofraw = download(transport['proof_artifact'], 'transport-proof.zip', transport['proof_bytes'],
                        transport['proof_sha256'], 'object-proof-download')
    zraw = download(transport['artifact'], 'artifact.zip', origin['bytes'], origin['sha256'], 'object-download')

    def roster(z, count, expanded):
        infos = z.infolist()
        need(len(infos) == count and len({i.filename for i in infos}) == count
             and sum(i.file_size for i in infos) == expanded, 'exact retained ZIP roster/expanded bytes')
        for info in infos:
            name = PurePosixPath(info.filename)
            need(not name.is_absolute() and str(name) == info.filename and '\\' not in info.filename
                 and '..' not in name.parts and not info.is_dir() and not info.flag_bits & 1
                 and stat.S_IFMT(info.external_attr >> 16) in (0, stat.S_IFREG), 'regular canonical inert ZIP member')
        need(z.testzip() is None, 'complete retained ZIP CRC')
        return [i.filename for i in infos]

    with zipfile.ZipFile(io.BytesIO(proofraw)) as proof, zipfile.ZipFile(io.BytesIO(zraw)) as payload_zip:
        proof_names = roster(proof, 10, 85039)
        payload_names = roster(payload_zip, 258, 37492774)
        need(set(proof_names) == {'SOURCE_COMMIT', 'WORKFLOW_COMMIT', 'origin-artifact.json', 'origin-binding.json',
             'origin-crc.txt', 'origin-members.txt', 'origin-run.json', 'preservation-run.json',
             'preserved-artifact.json', 'roundtrip.json'}, 'fixed preservation proof members')

        def member(z, name, limit):
            need(z.getinfo(name).file_size <= limit, 'bounded proof member')
            return z.read(name)

        for name, value in [('SOURCE_COMMIT', origin['checkout_source']), ('WORKFLOW_COMMIT', origin['workflow_source'])]:
            expected = (value+'\n').encode()
            need(member(proof, name, 41) == member(payload_zip, name, 41) == expected, 'unchanged original source metadata')
        need(member(proof, 'origin-members.txt', 64*1024).decode().splitlines() == payload_names
             and b'No errors detected' in member(proof, 'origin-crc.txt', 64*1024), 'authenticated original roster/CRC record')
        objects = [n for n in payload_names if n.endswith('.o')]
        need(len(objects) == 90, 'complete original90-object witness')
        originmeta = m.unique_json(member(proof, 'origin-artifact.json', 64*1024))
        originrun = m.unique_json(member(proof, 'origin-run.json', 64*1024))
        binding = m.unique_json(member(proof, 'origin-binding.json', 64*1024))
        roundtrip = m.unique_json(member(proof, 'roundtrip.json', 64*1024))
        saved_run = m.unique_json(member(proof, 'preservation-run.json', 64*1024))
        saved_payload = m.unique_json(member(proof, 'preserved-artifact.json', 64*1024))
        run_identity(originrun, origin['workflow_source'], origin['run'], 1, 'push',
                     '.github/workflows/model-driver-cpu-context-296-once.yml', 'codex/feat-296-native-tools-candidate')
        need(originrun['status'] == 'completed' and originrun['conclusion'] == 'success', 'historical successful origin run')
        link = originmeta['workflow_run']
        need(originmeta['id'] == origin['artifact'] and originmeta['name'] == 'model-driver-cpu-context-296-0'
             and originmeta['size_in_bytes'] == origin['bytes'] and originmeta['digest'] == 'sha256:'+origin['sha256']
             and originmeta['expired'] is False and link['id'] == origin['run'] and link['head_sha'] == origin['workflow_source']
             and link['repository_id'] == link['head_repository_id'] == 1395501963
             and epoch(originmeta['created_at']) <= run_created < epoch(originmeta['expires_at']), 'origin alive when preserved')
        origin_fields = {k: origin[k] for k in ('artifact', 'run', 'bytes', 'sha256', 'workflow_source', 'checkout_source')}
        need(binding['schema'] == 'openpilot.driver476-witness-preservation.v1'
             and binding['status'] == 'AUTHENTICATED_ORIGIN_ONLY' and binding['origin'] == origin_fields
             and binding['preservation_source'] == transport['source'] and binding['owner'] == '40241602401:1:inventory'
             and binding['replay_executed'] is False and binding['consumer_binding_changed'] is False, 'historical preservation binding')
        run_identity(saved_run, transport['source'], transport['run'], transport['attempt'], 'workflow_dispatch',
                     '.github/workflows/python-rust-523-capacity.yml', 'codex/feat-523-python-rust')
        # 완료 전 snapshot 원문은 그대로 두고, 완료 판정은 위 live API에서만 한다.
        need(saved_run['status'] == 'in_progress' and saved_run['conclusion'] is None, 'unchanged pre-completion run snapshot')
        artifact(saved_payload, transport['artifact'], transport['name'], origin['bytes'], origin['sha256'])
        need(roundtrip['schema'] == 'openpilot.driver476-witness-roundtrip.v1' and roundtrip['status'] == 'ROUNDTRIP_VERIFIED'
             and roundtrip['artifact_id'] == str(transport['artifact']) and roundtrip['run_id'] == str(transport['run'])
             and roundtrip['source_commit'] == transport['source'] and roundtrip['bytes'] == origin['bytes']
             and roundtrip['sha256'] == origin['sha256'] and roundtrip['requested_retention_days'] == 90
             and roundtrip['endpoint'] == '/zip' and roundtrip['unwrapped'] is False
             and roundtrip['original_bytes_equal'] is True
             and roundtrip['expires_epoch'] == epoch(saved_payload['expires_at']), 'exact historical API roundtrip')
        old_receipt = m.unique_json(member(payload_zip, 'driver-catalog/current-catalog-receipt.json', 4*MIB))
        need(old_receipt['source_commit'] == origin['checkout_source']
             and old_receipt['models']['dmonitoring_model_tinygrad.pkl']['sha256'] == RETAINED_MODEL_SHA, 'historical checkout/model retained')

    receipt = {'schema': 'openpilot.driver476-transport-authentication.v1', 'source_commit': os.environ['GITHUB_SHA'],
        'transport_only': True,
        'origin_artifact': originmeta, 'origin_run': originrun, 'origin_witness': origin,
        'transport': {'binding': transport, 'artifact': payload, 'proof_artifact': proofmeta, 'run': runmeta},
        'preservation_roundtrip': roundtrip, 'whole_zip_verified': True, 'all258_CRC_verified': True,
        'original_objects': 90, 'original_metadata_unchanged': True, 'models_executed': 0,
        'original_executed': 0, 'model_programs_executed': 0, 'compiler_invocations': 0,
        'reuploaded_archives': 0, 'full_six_source_acceptance': False, 'retirement_credit': 0}
    m.save(frozen/'transport-authentication.json', receipt)
    holds.add(frozen/'transport-authentication.json')
    holds.verify()
    return receipt


def ranges_before_writes(calls, views, allocations):
    """원본 PROGRAM ins/outs view 계약에 따른 범위. instruction별 read/write 관측은 아니다."""
    written = [[] for _ in allocations]
    required = [[] for _ in allocations]
    for call in calls:
        for index in call["reads"]:
            view = views[index]
            spans = [(view["offset"], view["offset"]+view["bytes"])]
            for start, end in written[view["allocation"]]:
                spans = [(a, b) for lo, hi in spans for a, b in
                         ((lo, min(hi, start)), (max(lo, end), hi)) if a < b]
            required[view["allocation"]].extend(spans)
        for index in call["writes"]:
            view = views[index]
            written[view["allocation"]].append((view["offset"], view["offset"]+view["bytes"]))
    return required


def original_worker(request, expected_sha):
    """GHA 자식 프로세스: 원래 retained PROGRAM의 실제 bytes/ctypes ABI를 관측한다."""
    root, job = canonical(os.environ["CI296_SOURCE"],True), canonical(os.environ["CI_JOB_TEMP"],True)
    need(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("DRIVER476_SOURCE_ALLOWED") == "true"
         and os.environ.get("GITHUB_REPOSITORY")=='bin9208/openpilot-rust'
         and os.environ.get("GITHUB_JOB") == "model-family-retirement"
         and os.environ.get('RUNNER_OS')=='Linux' and os.environ.get('RUNNER_ARCH')=='X64'
         and os.uname().machine=='x86_64', "original worker Actions trust")
    need(os.environ.get('RUNNER_ENVIRONMENT')=='github-hosted' or
         (os.environ.get('RUNNER_ENVIRONMENT')=='self-hosted' and os.environ.get('RUNNER_NAME')=='bin-actions'),
         'original worker actual runner')
    owner=':'.join(os.environ[k] for k in ('GITHUB_RUN_ID','GITHUB_RUN_ATTEMPT','GITHUB_JOB'))
    need((job/'owner').read_text().strip()==owner and (root.parent/'.ci296-owner').read_text().strip()==owner,
         'original worker owner binding')
    os.umask(0o077)
    m = common(root)
    qraw = m.read(canonical(request), 4*MIB)
    need(digest(qraw) == expected_sha, "original worker request pin")
    q = m.unique_json(qraw)
    need(q["source_commit"] == os.environ["GITHUB_SHA"], "original worker source")
    out = job/"driver-retained476"
    prep = out/"preparation"
    destination = out/"proof/original-prestate"
    destination.mkdir(mode=0o700)
    catalog = prep/"driver-catalog/catalog"
    current = m.unique_json(m.read(catalog/"current.json"))
    generation = current["generation"]
    need(re.fullmatch('[0-9a-f]{64}', generation), "original worker catalog generation")
    bundle = catalog/generation/"driver-1344x760"
    graph = m.unique_json(m.read(bundle/"graph.json"))
    original_request = m.unique_json(m.read(Path(q["cached_request"]["path"]), 4*MIB))
    need(digest(m.read(Path(q["cached_request"]["path"]), 4*MIB)) == q["cached_request"]["sha256"], "worker cached request pin")
    preflight_path = canonical(q['retained_preflight']['path'])
    need(preflight_path == Path(q['cached_request']['path']).parent/'native-api-binding.json', 'worker retained preflight path')
    preflight_raw = m.read(preflight_path, 4*MIB)
    need(digest(preflight_raw) == q['retained_preflight']['sha256'], 'worker retained preflight pin')
    preflight = m.unique_json(preflight_raw)
    need(preflight['source_commit'] == q['source_commit'] and preflight['actual_host_isa'] is True
         and preflight['generated_code_executed'] is False and preflight['whole44_triplets'] is True
         and len(preflight['warp_programs']) == 1
         and preflight['bridge_source_sha256'] == original_request['admission']['current']['isa_verifier_source_sha256'],
         'original PROGRAMs require current native whole44/warp admission before execution')
    for path in (root, root/"tinygrad_repo", root/"rust/tools"):
        sys.path.insert(0, str(path))
    os.environ.update(DEV="CPU:LLVM", JIT="1", JIT_BATCH_SIZE="0", OPENPILOT_HACKS="1", CPU_COUNT="2")
    from tinygrad import Device
    from tinygrad.device import Compiler
    from tinygrad.engine.jit import _prepare_jit_inputs
    from tinygrad.engine.realize import resolve_params
    from tinygrad.runtime.support.compiler_cpu import LLVMCompiler, ClangCompiler
    from tinygrad.runtime.ops_cpu import CPUComputeQueue
    from tinygrad.uop.ops import Ops
    from model_export.original import DriverArtifacts, driver
    from model_export.binding_order import ordered_inputs
    from model_export.buffers import BufferTable, host_buffer

    def no_compile(*_args, **_kwargs):
        raise RuntimeError("original retained replay attempted source compilation")
    LLVMCompiler.compile_to_obj = no_compile
    ClangCompiler.compile_to_obj = no_compile
    Compiler.compile_cached = no_compile
    models = prep/"driver-fixture/no-collector/models"
    need(digest(m.read(models/'dmonitoring_model_tinygrad.pkl',128*MIB))
         ==original_request['admission']['original_model_sha256'], 'actual loaded original PKL pin')
    need(digest(m.read(models/'dm_warp_1344x760_tinygrad.pkl',2*MIB)) == preflight['warp_model_sha256']
         == 'a860848b8e91f9739496b21b6cb1b7155212831aca288dd84963d05a3624df2d', 'actual admitted original warp PKL')
    original = driver(DriverArtifacts(models/"dmonitoring_model_tinygrad.pkl",
        models/"dm_warp_1344x760_tinygrad.pkl", models/"dmonitoring_model_metadata.pkl"))
    captured = original.pipeline.jit.captured
    inputs, variables, _, info = _prepare_jit_inputs(ordered_inputs(captured.expected_names, original.bindings.inputs), {})
    need(not variables and info == captured.expected_input_info, "actual original pipeline binding")
    table = BufferTable()
    input_bindings, output_bindings = [], []
    for name, tensor in original.bindings.inputs.items():
        input_bindings.append({"name":name,"view":table.view(host_buffer(tensor.uop.base.buffer))})
    for name, tensor in original.bindings.outputs.items():
        output_bindings.append({"name":name,"view":table.view(host_buffer(tensor.uop.buffer))})
    originals = original_request["admission"]["programs"]
    need(len(originals) == 44, "original whole44 admission")
    by_key = {(p["source_sha256"], p["image_sha256"], tuple(p["global_slots"]), p["scalars"],
               p["workers"], p["core_id"]): i for i,p in enumerate(originals)}
    warp_keys = {(p['source_sha256'],p['image_sha256'],tuple(p['global_slots']),p['scalars'],p['workers'],p['core_id'])
                 for p in preflight['warp_programs']}
    need(len(by_key) == 44, "original unique source/image/ABI")
    calls, seen, warp_seen = [], set(), set()
    for call in captured.linear.src:
        operation = call.src[0]
        buffers = [host_buffer(u.buffer) for u in resolve_params(call, tuple(inputs))]
        if operation.op is Ops.SLICE:
            need(buffers[0].base is buffers[1].base, "original slice alias")
            table.view(buffers[0]); continue
        if operation.op is Ops.COPY:
            reads, writes, slots = [1], [0], [0,1]
            source_sha = image_sha = None
            workers, scalars, core_id, program = 1, [], None, None
        else:
            need(operation.op is Ops.PROGRAM, "original supported PROGRAM/COPY only")
            p = operation.arg
            reads, writes, slots = p.ins, p.outs, p.globals
            source_sha, image_sha = digest(operation.src[3].arg.encode()), digest(operation.src[4].arg)
            workers, scalars, core_id = p.global_size[0], [0]*len(p.vars), p.runtimevars.get('core_id')
            key = (source_sha, image_sha, tuple(slots), len(scalars), workers, core_id)
            program = by_key.get(key)
        r = [table.view(buffers[i]) for i in reads]
        w = [table.view(buffers[i]) for i in writes]
        views = [table.view(buffers[i]) for i in slots]
        index = len(calls)
        native = graph['calls'][index]
        need(views == native['views'] and scalars == native['scalars'] and workers == native['workers']
             and core_id == native['core_id'], "original/native call order/alias/ABI")
        if native['kernel'] in graph['cached_code']['kernels']:
            need(program is not None, "model PROGRAM absent from whole44 independent roster")
            seen.add(program)
        else:
            program = None
            if source_sha is not None:
                need(key in warp_keys, 'original warp image/ABI absent from native host admission')
                warp_seen.add(key)
                need(digest(m.read(bundle/f"kernel-{native['kernel']}.txt", 16*MIB)) == source_sha,
                     "fresh warp SOURCE differs from original")
        calls.append({'call':index,'kernel':native['kernel'],'views':views,'reads':r,'writes':w,
                      'workers':workers,'scalars':scalars,'core_id':core_id,
                      'source_sha256':source_sha,'image_sha256':image_sha,'original_index':program})
    views = [{'view':i,'allocation':v.allocation,'offset':v.offset,'bytes':v.bytes} for i,v in enumerate(table.views)]
    need(seen == set(range(44)) and warp_seen == warp_keys and len(calls) == len(graph['calls']) and input_bindings == graph['inputs']
         and output_bindings == graph['outputs'], "complete original program/input/output roster")
    need([{k:v for k,v in row.items() if k!='view'} for row in views] == graph['views'], "whole actual original view graph")
    need([b.nbytes for b in table.bases] == [a['bytes'] for a in graph['allocations']], "whole original base extents")

    def live(base):
        base.ensure_allocated()
        kind = base.device.split(':')[0]
        if kind == 'CPU': view = base.as_memoryview(force_zero_copy=True)
        elif kind == 'NPY': view = memoryview(base._buf).cast('B')
        elif kind == 'PYTHON': view = memoryview(base._buf).cast('B')
        else: raise RuntimeError('original actual host storage required')
        need(len(view) == base.nbytes and base.nbytes > 0, 'actual base byte extent')
        return view

    entry_calls = {e.name:calls[e.start:e.end] for e in original.pipeline.entrypoints}
    events, lock = [], threading.Lock()
    real_exec = CPUComputeQueue._exec
    def observe(queue, tid, prg, bufs, *args):
        def actual(*machine_args):
            row={'worker':tid,'image_sha256':digest(bytes(prg.mem)),
                 'pointers':[v.value for v in machine_args[:bufs]],'scalars':[v.value for v in machine_args[bufs:]],
                 'pointer_bytes':[ctypes.sizeof(v) for v in machine_args[:bufs]],
                 'scalar_bytes':[ctypes.sizeof(v) for v in machine_args[bufs:]]}
            with lock: events.append(row)
            return prg.fxn(*machine_args)
        return real_exec(queue, tid, types.SimpleNamespace(runtimevars=prg.runtimevars,fxn=actual), bufs, *args)
    CPUComputeQueue._exec = observe

    def capture(phase, selected):
        Device['CPU'].synchronize()
        directory = destination/phase
        directory.mkdir(mode=0o700)
        total, allocations = 0, []
        for i,base in enumerate(table.bases):
            view=live(base); raw=bytes(view); total+=len(raw)
            need(total<=64*MIB, 'original prestate64MiB')
            filename=f'allocation-{i}.bin'; m.write_new(directory/filename,raw)
            allocations.append({'allocation':i,'bytes':len(raw),'file':filename,'sha256':digest(raw),
                                'actual_base':ctypes.addressof(ctypes.c_ubyte.from_buffer(view))})
        m.save(directory/'receipt.json', {'schema':'openpilot.driver476.original-prestate.v1','phase':phase,
            'allocations':allocations,'views':views,'calls':calls,'inputs':input_bindings,'outputs':output_bindings,
            'read_before_write_ranges':ranges_before_writes(selected,views,allocations),
            'range_contract':'original declared ins/outs view ranges, not instruction-level byte coverage',
            'original_model_sha256':original_request['admission']['original_model_sha256'],
            'original_manifest_sha256':original_request['admission']['original_manifest_sha256'],
            'source_commit':q['source_commit'],
            'whole44':True,'actual_storage':True,'numeric_acceptance':False})

    capture('loaded',calls)
    data = Path(q['preparation'])/'driver-first-frame-diagnostic'
    executed=set()
    for label in ('warmup','frame-0'):
        for name,filename in (('frame',f'{label if label=="frame-0" else "warmup"}.bin'),
                              ('transform',f'{label}-transform.bin'),('calib',f'{label}-calib.bin')):
            raw=m.read(data/filename,4*MIB)
            buffer=original.bindings.inputs[name].uop.base.buffer
            need(len(raw)==buffer.nbytes,'original input complete bytes')
            buffer.ensure_allocated()
            if buffer.device=='NPY': live(buffer)[:]=raw
            else: buffer.copyin(memoryview(raw))
        for stage in original.stages:
            capture(f'{label}-{stage.name}',entry_calls[stage.name])
            events.clear()
            stage_inputs,variables,_,_= _prepare_jit_inputs(ordered_inputs(stage.jit.captured.expected_names,stage.inputs),{})
            need(not variables,'original stage scalar contract')
            stage.jit.captured(stage_inputs,variables)
            Device['CPU'].synchronize()
            pending=list(events)
            for call in entry_calls[stage.name]:
                if call['image_sha256'] is None: continue
                group,pending=pending[:call['workers']],pending[call['workers']:]
                need(sorted(e['worker'] for e in group)==list(range(call['workers'])),'actual original worker roster')
                for e in group:
                    pointers=[allocations_pointer(table,views[v],live) for v in call['views']]
                    scalars=list(call['scalars'])
                    if call['core_id'] is not None: scalars[call['core_id']]=e['worker']
                    need(e['image_sha256']==call['image_sha256'] and e['pointers']==pointers
                         and e['scalars']==scalars and e['pointer_bytes']==[8]*len(pointers)
                         and e['scalar_bytes']==[4]*len(scalars),'actual retained image/worker machine ABI')
                if call['original_index'] is not None: executed.add(call['original_index'])
            need(not pending,'unaccounted original worker invocations')
            m.save(destination/f'{label}-{stage.name}-actual-abi.json',events)
        raw=original.bindings.outputs['model'].numpy().reshape(-1).tobytes()
        need(len(raw)==2212 and digest(raw)==ORIGINAL_RAW[label],'observed original whole raw differs from frozen')
        m.write_new(destination/f'{label}-original.raw',raw)
    need(executed==set(range(44)),'actual complete44 original runtime PROGRAM execution')
    m.save(destination/'complete.json',{'actual_original':True,'whole44':True,'retained_warp_programs':len(warp_seen),'frames':2,
        'actual_model_programs':44,'source_commit':q['source_commit'],'request_sha256':expected_sha,
        'graph_sha256':digest(m.read(bundle/'graph.json')),
        'generated_original_programs_executed':True,'numeric_acceptance':False})


def allocations_pointer(table, view, live):
    base=live(table.bases[view['allocation']])
    return ctypes.addressof(ctypes.c_ubyte.from_buffer(base))+view['offset']


class Candidate:
    def __init__(self, args, root, job, m):
        self.args, self.root, self.job, self.m = args, root, job, m
        self.out = job / "driver-retained476"
        self.out.mkdir(mode=0o700)  # 재실행/덮어쓰기 금지. 부모가 upload 뒤 회수한다.
        self.proof = self.out / "proof"
        self.proof.mkdir(mode=0o700)
        self.holds, self.children = m.Holds(), m.Children(self.proof / "children.jsonl")
        self.baselines, self.commands = {}, []
        self.required_baselines = {"source", "sdk", "venv", "python-base"}
        self.selector_runs, self.prepared = 0, False
        self.prestate_verified, self.candidate_numeric = False, False
        self.phase = "admission"
        self.deadline = min(time.monotonic() + 2400,
                            time.monotonic() + args.deadline_epoch - time.time())
        self.env = os.environ.copy()

    def read(self, path, limit=4*MIB):
        return self.m.unique_json(self.m.read(path, limit))

    def pin(self, row, elf=False, limit=128*MIB, provider=False, python=False):
        need(set(row) == {"path", "sha256"} and re.fullmatch("[0-9a-f]{64}", row["sha256"]), "explicit pin")
        alias = Path(row["path"])
        path = canonical(alias.resolve(strict=True) if python else alias, provider=provider)
        need(self.m.regular(path, limit)["sha256"] == row["sha256"], f"pin differs: {path}")
        self.holds.add(alias, row["sha256"], elf=elf)
        return alias if python else path

    def baseline(self, name, path, paths=None, sdk=False):
        value = self.m.snapshot(path, paths if paths is not None else self.m.walk(path), sdk=sdk)
        if name in self.baselines:
            need(self.baselines[name] == (path, paths, sdk, value), f"earlier baseline changed: {name}")
            return
        self.baselines[name] = (path, paths, sdk, value)
        self.m.save(self.proof / f"{name}.before.json", value)

    def budget(self):
        need(time.monotonic() < self.deadline, "shared2400s/parent deadline")
        total = 0
        for base in (self.out, self.job/"proof"):
            for path in self.m.walk(base, mutable=True):
                try:
                    info = path.lstat()
                except FileNotFoundError:
                    continue  # mutable metadata only; immutable input snapshots remain strict.
                need(not stat.S_ISLNK(info.st_mode), "candidate/proof output symlink")
                need(stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode), "candidate/proof output type")
                total += max(info.st_size, info.st_blocks*512)
                need(total <= 2*GIB, "combined candidate/proof2GiB bound")
        fs = os.statvfs(self.out)
        need(fs.f_bavail*fs.f_frsize >= 2*GIB-total+512*MIB, "candidate remaining growth/reserve")

    def command(self, argv, label, seconds, env=None, allow_failure=False):
        self.phase = label
        need(not self.children.scan(), "live child before command")
        self.budget()
        started, process = time.monotonic(), None
        try:
            def limits():
                resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
                resource.setrlimit(resource.RLIMIT_FSIZE, (512*MIB, 512*MIB))
            with (self.proof/f"{label}.stdout").open("xb") as stdout, \
                    (self.proof/f"{label}.stderr").open("xb") as stderr:
                process = subprocess.Popen(list(map(str, argv)), cwd=self.root, env=env or self.env,
                    stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                    start_new_session=True, preexec_fn=limits)
                self.children.process = process
                while True:
                    self.children.reap()
                    self.budget()
                    need(time.monotonic() < started+seconds, f"{label} deadline")
                    need(stdout.tell() <= 32*MIB and stderr.tell() <= 32*MIB, "stream32MiB bound")
                    if process.returncode is not None:
                        need(not self.children.scan(), "child survived leader")
                        need(allow_failure or process.returncode == 0, f"{label} exit{process.returncode}")
                        break
                    time.sleep(0.1)
        finally:
            try:
                self.children.stop()
            finally:
                self.commands.append({"label": label, "argv": list(map(str, argv)),
                    "returncode": process.returncode if process else None,
                    "elapsed_seconds": time.monotonic()-started})
                self.children.process = None
        return self.m.read(self.proof/f"{label}.stdout", 32*MIB)

    def admit(self):
        self.required_baselines.update({"driver-fixture", "driver-catalog",
                                        "driver-first-frame-diagnostic", "peer", "cached-inputs"})
        request_path = canonical(self.args.request)
        need(request_path.is_relative_to(self.job/"proof"), "parent-owned request proof required")
        self.pin({"path": str(request_path), "sha256": self.args.request_sha256}, limit=4*MIB)
        q = self.read(request_path)
        need(set(q) == {"schema", "source_commit", "source_index", "preparation", "sdk",
                        "python", "python_base", "llvm", "elfs", "cached_request", "retained_preflight"}
             and q["schema"] == 1, "request schema1")
        need(q["source_commit"] == os.environ["GITHUB_SHA"], "request source SHA")
        index = self.pin(q["source_index"], limit=4*MIB)
        need(index.is_relative_to(self.job / "proof"), "parent index proof required")
        names = self.m.read(index, 4*MIB).split(b"\0")
        need(names[-1] == b"" and len(names) > 1 and len(set(names[:-1])) == len(names)-1, "exact NUL source roster")
        need(COMMON.encode() in names and b"tools/ci/driver_retained_program_476.py" in names, "tracked controller/common source required")
        paths = [self.root]
        for raw in names[:-1]:
            path = Path(os.fsdecode(raw))
            need(not path.is_absolute() and all(p not in ("", ".", "..") for p in path.parts), "relative source index")
            paths.append(self.root/path)
        self.baseline("source", self.root, paths)
        head = self.command(["/usr/bin/git", "rev-parse", "HEAD"], "source-head", 10).decode().strip()
        need(head == os.environ["GITHUB_SHA"], "actual source HEAD")
        self.sdk = canonical(q["sdk"], directory=True)
        need(self.sdk == Path(os.environ["CI_PREPARED_RUST_ROOT"])
             and self.sdk.name == "1.94.0-x86_64-unknown-linux-gnu", "actual prepared SDK1.94 binding")
        for program, expected in self.m.SDK_PINS.items():
            self.holds.add(self.sdk/"bin"/program, expected, elf=True)
        self.baseline("sdk", self.sdk, sdk=True)
        self.python = self.pin(q["python"], elf=True, provider=True, python=True)
        need(self.python.is_relative_to(self.job/"venv/bin"), "existing private venv Python required")
        self.llvm = self.pin(q["llvm"], elf=True, provider=True, limit=512*MIB)
        need(q["llvm"]["sha256"] == LLVM_SHA, "existing LLVM18 provider pin")
        self.baseline("venv", canonical(self.job/"venv", directory=True))
        self.baseline("python-base", canonical(q["python_base"], directory=True, provider=True))
        need(set(q["elfs"]) == ELFS, "complete held native ELF roster")
        self.elfs = {name: self.pin(row, elf=True) for name, row in q["elfs"].items()}
        for name, path in self.elfs.items():
            if name == "model_family_retirement":
                need(path.parent == self.root/"rust/target/debug/deps"
                     and re.fullmatch(r"model_family_retirement-[0-9a-f]{16}", path.name), "held selector ELF")
            else:
                need(path == self.root/"rust/target/debug"/name, f"same-source target ELF: {name}")
        self.preparation = canonical(q["preparation"], directory=True)
        need(self.preparation == Path(os.environ["MODEL_FAMILY_GATE_PREPARATION"])
             and self.preparation.is_relative_to(self.job), "same owned fresh preparation")
        for name in ("driver-fixture", "driver-catalog", "driver-first-frame-diagnostic", "peer"):
            self.baseline(name, canonical(self.preparation/name, directory=True))
        self.original = self.preparation/"driver-fixture"
        for name, expected in (("no-collector", MANIFEST_SHA), ("collector-current", COLLECTOR_SHA)):
            self.holds.add(self.original/name/"observations/reference-manifest.json", expected)
        diagnostic = self.read(self.preparation/"driver-first-frame-diagnostic/diagnostic.json")
        fresh_receipt = self.read(self.preparation/"driver-catalog/current-catalog-receipt.json")
        need(diagnostic["model_run_exit"] == 0 and diagnostic["retirement_acceptance"] is False
             and diagnostic["current_catalog_receipt"] == fresh_receipt
             and fresh_receipt["source_commit"] == os.environ["GITHUB_SHA"]
             and fresh_receipt.get("cached_code") is None, "same-source fresh failure postmortem required")
        held_model = [row for row in diagnostic["held_inputs"] if row["path"] == str(self.elfs["model-run"])]
        need(len(held_model) == 1 and held_model[0]["seal"]["sha256"]
             == self.holds.files[self.elfs["model-run"]]["seal"]["sha256"], "same held postmortem model-run required")
        for label in ("warmup", "frame-0"):
            base = self.preparation/"driver-first-frame-diagnostic"
            raw = {name: self.m.read(base/f"{label}-{suffix}", 2212)
                   for name, suffix in (("original", "original.raw"), ("daemon", "daemon.raw"), ("model", "model.bin"))}
            need(all(len(value) == 2212 for value in raw.values()), "whole553float/2212B raw required")
            need(digest(raw["original"]) == ORIGINAL_RAW[label] and raw["daemon"] == raw["model"], "independent original/same-model failure binding")
            need((raw["original"] == raw["model"]) == (label == "warmup"), "image-dependent failure required")
            if label == "frame-0":
                a, b = (struct.unpack("<553f", raw[name])[0] for name in ("original", "model"))
                need(abs(a-b) > 1e-6*max(1., abs(a), abs(b)), "nonzero strict faceOrientation failure required")
        self.request = self.pin(q["cached_request"], limit=4*MIB)
        need(self.request.parent == self.job/"driver476-original-request", "separate immutable cached request directory")
        cached = self.read(self.request)
        preflight_path = self.pin(q["retained_preflight"], limit=4*MIB)
        need(preflight_path == self.request.parent/"native-api-binding.json", "same independent retained preflight")
        preflight = self.read(preflight_path)
        need(preflight["source_commit"] == q["source_commit"] and preflight["whole44_triplets"] is True
             and preflight["actual_host_isa"] is True and preflight["generated_code_executed"] is False
             and len(preflight["warp_programs"]) == 1
             and preflight["bridge_source_sha256"] == cached["admission"]["current"]["isa_verifier_source_sha256"],
             "actual whole44 plus original warp native admission")
        need(cached["source_root"] == str(self.root) and len(cached["admission"]["programs"]) == 44
             and len(cached["programs"]) == 44 and cached.get("library") is None
             and cached.get("graph") is None, "whole44 independent preflight/current source")
        need({row["original_index"] for row in cached["programs"]} == set(range(44)), "exact44 unique program indices")
        for row in cached["programs"]:
            expected = cached["admission"]["programs"][row["original_index"]]
            for field, pin in (("source", "source_sha256"), ("image", "image_sha256"),
                               ("reconstruction_object", "reconstruction_object_sha256")):
                self.pin({"path": row[field], "sha256": expected[pin]}, limit=16*MIB)
        for field, expected in (("isa_receipt", cached["admission"]["current"]["isa_receipt_sha256"]),
                                ("original_manifest", cached["admission"]["original_manifest_sha256"]),
                                ("original_model", cached["admission"]["original_model_sha256"])):
            self.pin({"path": cached[field], "sha256": expected})
        self.cached_sha = q["cached_request"]["sha256"]
        self.baseline("cached-inputs", canonical(self.request.parent, directory=True))
        self.budget()

    def clone_fixture(self):
        self.candidate = self.out/"preparation"
        self.candidate.mkdir(mode=0o700)
        fixture = self.candidate/"driver-fixture"
        fixture.mkdir(mode=0o700)
        total = 0
        for source in self.m.walk(self.original):
            if source == self.original:
                continue
            target = fixture/source.relative_to(self.original)
            info = source.lstat()
            need(not stat.S_ISLNK(info.st_mode), "fixture aliases forbidden")
            if stat.S_ISDIR(info.st_mode):
                target.mkdir(mode=0o700)
            else:
                need(stat.S_ISREG(info.st_mode), "fixture regular file required")
                total += info.st_size
                need(total <= 512*MIB, "fixture copy512MiB bound")
                self.m.copy_new(source, target, 128*MIB)
            self.budget()
        (self.candidate/"peer").mkdir(mode=0o700)
        peer = self.preparation/"peer/driver-daemon-peer"
        self.holds.add(peer, elf=True)
        target = self.candidate/"peer/driver-daemon-peer"
        self.m.copy_new(peer, target, 128*MIB)
        target.chmod(0o500)

    def prepare(self):
        self.clone_fixture()
        for name in ("tmp", "cache"):
            (self.out/name).mkdir(mode=0o700)
        for stem, binary in (("MODEL_CACHED_CODE", "offline-cached-code"),
                             ("MODEL_LLVM_GUARDED_ADD", "offline-llvm-guarded-add"),
                             ("MODEL_CATALOG_PUBLISH", "publish-model-catalog")):
            self.env[stem+"_BINARY"] = str(self.elfs[binary])
            self.env[stem+"_SHA256"] = self.holds.files[self.elfs[binary]]["seal"]["sha256"]
        self.env.update(MODEL_CACHED_CODE_REQUEST=str(self.request),
                        MODEL_CACHED_CODE_REQUEST_SHA256=self.cached_sha, PYTHONDONTWRITEBYTECODE="1",
                        TMPDIR=str(self.out/"tmp"), XDG_CACHE_HOME=str(self.out/"cache"),
                        CACHEDB=str(self.out/"cache/compiler.db"))
        self.command([self.elfs["prepare-model-family-catalog"], "--root", self.root,
            "--models", self.candidate/"driver-fixture/no-collector/models",
            "--expected-model-manifest", self.candidate/"driver-fixture/no-collector/observations/reference-manifest.json",
            "--expected-model-manifest-sha256", MANIFEST_SHA, "--kind", "driver", "--python", self.python,
            "--llvm", self.llvm, "--output", self.candidate/"driver-catalog"], "cached-catalog", 900)
        receipt = self.read(self.candidate/"driver-catalog/current-catalog-receipt.json")
        need(receipt["cached_code"]["complete_preflight"] is True
             and receipt["cached_code"]["generated_code_executed"] is False, "inert whole-roster preflight required")
        fresh_target = self.read(self.preparation/"driver-catalog/compiler-target.json")
        candidate_target = self.read(self.candidate/"driver-catalog/compiler-target.json")
        for phase in ("before", "after"):
            need(candidate_target[phase] == fresh_target[phase], "natural compiler/provider target changed")
        self.prepared = True

    def require_runtime_prestate(self):
        original = self.proof/'original-prestate'
        model = self.candidate/'cached-first-native/model-run/prestate476'
        daemon = self.candidate/'cached-first-native/driver-output/1344-raw-1/prestate476'
        phases=('loaded','warmup-prepare','warmup-model','frame-0-prepare','frame-0-model')
        for root in (model,daemon):
            for phase in phases:
                need((root/phase/'receipt.json').is_file(),f'ACTUAL_NATIVE_PRESTATE_MISSING: {root}/{phase}')
        self.command([self.python, self.root/'tools/ci/driver_retained_program_476.py',
            '--original-worker','--request',self.args.request,'--request-sha256',self.args.request_sha256],
            'original-prestate',120,self.env)
        complete=self.read(original/'complete.json')
        current=self.read(self.candidate/'driver-catalog/catalog/current.json')
        generation=current['generation']
        need(re.fullmatch('[0-9a-f]{64}',generation),'actual candidate generation')
        bundle=self.candidate/'driver-catalog/catalog'/generation/'driver-1344x760'
        graph_raw=self.m.read(bundle/'graph.json')
        graph=self.m.unique_json(graph_raw)
        need(complete['actual_model_programs']==44 and complete['frames']==2
             and complete['source_commit']==os.environ['GITHUB_SHA']
             and complete['request_sha256']==self.args.request_sha256 and complete['graph_sha256']==digest(graph_raw),
             'actual original whole44/source/graph completion')
        verified=[]
        for phase in phases:
            source=self.read(original/phase/'receipt.json')
            need(source['schema']=='openpilot.driver476.original-prestate.v1' and source['actual_storage'] is True,
                 'actual original receipt required')
            original_bytes=[]
            for i,row in enumerate(source['allocations']):
                need(row['allocation']==i and row['file']==f'allocation-{i}.bin','original allocation order')
                raw=self.m.read(original/phase/row['file'],64*MIB)
                need(len(raw)==row['bytes'] and digest(raw)==row['sha256'],'original complete allocation bytes')
                original_bytes.append(raw)
            for label,root in (('model-run',model),('daemon',daemon)):
                receipt=self.read(root/phase/'receipt.json')
                need(receipt['schema']=='openpilot.driver476.actual-prestate.v1'
                     and receipt['actual_loaded_storage'] is True and receipt['kernels_executed_by_capture']==0,
                     'actual loaded native storage required')
                need(receipt['graph_sha256']==digest(graph_raw) and
                     receipt['certificate_sha256']==graph['cached_code']['certificate_sha256']
                     and receipt['weights_sha256']==graph['weights_sha256'],'actual graph/certificate/weights binding')
                for key,path in (('module_source_sha256','rust/crates/model-runtime/src/cpu/driver_prestate_476.rs'),
                                 ('cpu_source_sha256','rust/crates/model-runtime/src/cpu.rs')):
                    need(receipt[key]==digest(self.m.read(self.root/path,4*MIB)),'compiled prestate API source pin')
                need(len(receipt['programs'])==44 and len(receipt['allocations'])==len(original_bytes)
                     and receipt['inputs']==source['inputs'] and receipt['outputs']==source['outputs']
                     and receipt['original_model_sha256']==source['original_model_sha256']
                     and receipt['original_manifest_sha256']==source['original_manifest_sha256']
                     and source['source_commit']==os.environ['GITHUB_SHA'],
                     'whole44/actual allocation/input/output roster')
                native_bytes=[]
                for i,row in enumerate(receipt['allocations']):
                    need(row['allocation']==i and row['file']==f'allocation-{i}.bin','native allocation order')
                    raw=self.m.read(root/phase/row['file'],64*MIB)
                    need(len(raw)==row['bytes']==len(original_bytes[i]) and digest(raw)==row['sha256'],
                         'native complete allocation bytes')
                    native_bytes.append(raw)
                for left,right in zip(source['views'],receipt['views'],strict=True):
                    need(all(left[k]==right[k] for k in ('view','allocation','offset','bytes')),'independent actual alias/view graph')
                    raw=native_bytes[right['allocation']][right['offset']:right['offset']+right['bytes']]
                    need(digest(raw)==right['sha256'],'actual view bytes')
                programs={p['kernel']:(p['isa_proof_index'],p['original']) for p in receipt['programs']}
                need(len(programs)==44,'unique complete44 native programs')
                for left,right in zip(source['calls'],receipt['calls'],strict=True):
                    need(left['call']==right['call'] and left['kernel']==right['kernel'] and left['views']==right['views']
                         and left['core_id']==right['core_id'] and right['pointer_bytes']==8 and right['scalar_bytes']==4,
                         'actual source PROGRAM order/view/ABI')
                    need(right['actual_function']>0,'actual native function required')
                    expected_workers=[]
                    for worker in range(left['workers']):
                        scalars=list(left['scalars'])
                        if left['core_id'] is not None: scalars[left['core_id']]=worker
                        expected_workers.append({'worker':worker,'scalars':scalars})
                    need(right['workers']==expected_workers,'actual stored worker/scalar ABI')
                    for view,pointer in zip(right['views'],right['actual_pointers'],strict=True):
                        v=receipt['views'][view]
                        need(pointer==receipt['allocations'][v['allocation']]['actual_base']+v['offset'],
                             'native live pointer alias')
                    if left['original_index'] is not None:
                        index,p=programs[left['kernel']]
                        need(index==left['original_index'] and p['source_sha256']==left['source_sha256']
                             and p['image_sha256']==left['image_sha256'],'whole44 original source/image mapping')
                if phase!='loaded':
                    entry_name=phase.rsplit('-',1)[1]
                    entry=next(e for e in receipt['entrypoints'] if e['name']==entry_name)
                    expected_dispatch=[]
                    for call in receipt['calls'][entry['start']:entry['end']]:
                        for worker in call['workers']:
                            expected_dispatch.append({'call':call['call'],'worker':worker['worker'],
                                'actual_function':call['actual_function'],'actual_pointers':call['actual_pointers'],
                                'scalars':worker['scalars'],'pointer_bytes':8,'scalar_bytes':4})
                    observed=self.read(root/phase/'actual-dispatch.json')
                    need(observed==expected_dispatch,'actual native post-core_id dispatch differs from source ABI/order')
                compared=0
                for index,ranges in enumerate(source['read_before_write_ranges']):
                    for lo,hi in ranges:
                        need(0<=lo<hi<=len(native_bytes[index]),'source declared read-before-write range')
                        if native_bytes[index][lo:hi]!=original_bytes[index][lo:hi]:
                            first=next(i for i,(a,b) in enumerate(zip(original_bytes[index][lo:hi],native_bytes[index][lo:hi])) if a!=b)+lo
                            begin,end=max(0,first-8),min(len(native_bytes[index]),first+9)
                            self.m.save(self.proof/'actual-prestate-first-difference.json',{
                                'runtime':label,'phase':phase,'allocation':index,'byte':first,'read_span':[lo,hi],
                                'window':[begin,end],'original_hex':original_bytes[index][begin:end].hex(),
                                'native_hex':native_bytes[index][begin:end].hex(),
                                'graph_sha256':receipt['graph_sha256'],'certificate_sha256':receipt['certificate_sha256'],
                                'source_commit':os.environ['GITHUB_SHA'],'numeric_acceptance':False})
                            raise RuntimeError(f'ACTUAL_PRESTATE_DIFFERENCE {label}/{phase}/allocation{index}/{first}')
                        compared+=hi-lo
                verified.append({'runtime':label,'phase':phase,'compared_declared_read_before_write_bytes':compared,
                                 'actual_allocation_count':len(native_bytes),'whole44':True})
        self.m.save(self.proof/'actual-prestate-comparison.json',{'verified':verified,
            'actual_native_and_original_bytes':True,'whole44':True,'numeric_acceptance':False,
            'range_contract':'original declared ins/outs ranges; not instruction-level read/write coverage'})
        self.prestate_verified=True

    def run(self):
        self.admit()
        self.m.save(self.proof/"selector-plan.json", {"argv": [str(self.elfs["model_family_retirement"]),
            "--ignored", "--exact", SELECTOR, "--nocapture", "--test-threads=1"],
            "max_invocations": 1, "timeout_seconds": 1320, "full_six_source_acceptance": False})
        self.prepare()
        need(self.selector_runs == 0, "selector one invocation only")
        self.selector_runs += 1
        self.command([self.elfs["model_family_retirement"], "--ignored", "--exact", SELECTOR,
            "--nocapture", "--test-threads=1"], "cached-driver", 1320,
            self.env | {"MODEL_FAMILY_GATE_ROOT": str(self.root),
                        "MODEL_FAMILY_GATE_PREPARATION": str(self.candidate)}, allow_failure=True)
        selector_status=self.commands[-1]['returncode']
        self.require_runtime_prestate()
        need(selector_status==0,'cached selector failed; prestate retained; fresh failure still sticky')
        diagnostic = self.read(self.candidate/"cached-first-native/model-run/diagnostic.json")
        need(diagnostic["model_run_exit"] == 0 and len(diagnostic["comparisons"]) == 2, "candidate diagnostic complete")
        base = self.candidate/"cached-first-native/model-run"
        for label in ("warmup", "frame-0"):
            raw = [self.m.read(base/f"{label}-{suffix}", 2212)
                   for suffix in ("original.raw", "daemon.raw", "model.bin")]
            need(all(len(value) == 2212 for value in raw)
                 and digest(raw[0]) == ORIGINAL_RAW[label] and raw[0] == raw[1] == raw[2],
                 "candidate complete2212B original/daemon/model bytes differ")
        for row in diagnostic["comparisons"]:
            for name in ("model_vs_original", "daemon_vs_original", "model_vs_daemon"):
                pair = row[name]
                need(pair["left_bytes"] == pair["right_bytes"] == 2212 and pair["byte_equal"] is True,
                     "whole candidate raw2212 differs; existing packet1e-6 remains unchanged")
        self.candidate_numeric=True

    def finish(self, error):
        failures = []
        reaped = False
        failures.extend(f"missing baseline: {name}" for name in sorted(self.required_baselines-set(self.baselines)))
        try:
            self.children.stop()
            need(not self.children.scan(), "owned descendants remain")
            reaped = True
        except BaseException as e:
            failures.append(f"reap: {e}")
        for name, (root, paths, sdk, before) in self.baselines.items():
            try:
                after = self.m.snapshot(root, paths if paths is not None else self.m.walk(root), sdk=sdk)
                self.m.save(self.proof/f"{name}.after.json", after)
                need(before == after, f"{name} changed")
            except BaseException as e:
                failures.append(f"seal: {e}")
        try:
            self.holds.verify()
        except BaseException as e:
            failures.append(f"held: {e}")
        self.m.save(self.proof/"outcome.json", {"source": os.environ["GITHUB_SHA"], "phase": self.phase,
            "fresh_gate_exit": self.args.fresh_exit_status, "fresh_failure_sticky": True,
            "fresh_exit_observed": type(self.args.fresh_exit_status) is int and 0 <= self.args.fresh_exit_status <= 255,
            "fresh_failure_observed": type(self.args.fresh_exit_status) is int and 1 <= self.args.fresh_exit_status <= 255,
            "required_baselines": sorted(self.required_baselines), "registered_baselines": sorted(self.baselines),
            "candidate_prepared": self.prepared, "selector_invocations": self.selector_runs,
            "runtime_prestate_available": self.prestate_verified,
            "candidate_numeric_observed": self.candidate_numeric,
            "candidate_numeric_accepted": self.candidate_numeric and error is None and not failures,
            "full_six_source_acceptance": False, "retirement_credit": 0,
            "all_owned_children_reaped": reaped,
            "error": error, "finalizer_failures": failures, "final_seals_passed": not failures,
            "commands": self.commands, "owned_children": self.children.history,
            "held": self.holds.records()})
        self.holds.close()
        os.close(self.children.journal)
        need(not failures, "independent finalizer failed")


class TransportOnly:
    """SDK 없는 전송 인증. 기존 pidfd command 회수만 재사용한다."""
    command = Candidate.command

    def __init__(self, root, job, m):
        self.root, self.job, self.m = root, job, m
        self.out = job/'driver476-transport'
        self.out.mkdir(mode=0o700)
        self.proof, self.evidence = self.out/'commands', self.out/'proof'
        self.proof.mkdir(mode=0o700)
        self.evidence.mkdir(mode=0o700)
        self.holds, self.children = m.Holds(), m.Children(self.evidence/'children.jsonl')
        self.commands, self.phase, self.before, self.paths = [], 'admission', None, None
        self.env = os.environ.copy()
        self.deadline = min(time.monotonic()+22*60,
                            time.monotonic()+int(os.environ['DRIVER476_TRANSPORT_DEADLINE'])-time.time())

    def budget(self):
        need(time.monotonic() < self.deadline, 'transport shared22min deadline')
        total = 0
        for base in (self.out, self.job/'proof'):
            for path in self.m.walk(base):
                info = path.lstat()
                need(stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode), 'transport output type')
                total += max(info.st_size, info.st_blocks*512)
        need(total <= 128*MIB, 'combined transport/proof128MiB cap')
        fs = os.statvfs(self.job)
        need(fs.f_bavail*fs.f_frsize >= 128*MIB-total+16*MIB, 'transport remaining growth/reserve')

    def run(self):
        self.budget()
        index = self.job/'proof/index-before.bin'
        self.holds.add(index)
        names = self.m.read(index, 4*MIB).split(b'\0')
        need(names[-1] == b'' and 1 < len(names) <= 65000
             and len(set(names[:-1])) == len(names)-1, 'exact source index roster')
        self.paths = [self.root]
        for name in names[:-1]:
            path = Path(os.fsdecode(name))
            need(not path.is_absolute() and str(path) == os.fsdecode(name)
                 and '..' not in path.parts, 'canonical relative tracked source')
            self.paths.append(self.root/path)
        for path in (self.root/'tools/ci/driver_retained_program_476.py', self.root/COMMON,
                     self.job/'owner', self.root.parent/'.ci296-owner',
                     self.job/'shell-env', self.job/'transport-shell-env'):
            self.holds.add(path)
        need(Path(os.environ['BASH_ENV']) == self.job/'transport-shell-env', 'owned transport shell environment')
        python = canonical(Path(sys.executable).resolve(strict=True), provider=True)
        self.holds.add(python, elf=True)
        self.holds.add(Path(sys.executable), elf=True)
        tools = {name: canonical(Path('/usr/bin')/name, provider=True) for name in ('gh', 'bash', 'git')}
        for path in tools.values():
            self.holds.add(path, elf=True)
        self.before = self.m.snapshot(self.root, self.paths)
        self.m.save(self.evidence/'source.before.json', self.before)
        need(self.command([tools['git'], 'rev-parse', 'HEAD'], 'source-head', 10).decode().strip()
             == os.environ['GITHUB_SHA'], 'actual transport source HEAD')
        authenticate_retained_transport(self.m, self.command, tools, self.out/'frozen', self.holds)
        self.budget()

    def finish(self, error):
        failures, reaped, verified = [], False, False
        try:
            self.children.stop()
            need(not self.children.scan(), 'owned transport children remain')
            reaped = True
        except BaseException as e:
            failures.append('reap: '+str(e))
        try:
            need(self.before is not None and self.paths is not None, 'source baseline required')
            after = self.m.snapshot(self.root, self.paths)
            self.m.save(self.evidence/'source.after.json', after)
            need(after == self.before, 'transport source FD snapshot changed')
            self.holds.verify()
            verified = True
        except BaseException as e:
            failures.append('seal: '+str(e))
        try:
            # Finalizer has its separate90s allowance; growth cap remains identical.
            old = self.deadline
            self.deadline = time.monotonic()+90
            self.budget()
            self.deadline = old
        except BaseException as e:
            failures.append('budget: '+str(e))
        self.m.save(self.evidence/'outcome.json', {'source': os.environ['GITHUB_SHA'],
            'scope': 'transport-only-no-model-no-sdk', 'transport_only': True, 'phase': self.phase,
            'transport_authenticated': error is None and not failures,
            'source_fd_seals_passed': verified, 'all_owned_children_reaped': reaped,
            'finalizer_failures': failures, 'error': error, 'commands': self.commands,
            'held': self.holds.records(), 'models_executed': 0, 'original_executed': 0,
            'compiler_invocations': 0, 'cargo_invocations': 0, 'reuploaded_archives': 0,
            'full_six_source_acceptance': False, 'retirement_credit': 0})
        self.holds.close()
        os.close(self.children.journal)
        need(not failures, 'transport independent finalizer failed')


def transport_only_main():
    probe, error, finalized = None, None, False
    try:
        need(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, 'stdlib-only isolated Python required')
        expected = {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'bin9208/openpilot-rust',
            'GITHUB_REF': 'refs/heads/codex/feat-523-python-rust', 'GITHUB_JOB': 'transport',
            'RUNNER_ENVIRONMENT': 'github-hosted', 'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64'}
        need(all(os.environ.get(k) == v for k, v in expected.items()) and os.uname().machine == 'x86_64'
             and os.environ.get('GITHUB_EVENT_NAME') in ('push', 'workflow_dispatch'), 'trusted transport Actions context')
        need(os.environ['GITHUB_WORKFLOW_REF'] == 'bin9208/openpilot-rust/.github/workflows/python-rust-523-driver-transport.yml@'+os.environ['GITHUB_REF']
             and re.fullmatch('[0-9a-f]{40}', os.environ['GITHUB_SHA']) and not os.environ.get('LD_PRELOAD'), 'exact transport workflow/source')
        root, job = canonical(os.environ['CI296_SOURCE'], True), canonical(os.environ['CI_JOB_TEMP'], True)
        need(root == Path.cwd() == Path(os.environ['GITHUB_WORKSPACE'])
             and root == Path(os.environ['DIAG_SOURCE']) and root.name == 'source'
             and root.parent == Path(os.environ['CI296_SOURCE_OWNER'])
             and root.parent == Path(os.environ['DIAG_SOURCE_OWNER'])
             and root.parent.parent == Path(os.environ['DIAG_ACTIONS_WORKSPACE'])
             and re.fullmatch(r'\.ci296-source-'+re.escape(os.environ['GITHUB_RUN_ID'])+'-'
                              +re.escape(os.environ['GITHUB_RUN_ATTEMPT'])+r'\.[A-Za-z0-9]{6}', root.parent.name)
             and re.fullmatch(r'/tmp/openpilot-ci\.[A-Za-z0-9]{6}', str(job)), 'owned private transport paths')
        owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
        need(job.stat().st_mode & 0o077 == 0 and canonical(job/'owner').read_bytes() == owner.encode()
             and canonical(root.parent/'.ci296-owner').read_bytes() == (owner+'\n').encode(), 'exact source/job owner bytes')
        info = root.parent.stat()
        need(f'{info.st_dev}:{info.st_ino}' == os.environ['CI296_SOURCE_IDENTITY']
             == os.environ['DIAG_SOURCE_IDENTITY'], 'source owner inode')
        os.umask(0o077)
        for number in (signal.SIGINT, signal.SIGTERM, signal.SIGALRM):
            signal.signal(number, interrupted)
        probe = TransportOnly(root, job, common(root))
        signal.setitimer(signal.ITIMER_REAL, max(0.001, probe.deadline-time.monotonic()))
        probe.run()
    except BaseException:
        error = traceback.format_exc()
        print(error, file=sys.stderr)
    finally:
        if probe is not None:
            try:
                signal.alarm(90)
                probe.finish(error)
                finalized = True
            except BaseException:
                print(traceback.format_exc(), file=sys.stderr)
            finally:
                signal.alarm(0)
    return 0 if finalized and error is None else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--request", required=True)
    parser.add_argument("--request-sha256", required=True)
    parser.add_argument("--fresh-exit-status", required=True, type=int, choices=range(1, 256))
    parser.add_argument("--deadline-epoch", required=True, type=int)
    args = parser.parse_args()
    candidate, error = None, None
    try:
        need(os.environ.get("GITHUB_ACTIONS") == "true"
             and os.environ.get("GITHUB_REPOSITORY") == "bin9208/openpilot-rust"
             and os.environ.get("GITHUB_JOB") == "model-family-retirement"
             and os.environ.get("RUNNER_OS") == "Linux" and os.environ.get("RUNNER_ARCH") == "X64"
             and os.uname().machine == "x86_64", "actual Actions model-family Linux/X64 required")
        need(os.environ.get("GITHUB_EVENT_NAME") in {"push", "pull_request", "workflow_dispatch"}
             and os.environ.get("DRIVER476_SOURCE_ALLOWED") == "true", "parent must bind actual source trust")
        runner = os.environ.get("RUNNER_ENVIRONMENT")
        need(runner == "github-hosted" or (runner == "self-hosted"
             and os.environ.get("RUNNER_NAME") == "bin-actions"), "actual approved runner required")
        need(re.fullmatch("[0-9a-f]{40}", os.environ.get("GITHUB_SHA", "")), "source SHA40 required")
        need(not os.environ.get("LD_PRELOAD"), "LD_PRELOAD forbidden")
        root, job = canonical(os.environ["CI296_SOURCE"], True), canonical(os.environ["CI_JOB_TEMP"], True)
        need(root == Path.cwd() == Path(os.environ["GITHUB_WORKSPACE"]), "actual private source binding")
        need(re.fullmatch(r"/tmp/openpilot-ci\.[A-Za-z0-9]{6}", str(job)), "actual job-temp root")
        need(job.stat().st_mode & 0o077 == 0, "job0700 required")
        owner = ":".join(os.environ[name] for name in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB"))
        need(canonical(job/"owner").read_text().strip() == owner
             and canonical(root.parent/".ci296-owner").read_text().strip() == owner, "source/job owner token")
        info = root.parent.stat()
        need(root.parent == Path(os.environ["CI296_SOURCE_OWNER"])
             and f"{info.st_dev}:{info.st_ino}" == os.environ["CI296_SOURCE_IDENTITY"], "source owner inode binding")
        need(time.time() < args.deadline_epoch, "parent deadline expired")
        os.umask(0o077)
        for number in (signal.SIGINT, signal.SIGTERM, signal.SIGALRM):
            signal.signal(number, interrupted)
        m = common(root)
        candidate = Candidate(args, root, job, m)
        signal.setitimer(signal.ITIMER_REAL, max(0.001, candidate.deadline-time.monotonic()))
        candidate.run()
    except BaseException:
        error = traceback.format_exc()
        print(error, file=sys.stderr)
    finally:
        if candidate is not None:
            try:
                signal.alarm(90)  # 독립 finalizer 한도. 부모는 이 구간을 남겨야 한다.
                candidate.finish(error)
            except BaseException:
                print(traceback.format_exc(), file=sys.stderr)
            finally:
                signal.alarm(0)
    return args.fresh_exit_status  # candidate 준비/통과/실패 모두 fresh failure를 지우지 않는다.


if __name__ == "__main__":
    if len(sys.argv)>1 and sys.argv[1] == '--verify-retained-transport':
        need(len(sys.argv) == 2, 'transport-only entry takes no IDs or other arguments')
        sys.exit(transport_only_main())
    elif len(sys.argv)>1 and sys.argv[1] in ('--original-worker', '--collect-retained'):
        parser=argparse.ArgumentParser(allow_abbrev=False)
        parser.add_argument('--request',required=True)
        parser.add_argument('--request-sha256',required=True)
        worker_args=parser.parse_args(sys.argv[2:])
        worker = original_worker if sys.argv[1] == '--original-worker' else retained_collection_worker
        worker(worker_args.request,worker_args.request_sha256)
    else:
        sys.exit(main())
