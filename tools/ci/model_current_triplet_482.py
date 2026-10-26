#!/usr/bin/env python3
"""#482 Actions 전용 fresh triplet/2frame 관측. 로컬 기능 실행 금지.

기존 수치 gate, cached issuer와 production 경로를 변경하지 않는다.
controller만 stdlib이며 worker의 원본 import/compile/PKL 실행은 Actions에서만 한다.
"""
from __future__ import annotations

import ctypes
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import resource
import runpy
import signal
import stat
import struct
import subprocess
import sys
import threading
import time
import traceback

MIB, GIB = 1024**2, 1024**3
COMMON = "tools/ci/controlsd_normal_523.py"
COMMON_SHA = "8a5da6caaf97dc00a0672e271130e0ce0ead8c055bee9883f7e503b9904b85a7"
LLVM_SHA = "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
STORE_SOURCE = "e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0"
PREFIX = "cpu::diagnostic_482_tests::"
READONLY = PREFIX + "fresh_program_roster_triplets_and_host_isa_are_bound_without_execution"
TAPPED = PREFIX + "bound_road1928_two_frame_producer_capture"
CACHED_CONTEXT = PREFIX + "cached_context::complete_fresh91_cached_context_matches_existing_original"
WRITER_PREPARE = PREFIX + "writer_pair::fresh_writer_pair_is_bound_without_execution"
WRITER_EXECUTE = PREFIX + "writer_pair::fresh_writer_pair_reuses_admitted_original90"
CELLS = (100, 101, 1349)
# source-locked NumPy2.5.3 wheel의 DT_NEEDED가 없는 두 CPython test extension만 해당한다.
ZERO_NEEDED_NUMPY = {
    "numpy/_core/_operand_flag_tests.cpython-312-x86_64-linux-gnu.so":
        (16800, "d985fc6e648abe43a0d11d9e9c1c49c15a3020936c6105c159cc9ab2ff2e41f0"),
    "numpy/_core/_struct_ufunc_tests.cpython-312-x86_64-linux-gnu.so":
        (16936, "895b0c3376a227d8f2f9f35465876cd6b381daa177effa7a3070c7d31bba8688"),
}


def need(value, message):
    if not value:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def zero_needed_elf(raw):
    """ET_DYN의 선언된 needed edge만 확인한다. runtime symbol/provider 폐쇄 주장은 아니다."""
    need(64 <= len(raw) <= 64*1024 and raw[:7] == b"\x7fELF\x02\x01\x01", "bounded ELF64 little endian")
    need(struct.unpack_from("<HHI", raw, 16) == (3, 62, 1), "ET_DYN x86_64 version1 required")
    phoff = struct.unpack_from("<Q", raw, 32)[0]
    ehsize, phsize, count = struct.unpack_from("<HHH", raw, 52)
    need(ehsize == 64 and phsize == 56 and 0 < count <= 128
         and phoff >= 64 and phoff + phsize*count <= len(raw), "bounded ELF program headers")
    dynamic = []
    for index in range(count):
        header = phoff + phsize*index
        kind, _, offset, _, _, size, _, _ = struct.unpack_from("<IIQQQQQQ", raw, header)
        need(kind != 3, "Python extension must not have PT_INTERP")
        if kind == 2:
            need(size >= 16 and size % 16 == 0 and offset + size <= len(raw), "bounded PT_DYNAMIC")
            dynamic.append((header, offset, size))
    need(len(dynamic) == 1, "unique PT_DYNAMIC required")
    header, offset, size = dynamic[0]
    ended = False
    for cursor in range(offset, offset + size, 16):
        tag, value = struct.unpack_from("<qQ", raw, cursor)
        need(not ended or (tag, value) == (0, 0), "nonzero dynamic data after DT_NULL")
        need(tag != 1, "DT_NEEDED cannot be omitted by ldd")
        if tag == 0:
            need(value == 0, "DT_NULL value")
            ended = True
    need(ended, "DT_NULL terminator required")
    return {"phoff": phoff, "dynamic_header": header, "dynamic_offset": offset, "dynamic_bytes": size}


def write(path, raw):
    need(len(raw) <= 512*MIB, "file512MiB limit")
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)


def save(path, value):
    raw = (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()
    need(len(raw) <= 32*MIB, "metadata32MiB limit")
    write(path, raw)


def trusted():
    expected = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": "bin9208/openpilot-rust",
                "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/codex/feat-523-python-rust",
                "GITHUB_JOB": "model-pipelines", "RUNNER_ENVIRONMENT": "github-hosted",
                "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64"}
    need(all(os.environ.get(k) == v for k, v in expected.items()) and os.uname().machine == "x86_64",
         "exact hosted branch/job required")
    need(re.fullmatch("[0-9a-f]{40}", os.environ.get("GITHUB_SHA", "")), "source40 required")
    job, root = Path(os.environ["CI_JOB_TEMP"]), Path(os.environ["DIAG_SOURCE"])
    need(re.fullmatch(r"/tmp/openpilot-ci\.[A-Za-z0-9]{6}", str(job)), "private job path")
    for p in (job, root, root.parent, job / "proof"):
        m = p.lstat()
        need(p.resolve(strict=True) == p and stat.S_ISDIR(m.st_mode) and m.st_uid == os.getuid()
             and not m.st_mode & 0o022, "canonical owned private root")
    need(job.stat().st_mode & 0o077 == 0, "private job0700 required")
    marker = root.parent/'.ci296-owner'
    need(re.fullmatch(r'\.ci296-source-'+re.escape(os.environ['GITHUB_RUN_ID'])+'-'+re.escape(os.environ['GITHUB_RUN_ATTEMPT'])+r'\.[A-Za-z0-9]{6}',root.parent.name)
         and not marker.is_symlink() and marker.is_file()
         and marker.read_text().strip()==':'.join(os.environ[k] for k in ('GITHUB_RUN_ID','GITHUB_RUN_ATTEMPT','GITHUB_JOB')),
         'current private source owner marker')
    info=root.parent.stat()
    need(f'{info.st_dev}:{info.st_ino}'==os.environ['DIAG_SOURCE_IDENTITY'],'source owner inode binding')
    need((job / "owner").read_text().strip() == ":".join(os.environ[k] for k in
         ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB")), "owner binding")
    need(root == Path.cwd() == Path(os.environ["GITHUB_WORKSPACE"]), "source/root binding")
    need(root.parent == Path(os.environ["DIAG_SOURCE_OWNER"])
         and root.parent.parent == Path(os.environ["DIAG_ACTIONS_WORKSPACE"]), "source owner nesting")
    return root, job


def common(root):
    path = root / COMMON
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        before = os.fstat(fd)
        raw = os.pread(fd, 128*1024+1, 0)
        identity = lambda x: tuple(getattr(x, "st_"+k) for k in ("dev", "ino", "mode", "uid", "size", "mtime_ns", "ctime_ns"))
        need(stat.S_ISREG(before.st_mode) and len(raw) == before.st_size <= 128*1024
             and sha(raw) == COMMON_SHA and identity(before) == identity(os.fstat(fd)) == identity(path.lstat()), "common source pin/identity")
    finally:
        os.close(fd)
    spec = importlib.util.spec_from_file_location("_triplet482_owned", path)
    module = importlib.util.module_from_spec(spec)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def spec_file(module, path, limit=16*MIB):
    seal = module.regular(path, limit)
    return {"path": str(path), "sha256": seal["sha256"], "bytes": seal["identity"][5]}


def worker_environment(root):
    # isolated Python의 PYTHONPATH 대신 검증한3개 source root를 명시한다.
    sys.path[:0] = [str(root), str(root / "tinygrad_repo"), str(root / "rust/tools")]


def import_seal(module, root, job):
    rows = {}
    for name, loaded in sorted(sys.modules.items()):
        filename = getattr(loaded, "__file__", None)
        if not filename:
            continue
        path = Path(filename).resolve(strict=True)
        need(path.is_relative_to(root) or path.is_relative_to(job / "venv")
             or path.is_relative_to(Path(os.environ["ORIGINAL_PYTHON_BASE"])), "import escaped sealed roots")
        rows[name] = {"path": str(path), **module.regular(path)}
    return rows


def config(compiler, root, module, pass_options):
    from tinygrad.runtime.autogen import llvm
    values = {}
    for label, getter in (("triple", llvm.LLVMGetTargetMachineTriple),
                          ("cpu", llvm.LLVMGetTargetMachineCPU), ("features", llvm.LLVMGetTargetMachineFeatureString)):
        p = getter(compiler.target_machine)
        need(bool(p), "null target-machine getter")
        try:
            values[label] = ctypes.string_at(p).decode("ascii")
        finally:
            llvm.LLVMDisposeMessage(p)
    provider = Path(os.environ["LLVM_PATH"])
    need(module.regular(provider)["sha256"] == LLVM_SHA, "fixed LLVM provider pin")
    values.update(passes=compiler.passes.decode(), arch=compiler.arch, provider=str(provider),
                  provider_sha256=LLVM_SHA, dll_name=llvm.dll._name,
                  compiler_source=spec_file(module, root / "tinygrad_repo/tinygrad/runtime/support/compiler_cpu.py"),
                  env={k: os.environ.get(k) for k in ("DEV", "CPU_COUNT", "LLVMOPT", "JIT", "JIT_BATCH_SIZE", "OPENPILOT_HACKS")})
    values['pass_options'] = dict(pass_options.get(ctypes.cast(compiler.pbo,ctypes.c_void_p).value,{}))
    need(len(values['pass_options'])==4 and all(v==1 for v in values['pass_options'].values()), 'actual pass option setter roster')
    need(Path(llvm.dll._name).resolve(strict=True) == provider.resolve(strict=True), "autogen LLVM provider differs")
    need(values["triple"] == "x86_64-none-unknown-elf" and values["passes"] == "default<O2>", "natural compiler config")
    return values


def collect(root, job, m):
    build = job / "build/model482"
    objects = build / "triplets"
    objects.mkdir()
    need(not Path(os.environ["CACHEDB"]).exists(), "original cache must be absent before tinygrad import")
    worker_environment(root)
    from tinygrad import device
    from tinygrad.runtime.support import compiler_cpu, elf
    from tinygrad.runtime.ops_cpu import CPUProgram
    from tinygrad.runtime.autogen import llvm
    events, chains, loaded = [], [], {}
    journal = os.open(job/'proof/model482/compile-cache-link.jsonl', os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_CLOEXEC|os.O_NOFOLLOW, 0o600)
    event_size = 0
    local = threading.local()
    cache_get, cache_put = device.diskcache_get, device.diskcache_put
    cached, compile_obj, loader, elf_loader = (device.Compiler.compile_cached, compiler_cpu.LLVMCompiler.compile_to_obj,
                                              compiler_cpu.jit_loader, elf.elf_loader)
    program_init = CPUProgram.__init__
    option_names = ['LLVMPassBuilderOptionsSet'+name for name in ('LoopUnrolling','LoopVectorization','SLPVectorization','VerifyEach')]
    setters = {name:getattr(llvm,name) for name in option_names}
    pass_options = {}
    def setter(name, delegate):
        def observe(pointer,value):
            result = delegate(pointer,value)
            pass_options.setdefault(ctypes.cast(pointer,ctypes.c_void_p).value,{})[name] = int(value)
            return result
        return observe

    def record(event):
        nonlocal event_size
        need(len(events) < 100000, "compile/cache event bound")
        raw = (json.dumps(event,sort_keys=True,separators=(',',':'),allow_nan=False)+'\n').encode()
        event_size += len(raw)
        need(event_size<=16*MIB, 'event journal16MiB limit')
        need(os.write(journal, raw)==len(raw), 'short compile journal write')
        events.append(event)

    def get(table, key, *args, **kwargs):
        result = cache_get(table, key, *args, **kwargs)
        record({"kind": "get", "table": table, "key": key, "hit": result is not None,
                "returned_sha256": sha(result) if isinstance(result, bytes) else None})
        return result

    def put(table, key, value, *args, **kwargs):
        result = cache_put(table, key, value, *args, **kwargs)
        record({"kind": "put", "table": table, "key": key, "value_sha256": sha(value) if isinstance(value, bytes) else None})
        return result

    def cached_call(self, source):
        need(not getattr(local, "active", None), "nested original compiler")
        local.active = {"source": source, "cachekey": self.cachekey, "config": None, "object": None, "layout": None}
        try:
            result = cached(self, source)
            record({"kind": "compile_cached", "source_sha256": sha(source.encode()), "cachekey": self.cachekey,
                    "returned_sha256": sha(result)})
            return result
        finally:
            local.active = None

    def obj(self, source):
        active = local.active
        need(active and active["source"] == source and isinstance(self, compiler_cpu.CPULLVMCompiler), "actual LLVM compile chain")
        need(0 < len(source.encode()) <= 16*MIB, 'source16MiB bound before compiler')
        active["config"] = config(self, root, m, pass_options)
        pending = objects/f'compile-{len(chains)}.pending.ll'
        write(pending, source.encode())
        record({'kind':'compile_enter','source_sha256':sha(source.encode()),'pending_source':str(pending)})
        result = compile_obj(self, source)
        need(isinstance(result, bytes) and 0 < len(result) <= 16*MIB, "original object bound")
        active["object"] = result
        return result

    def layout(blob, *args, **kwargs):
        result = elf_loader(blob, *args, **kwargs)
        active = getattr(local, "active", None)
        if active is not None and blob == active["object"]:
            _, sections, _ = result
            text = [s for s in sections if s.header.sh_flags & 4 and s.header.sh_type == 1]
            need(len(text) == 1 and text[0].header.sh_addr == 0, "original executable section layout")
            alignment = max(int(s.header.sh_addralign) for s in sections if s.header.sh_type == 1)
            active["layout"] = {"text_bytes": len(text[0].content), "image_alignment": alignment}
        return result

    def link(blob, *args, **kwargs):
        result = loader(blob, *args, **kwargs)
        active = getattr(local, "active", None)
        need(active and active["object"] == blob and active["layout"], "ELF→original jit_loader chain")
        need(0 < len(result) <= 16*MIB, "original image bound")
        index = len(chains)
        paths = []
        for suffix, raw in (("ll", active["source"].encode()), ("o", blob), ("bin", result)):
            path = objects / f"compile-{index}.{suffix}"
            write(path, raw)
            paths.append(spec_file(m, path))
        chains.append({"event": index, "source": paths[0], "object": paths[1], "image": paths[2],
                       "layout": active["layout"], "config": active["config"]})
        record({"kind": "compile_link", "event": index, "source_sha256": paths[0]["sha256"],
                "object_sha256": paths[1]["sha256"], "image_sha256": paths[2]["sha256"]})
        return result

    def initialized(self, dev, name, image, *args, **kwargs):
        program_init(self, dev, name, image, *args, **kwargs)
        pointer = ctypes.cast(self.fxn, ctypes.c_void_p).value
        loaded.setdefault(sha(image), []).append(pointer)

    device.diskcache_get, device.diskcache_put, device.Compiler.compile_cached = get, put, cached_call
    compiler_cpu.LLVMCompiler.compile_to_obj, compiler_cpu.jit_loader, elf.elf_loader = obj, link, layout
    CPUProgram.__init__ = initialized
    for name,delegate in setters.items(): setattr(llvm,name,setter(name,delegate))
    model = build / "driving_tinygrad.pkl"
    oldargv = sys.argv
    try:
        sys.argv = [str(root / "openpilot/selfdrive/modeld/compile_modeld.py"), "--model-size", "512x256",
                    "--camera-resolutions", "1928x1208", "1344x760", "--frame-skip", "4", "--onnx",
                    str(root / "openpilot/selfdrive/modeld/models/driving_supercombo.onnx"), "--output", str(model)]
        runpy.run_path(sys.argv[0], run_name="__main__")
        from model_export.original import driving
        from model_export.pipeline import linear_calls
        from model_export.buffers import host_buffer
        from tinygrad.engine.realize import resolve_params
        from tinygrad.uop.ops import Ops
        original = driving(model, (1928, 1208))
        bases, allocations, view_ids, views = {}, [], {}, []
        def view(buffer):
            buffer = host_buffer(buffer)
            key = id(buffer.base)
            if key not in bases:
                bases[key] = len(allocations)
                allocations.append(buffer.base.nbytes)
            identity = (bases[key], buffer.offset, buffer.nbytes)
            if identity not in view_ids:
                view_ids[identity] = len(views)
                views.append(dict(zip(('allocation','offset','bytes'),identity)))
            return view_ids[identity]
        input_views = {name:view(t.uop.base.buffer) for name,t in original.bindings.inputs.items()}
        output_views = {name:view(t.uop.buffer) for name,t in original.bindings.outputs.items()}
        operations, programs, calls, kernels, identities = {}, [], [], {}, set()
        for call in linear_calls(original.pipeline.jit.captured.linear):
            op = call.src[0]
            buffers = [host_buffer(x.buffer) for x in resolve_params(call, ())]
            need(op.op in (Ops.PROGRAM, Ops.COPY, Ops.SLICE), "unknown original call")
            if op.op is Ops.SLICE:
                need(len(buffers)==2 and buffers[0].base is buffers[1].base, 'SLICE backing allocation')
                view(buffers[0])
                continue
            if op.op is Ops.COPY:
                need(len(buffers)==2 and buffers[0].nbytes==buffers[1].nbytes, 'COPY storage extent')
                key = ("copy", buffers[0].nbytes)
                kernels.setdefault(key, len(kernels))
                view(buffers[1]); view(buffers[0])
                calls.append({"index": len(calls), "program": None, "kernel": kernels[key], "workers": 1, "core_id": None,
                              "views":[view(b) for b in buffers],"scalars":[]})
                continue
            info = op.arg
            need(all(v.expr=='core_id' for v in info.vars) and all(type(v) is int for v in info.global_size)
                 and tuple(info.global_size[1:])==(1,1) and (info.local_size is None or tuple(info.local_size)==(1,1,1)),
                 'static original CPU launch/scalars required')
            source, image = op.src[3].arg.encode(), op.src[4].arg
            kernels.setdefault(source, len(kernels))
            if op not in operations:
                matches = [r for r in chains if r["source"]["sha256"] == sha(source) and r["image"]["sha256"] == sha(image)]
                unique = {(r["object"]["sha256"], r["layout"]["text_bytes"], r["layout"]["image_alignment"]) for r in matches}
                need(len(unique) == 1 and matches, "missing/ambiguous actual first compile object")
                chain = matches[0]
                alignment = chain["layout"]["image_alignment"]
                need(alignment > 0 and all(p % alignment == 0 for p in loaded.get(sha(image), []))
                     and loaded.get(sha(image)), "actual loaded image alignment not observed")
                identity = (sha(source), sha(image), tuple(info.globals), len(info.vars), info.global_size[0], info.runtimevars.get("core_id"))
                need(identity not in identities, "ambiguous PROGRAM identity")
                identities.add(identity)
                operations[op] = len(programs)
                programs.append({"index": len(programs), "kernel": kernels[source],
                    "source": chain["source"], "image": chain["image"], "object": chain["object"],
                    "compile_event": chain["event"],
                    "original": {"source_sha256": sha(source), "image_sha256": sha(image),
                        "reconstruction_object_sha256": chain["object"]["sha256"], "image_bytes": len(image),
                        **chain["layout"], "pointers": len(info.globals), "scalars": len(info.vars),
                        "global_slots": list(info.globals), "workers": info.global_size[0], "core_id": info.runtimevars.get("core_id")},
                    })
            for i in (*info.ins, *info.outs): view(buffers[i])
            calls.append({"index": len(calls), "program": operations[op], "kernel": kernels[source],
                          "workers": info.global_size[0], "core_id": info.runtimevars.get("core_id"),
                          "views":[view(buffers[i]) for i in info.globals], "scalars":[0 for _ in info.vars]})
        need(0 < len(programs) <= 4096 and len(calls) <= 1_000_000, "complete roster bounds")
        event_bytes = (json.dumps({"events": events, "chains": chains, "loaded_image_addresses": loaded},
                                 sort_keys=True, separators=(',', ':'), allow_nan=False)+'\n').encode()
        need(len(event_bytes)<=16*MIB, 'compile/cache event16MiB limit')
        write(build/'compile-events.json', event_bytes)
        sources = {}
        # 기존 exact8/10 roster는 Rust에서도 compiled bytes와 대조한다.
        for filename in json.loads(os.environ["MODEL482_SOURCE_PATHS"]):
            sources[filename] = spec_file(m, root / filename, 32*MIB)["sha256"]
        request = {"schema": 1, "source_root": str(root), "job": str(job), "source_sha256": os.environ["GITHUB_SHA"],
                   "model": spec_file(m, model, 128*MIB), "programs": programs, "calls": calls, "sources": sources,
                   "allocations":allocations,"views":views,"inputs":input_views,"outputs":output_views,
                   "events": spec_file(m, build / "compile-events.json", 16*MIB)}
        save(build / "triplet-request.json", request)
        imports = import_seal(m, root, job)
        save(job / "proof/model482/collect-imports.json", imports)
        m.save(job / "proof/model482/collect-summary.json", {"complete_selected_program_roster": True,
            "programs": len(programs), "calls": len(calls), "compile_events": len(chains),
            "original_compile_link_chain_complete": True, "historical_cache_hits_proven": False,
            "original_capture_executes_generated_code": True, "cached_issuer_invoked": False})
    finally:
        sys.argv = oldargv
        device.diskcache_get, device.diskcache_put, device.Compiler.compile_cached = cache_get, cache_put, cached
        compiler_cpu.LLVMCompiler.compile_to_obj, compiler_cpu.jit_loader, elf.elf_loader = compile_obj, loader, elf_loader
        CPUProgram.__init__ = program_init
        for name,delegate in setters.items(): setattr(llvm,name,delegate)
        os.close(journal)


def two_frames(root, job, m, tapped):
    worker_environment(root)
    import numpy as np
    from tinygrad import Device
    from model_export import export_cpu
    from model_export.original import driving
    from check_pipeline_reference import execute, write_tensor
    from openpilot.selfdrive.modeld.compile_modeld import get_policy_npy_shapes
    build = job / "build/model482"
    destination = build / "traffic"
    original = driving(build / "driving_tinygrad.pkl", (1928, 1208))
    imports_before = import_seal(m, root, job)
    callback = None
    if not tapped:
        destination.mkdir()
        (destination / "original").mkdir()
        export_cpu(original.pipeline.jit, original.bindings, destination / "bundle", entrypoints=original.pipeline.entrypoints)
    else:
        (destination / "original-tapped").mkdir()
        callback = OriginalTap(original, build, m)
    random = np.random.default_rng(20260930)
    hidden = original.metadata.output_slices.get("hidden_state", (0, 0))
    previous = np.zeros(hidden[1] - hidden[0], dtype=np.float32)
    sequence, descriptors = [], {}
    try:
        for frame in range(2):
            if callback:
                callback.frame = frame
            inputs = {}
            for name in ("frame", "big_frame"):
                values = random.integers(0, 256, original.bindings.inputs[name].shape, dtype=np.uint8)
                inputs[name] = f"{name}-{frame}.bin"
                if not tapped:
                    write(destination / inputs[name], values.tobytes())
                else:
                    need(m.read(destination / inputs[name]) == values.tobytes(), "same frame input drift")
                write_tensor(original.bindings.inputs[name], values)
            matrix = np.array([[0.5, 0.01, -40 + frame % 3], [-0.01, 0.8, -30], [0.0002, -0.0001, 1]], dtype=np.float32)
            for name in ("tfm", "big_tfm"):
                inputs[name] = f"{name}-{frame}.bin"
                if not tapped:
                    write(destination / inputs[name], matrix.tobytes())
                else:
                    need(m.read(destination / inputs[name]) == matrix.tobytes(), "same matrix input drift")
                write_tensor(original.bindings.inputs[name], matrix)
            shapes, sizes = get_policy_npy_shapes(original.metadata.input_shapes)
            packed = np.zeros(sum(sizes), dtype=np.float32)
            fields = {k: v.reshape(s) for (k, s), v in zip(shapes.items(), np.split(packed, np.cumsum(sizes[:-1])), strict=True)}
            fields["desire"][frame % fields["desire"].size] = 1
            fields["traffic_convention"][:] = [1, 0]
            fields["action_t"][:] = [0, (frame % 20) / 20]
            fields["prev_feat"][:] = previous
            inputs["packed_npy_inputs"] = f"packed_npy_inputs-{frame}.bin"
            if not tapped:
                write(destination / inputs["packed_npy_inputs"], packed.tobytes())
            else:
                need(m.read(destination / inputs["packed_npy_inputs"]) == packed.tobytes(), "same recurrent input drift")
            write_tensor(original.bindings.inputs["packed_npy_inputs"], packed)
            outputs = {}
            for stage in original.stages:
                execute(stage)
                Device.default.synchronize()
            model = original.bindings.outputs["model"].numpy().reshape(-1)
            need(np.all(np.isfinite(model)), "non-finite original model output")
            previous = model[hidden[0]:hidden[1]].copy()
            for name, tensor in original.bindings.outputs.items():
                values = tensor.numpy()
                filename = f"{name}-{frame}.bin"
                raw = values.tobytes()
                write(destination / ("original-tapped" if tapped else "original") / filename, raw)
                if tapped:
                    need(m.read(destination / "original" / filename) == raw, f"original plain/tapped drift: {filename}")
                outputs[name] = filename
                descriptors[filename] = {"frame": frame, "shape": list(values.shape), "dtype": values.dtype.str, "sha256": sha(raw)}
            sequence.extend([{"entrypoint": "prepare", "inputs": inputs, "outputs": {}},
                             {"entrypoint": "policy", "inputs": {}, "outputs": outputs}])
        if not tapped:
            save(destination / "sequence.json", sequence)
            save(destination / "outputs.json", descriptors)
        else:
            callback.finish(job / "proof/model482/original-worker-cells.json")
        imports_after = import_seal(m, root, job)
        need(all(imports_after.get(k) == v for k, v in imports_before.items()), "import bytes/identity changed")
        save(job / f"proof/model482/{'tapped' if tapped else 'plain'}-imports.json", import_seal(m, root, job))
    finally:
        if callback:
            callback.restore()


def cached_context_export(root, job, m):
    """Export the already captured fresh PKL; never execute an original stage."""
    worker_environment(root)
    from model_export import export_cpu
    from model_export.native import cached_export_session
    from model_export.original import driving
    build = job / "build/model482"
    before = import_seal(m, root, job)
    original = driving(build / "driving_tinygrad.pkl", (1928, 1208))
    with cached_export_session() as session:
        need(session is not None, "C requires explicit admitted native issuer")
        cached = session.for_model(original.pipeline.jit)
        need(len(cached.operations) == 91, "C requires every fresh PROGRAM")
        export_cpu(original.pipeline.jit, original.bindings, build / "cached-context/bundle",
                   entrypoints=original.pipeline.entrypoints, cached=cached)
    after = import_seal(m, root, job)
    need(all(after.get(k) == v for k, v in before.items()), "C import bytes/identity changed")
    save(job / "proof/model482/cached-export-imports.json", after)


def writer_pair_compile(root, job, m):
    """D 전용 두 compiler 입력. PKL/model 실행과 compile_cached는 호출하지 않는다."""
    destination = job/'build/model482/cached-context/writer-pair'
    path = destination/'compile-request.json'
    need(m.regular(path)['sha256'] == os.environ['MODEL482_WRITER_COMPILE_REQUEST_SHA256'], 'D compile request seal')
    request = m.unique_json(m.read(path, 4*MIB))
    need(request['schema'] == 1 and request['source'] == os.environ['GITHUB_SHA'], 'D source request')
    inputs = {}
    for name, seal in request['files'].items():
        source = Path(seal['path'])
        need(source.is_relative_to(job/'build/model482') and spec_file(m, source, 16*MIB) == seal, 'D held compiler input')
        inputs[name] = m.read(source, 16*MIB)
    need(set(inputs) == {'original', 'guarded', 'rewrite', 'config'} and sha(inputs['original']) == STORE_SOURCE,
         'D exact writer inputs')
    worker_environment(root)
    from tinygrad.runtime.autogen import llvm
    from tinygrad.runtime.support.compiler_cpu import CPULLVMCompiler
    from tinygrad.device import Compiler
    imports_before = import_seal(m, root, job)
    names = ['LLVMPassBuilderOptionsSet'+name for name in ('LoopUnrolling','LoopVectorization','SLPVectorization','VerifyEach')]
    setters, pass_options, machines = {name: getattr(llvm, name) for name in names}, {}, {}
    create = llvm.LLVMCreateTargetMachine
    cached, cache_calls = Compiler.compile_cached, []
    def reject_cached(*_args, **_kwargs):
        cache_calls.append(True)
        raise RuntimeError('D fresh pair forbids compile_cached')
    pointer = lambda value: ctypes.cast(value, ctypes.c_void_p).value
    def observe_option(name, delegate):
        def observed(pbo, value):
            result = delegate(pbo, value)
            pass_options.setdefault(pointer(pbo), {})[name] = int(value)
            return result
        return observed
    def observe_machine(*args):
        result = create(*args)
        machines[pointer(result)] = dict(zip(('codegen_level','relocation_mode','code_model'), map(int, args[-3:])))
        return result
    for name, delegate in setters.items(): setattr(llvm, name, observe_option(name, delegate))
    llvm.LLVMCreateTargetMachine = observe_machine
    Compiler.compile_cached = reject_cached
    try:
        from model_export.native import _guarded_add_helper, _guarded_add
        compiler = CPULLVMCompiler(['x86_64', 'native'])
        expected = m.unique_json(inputs['config'])
        actual = config(compiler, root, m, pass_options)
        need(actual == expected, 'D actual natural compiler context differs from A')
        options = machines[pointer(compiler.target_machine)]
        need(list(options.values()) == [llvm.LLVMCodeGenLevelDefault, llvm.LLVMRelocPIC, llvm.LLVMCodeModelDefault],
             'D observed target-machine constructor options')
        text = inputs['original'].decode('utf-8')
        match = re.match(r'define void @([A-Za-z0-9_]+)\(', text)
        need(match and text.count('@'+match[1]+'(') == 1, 'D original writer declaration')
        renamed = text.replace('@'+match[1]+'(', '@op_impl_92(')
        with _guarded_add_helper() as helper:
            need(helper is not None, 'D requires the existing held guarded helper')
            guarded, rewrite = _guarded_add(renamed, helper[0])
        need(guarded.encode() == inputs['guarded'] and rewrite == inputs['rewrite'], 'D current B guard bytes differ')
        variants = {}
        context = {'llvm_context': pointer(compiler.context), 'target_machine': pointer(compiler.target_machine),
                   'pass_builder_options': pointer(compiler.pbo), 'constructor_options': options}
        for name, source in (('rename-only', renamed), ('guarded', guarded)):
            before = config(compiler, root, m, pass_options)
            need(before == actual, 'D compiler context before fresh compile')
            obj = compiler.compile_to_obj(source)
            need(isinstance(obj, bytes) and 0 < len(obj) <= 16*MIB, 'D writer object16MiB')
            after = config(compiler, root, m, pass_options)
            need(after == before, 'D compiler context after fresh compile')
            write(destination/(name+'.ll'), source.encode())
            write(destination/(name+'.o'), obj)
            variants[name] = {'source': spec_file(m, destination/(name+'.ll')), 'object': spec_file(m, destination/(name+'.o')),
                              'before': before, 'after': after, 'context': context}
        save(destination/'compiler-receipt.json', {'schema': 1, 'source': os.environ['GITHUB_SHA'],
            'original_source_sha256': STORE_SOURCE, 'variants': variants, 'fresh_compiles': 2,
            'compile_cached_calls': len(cache_calls), 'original_executions': 0,
            'original_model_executions': 0, 'original_compiler_invocations': 2, 'taps': 0})
        for seal in request['files'].values():
            need(spec_file(m, Path(seal['path']), 16*MIB) == seal, 'D compiler input changed')
        imports_after = import_seal(m, root, job)
        need(all(imports_after.get(k) == v for k, v in imports_before.items()), 'D imported source/provider changed')
        save(job/'proof/model482/writer-pair-imports.json', imports_after)
    finally:
        Compiler.compile_cached = cached
        llvm.LLVMCreateTargetMachine = create
        for name, delegate in setters.items(): setattr(llvm, name, delegate)


class OriginalTap:
    """원본 worker에서 동기화/Copyout 없이 동일 실함수를 정확1회 호출한다."""
    def __init__(self, original, build, m):
        from tinygrad.runtime.ops_cpu import CPUComputeQueue
        self.queue, self.previous = CPUComputeQueue, CPUComputeQueue._exec
        request = m.unique_json(m.read(build / "triplet-request.json", 4*MIB))
        rows = [r for r in request["programs"] if r["original"]["source_sha256"] == STORE_SOURCE]
        need(len(rows) == 1, "current known source/store context differs; no forced context")
        self.selected = rows[0]
        selected_calls = [r for r in request['calls'] if r['program']==self.selected['index']]
        need(len(selected_calls)==1 and request['views'][selected_calls[0]['views'][0]] == request['views'][request['outputs']['model']],
             'selected call and actual model view must be identical')
        self.call_index = selected_calls[0]['index']
        buffer = original.bindings.outputs["model"].uop.buffer.ensure_allocated()
        self.address, self.size = buffer._buf.va_addr, buffer.nbytes
        need(buffer.offset == 0 and self.size == 2576*4, "actual model view/store context differs")
        source = m.read(Path(rows[0]["source"]["path"]))
        need(sha(source) == STORE_SOURCE, "known exact store source")
        self.frame, self.events, self.lock = 0, [], threading.Lock()
        selected_image = rows[0]["original"]["image_sha256"]
        known = {r["original"]["image_sha256"] for r in request["programs"]}
        image_cache = {}
        previous = self.previous

        def observed(queue, tid, prg, count, *args):
            if id(prg) not in image_cache:
                image_cache[id(prg)] = (prg, sha(prg.mem[:]))
            image = image_cache[id(prg)][1]
            need(image in known, "unbound original PROGRAM image")
            selected = image == selected_image
            if selected:
                need(count == 10 and len(args) == 11 and args[0] == self.address and tid in (0, 1), "actual worker/argument/view drift")
                from tinygrad.helpers import mv_address
                pointer = ctypes.cast(prg.fxn, ctypes.c_void_p).value
                need(pointer == mv_address(prg.mem) and sha(prg.mem[:]) == selected_image, 'actual loaded image/function binding')
                cells = [i for i in CELLS if i // 1288 == tid]
                before = [ctypes.string_at(self.address + i*4, 4).hex() for i in cells]
            previous(queue, tid, prg, count, *args)
            if selected:
                after = [ctypes.string_at(self.address + i*4, 4).hex() for i in cells]
                with self.lock:
                    need(len(self.events) < 4, "duplicate worker callback")
                    self.events.append({"frame": self.frame, "program": self.selected["index"], "call":self.call_index,"worker": tid,
                        "cells": cells, "before_le_hex": before, "after_le_hex": after,
                        "function_address":pointer,
                        "pointer_args": list(args[:count]), "scalar_args_before_core_override": list(args[count:]),
                        "image_sha256": image})
        CPUComputeQueue._exec = observed

    def restore(self):
        self.queue._exec = self.previous

    def finish(self, path):
        need(sorted((r["frame"], r["worker"]) for r in self.events) == [(0, 0), (0, 1), (1, 0), (1, 1)], "missing/duplicate worker coverage")
        save(path, {"events": self.events, "plain_tapped_all_outputs_equal": True,
                    "ssa_first_producer_observed": False, "historical_half_snan5_resolved": False})


class Controller:
    def __init__(self, root, job, m):
        self.root, self.job, self.m = root, job, m
        self.out = job / "proof/model482"
        self.out.mkdir()
        self.build = job / "build/model482"
        self.build.mkdir(parents=True)
        self.build_root = job / "build"
        self.target = self.build_root / "target"
        self.build_root_before = self.build_identity(self.build_root)
        self.target_before = None
        self.build_scan_count = 0
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        need(hard == resource.RLIM_INFINITY or hard >= 4096, 'held file descriptor capacity')
        resource.setrlimit(resource.RLIMIT_NOFILE, (max(soft,4096), hard))
        self.holds, self.children = m.Holds(), m.Children(self.out / "children.jsonl")
        self.commands, self.phase = [], "admission"
        self.provider_maps = {}
        self.last_maps = 0
        remaining = int(os.environ['MODEL482_JOB_START']) + 61*60 - time.time()
        need(remaining > 0, 'job finalizer reserve unavailable')
        self.deadline = time.monotonic() + min(40*60, remaining)
        self.baselines, self.last_budget = {}, 0
        self.env = {k: os.environ[k] for k in ("GITHUB_ACTIONS", "GITHUB_REPOSITORY", "GITHUB_EVENT_NAME", "GITHUB_REF",
            "GITHUB_JOB", "GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "RUNNER_ENVIRONMENT", "RUNNER_OS", "RUNNER_ARCH",
            "CI_JOB_TEMP", "DIAG_SOURCE", "DIAG_SOURCE_OWNER", "DIAG_ACTIONS_WORKSPACE", "GITHUB_WORKSPACE",
            "DIAG_SOURCE_IDENTITY", "ORIGINAL_PYTHON_BASE", "ORIGINAL_PYTHON", "LLVM_PATH")}
        sdk = Path(os.environ["CI_PREPARED_RUST_ROOT"])
        need(sdk == job / "rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu", "private prepared SDK1.94 required")
        self.sdk = sdk
        self.env.update(HOME=str(job / "home"), PATH=f"{sdk}/bin:/usr/bin:/bin", CARGO_HOME=str(job / "cargo-home"),
            RUSTUP_HOME=str(job / "rustup-home"), CARGO_TARGET_DIR=str(job / "build/target"), RUSTC=str(sdk / "bin/rustc"),
            RUSTDOC=str(sdk / "bin/rustdoc"), CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="2", CARGO_PROFILE_DEV_DEBUG="0",
            CARGO_PROFILE_TEST_DEBUG="0", CARGO_TERM_COLOR="never", PYTHONDONTWRITEBYTECODE="1", LC_ALL="C.UTF-8",
            LD_LIBRARY_PATH=str(Path(os.environ["ORIGINAL_PYTHON_BASE"]) / "lib"), DEV="CPU:LLVM", JIT="1",
            JIT_BATCH_SIZE="0", CPU_COUNT="2", OPENPILOT_HACKS="1", LLVMOPT="1", TMPDIR=str(job / "cache"),
            XDG_CACHE_HOME=str(job / "cache"), CACHEDB=str(self.build / "original-cache.db"),
            CC=str(Path("/usr/bin/clang").resolve(strict=True)))

    def build_identity(self, path):
        try:
            info = path.lstat()
        except FileNotFoundError as problem:
            raise RuntimeError("build scan root unavailable") from problem
        need(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid()
             and not info.st_mode & 0o022 and path.resolve(strict=True) == path,
             "build scan root boundary")
        # Cargo가 바꾸는 directory 크기·시간·nlink는 고정하지 않는다.
        return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid)

    def build_roots(self):
        need(self.build_identity(self.build_root) == self.build_root_before, "build scan root changed")
        if self.target_before is None:
            try:
                self.target.lstat()
            except FileNotFoundError as problem:
                need(problem.errno == errno.ENOENT, "fresh target absence errno")
            else:
                raise RuntimeError("target appeared before first Cargo admission")
        else:
            need(self.build_identity(self.target) == self.target_before, "mutable target root changed")

    def build_missing(self, path, problem):
        need(isinstance(problem, FileNotFoundError) and problem.errno == errno.ENOENT,
             "build scan accepts only ENOENT")
        need(self.target_before is not None and path.is_absolute() and ".." not in path.parts
             and path != self.target and path.is_relative_to(self.target),
             "build ENOENT outside exact target child")
        self.build_roots()
        try:
            path.lstat()
        except FileNotFoundError as current:
            need(current.errno == errno.ENOENT, "build scan accepts only ENOENT")
        else:
            raise RuntimeError("build ENOENT entry is currently present")
        self.build_roots()
        need(len(os.fsencode(path)) <= 1024, "build ENOENT path1024B bound")
        return str(path)

    def build_scan_time(self, deadline):
        need(time.monotonic() <= min(deadline, self.deadline), "build metadata scan10s/global deadline")

    @staticmethod
    def build_charge(totals, allocated, apparent, prefix, cap):
        need(allocated >= 0 and apparent >= 0, "negative build metadata size")
        totals[0] += allocated
        totals[1] += apparent
        need(max(totals[0] + prefix[0], totals[1] + prefix[1]) <= cap,
             "build/registry allocated or apparent cap")

    @staticmethod
    def build_complete(attempt, missing):
        need(not missing or attempt < 2, "build metadata scan exhausted3attempts")
        return not missing

    def build_scan_record(self, attempt, totals, prefix, missing, status, started, error=None):
        text = None if error is None else str(error)
        value = {"schema": 1, "source": os.environ["GITHUB_SHA"], "phase": self.phase,
            "scan": self.build_scan_count, "attempt": attempt+1, "root": str(self.build_root),
            "root_identity": self.build_root_before, "target": str(self.target),
            "target_identity": self.target_before, "partial_allocated_bytes": totals[0],
            "partial_apparent_bytes": totals[1], "registry_prefix_bytes": list(prefix),
            "cap_bytes": 8*GIB, "missing_paths": missing, "status": status,
            "elapsed_seconds": time.monotonic()-started, "deadline_seconds": 10,
            "error": text[:2048] if text is not None else None,
            "error_truncated": text is not None and len(text) > 2048, "Cargo_restarted": False}
        raw = (json.dumps(value, sort_keys=True, allow_nan=False)+"\n").encode()
        need(len(raw) <= 32*1024, "build scan diagnostic32KiB bound")
        self.m.write_new(self.out/f"budget-build-{self.build_scan_count:04d}-{attempt+1}-{status}.json", raw)

    def measure_build(self, prefix=(0, 0), cap=8*GIB):
        started = time.monotonic()
        deadline = min(started+10, self.deadline)
        self.build_scan_count += 1
        need(self.build_scan_count <= 2048, "build scan count2048 bound")
        for attempt in range(3):
            totals, missing, seen = [0, 0], [], set()

            def absent(path, problem):
                self.build_scan_time(deadline)
                text = self.build_missing(path, problem)
                if text not in missing:
                    need(len(missing) < 16, "build ENOENT path count16 bound")
                    missing.append(text)

            def charge(info):
                if info.st_dev != self.build_root_before[0]:
                    return False  # du -x와 같이 다른 filesystem은 순회/합산하지 않는다.
                key = (info.st_dev, info.st_ino)
                if key in seen:
                    return False
                seen.add(key)
                need(stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode),
                     "special build metadata entry")
                apparent = max(0, info.st_size) if stat.S_ISREG(info.st_mode) or stat.S_ISLNK(info.st_mode) else 0
                self.build_charge(totals, info.st_blocks*512, apparent, prefix, cap)
                return True

            def visit(fd, path, depth):
                need(depth <= 64, "build directory depth64 bound")
                self.build_scan_time(deadline)
                try:
                    with os.scandir(fd) as entries:
                        for entry in entries:
                            self.build_scan_time(deadline)
                            child = path / entry.name
                            try:
                                info = os.stat(entry.name, dir_fd=fd, follow_symlinks=False)
                            except FileNotFoundError as problem:
                                absent(child, problem)
                                continue
                            if not charge(info) or not stat.S_ISDIR(info.st_mode):
                                continue
                            try:
                                child_fd = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC,
                                                   dir_fd=fd)
                            except FileNotFoundError as problem:
                                absent(child, problem)
                                continue
                            try:
                                identity = lambda value: (value.st_dev, value.st_ino, value.st_mode, value.st_uid, value.st_gid)
                                need(identity(os.fstat(child_fd)) == identity(info), "build directory replaced before open")
                                visit(child_fd, child, depth+1)
                                self.build_scan_time(deadline)
                                try:
                                    current = os.stat(entry.name, dir_fd=fd, follow_symlinks=False)
                                except FileNotFoundError as problem:
                                    absent(child, problem)
                                else:
                                    need(identity(current) == identity(info) == identity(os.fstat(child_fd)),
                                         "build directory replaced during scan")
                            finally:
                                os.close(child_fd)
                except FileNotFoundError as problem:
                    absent(path, problem)

            try:
                self.build_scan_time(deadline)
                self.build_roots()
                fd = os.open(self.build_root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
                try:
                    info = os.fstat(fd)
                    need((info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid) == self.build_root_before,
                         "build scan root changed at open")
                    charge(info)  # root directory의 allocated blocks도 포함한다.
                    visit(fd, self.build_root, 0)
                finally:
                    os.close(fd)
                self.build_roots()
                self.build_scan_time(deadline)
                clean = self.build_complete(attempt, missing)
                self.build_scan_record(attempt, totals, prefix, missing, "clean" if clean else "retry", started)
                self.build_scan_time(deadline)
                if clean:
                    return tuple(totals)
            except BaseException as problem:
                try:
                    self.build_scan_record(attempt, totals, prefix, missing, "failed", started, problem)
                except BaseException as diagnostic:
                    problem.add_note(f"build scan diagnostic unavailable: {diagnostic}")
                raise
        raise RuntimeError("build metadata scan did not complete")

    def prepare_build_target(self):
        self.build_scan_time(self.deadline)
        self.build_roots()
        need(self.target_before is None, "fresh target already admitted")
        self.target.mkdir(mode=0o700)
        self.target_before = self.build_identity(self.target)
        need(stat.S_IMODE(self.target_before[2]) == 0o700, "fresh target0700")
        self.build_metadata_controls()
        self.build_scan_time(self.deadline)

    def metadata_control_du_record(self, argv, apparent, started, seconds, result, error=None):
        # 정지 fixture 표본이며 budget의 전역 첫 실패 원문과 구분한다.
        status = getattr(error, "returncode", None) if result is None else result.returncode
        timed_out = isinstance(error, subprocess.TimeoutExpired)
        elapsed = time.monotonic()-started
        stem = "metadata-control-du-" + ("apparent" if apparent else "allocated")
        streams = {}
        for name, attribute in (("stdout", "output"), ("stderr", "stderr")):
            raw = getattr(error, attribute, None)
            if raw is None:
                raw = getattr(result, name, b"") if result is not None else b""
            need(isinstance(raw, bytes), "metadata du diagnostic requires original bytes")
            retained = raw[:64*1024]
            filename = f"{stem}.{name}"
            write(self.out/filename, retained)
            streams[name] = {"file": filename, "captured_bytes": len(raw), "retained_bytes": len(retained),
                "sha256": sha(retained), "truncated": len(raw) != len(retained),
                "capture_complete": status is not None and not timed_out}
        message = None if error is None else str(error)
        value = {"schema": 1, "source": os.environ["GITHUB_SHA"], "phase": self.phase,
            "input_origin": "authored-metadata-controls", "root": str(self.build_root),
            "kind": "apparent" if apparent else "allocated", "argv": argv,
            "returncode": status, "timed_out": timed_out, "timeout_seconds": seconds,
            "elapsed_seconds": elapsed, "exception": None if error is None else type(error).__name__,
            "exception_message": None if message is None else message[:4096],
            "exception_message_truncated": message is not None and len(message) > 4096,
            "raw_prefix_limit_bytes": 64*1024, "streams": streams, "retried": False,
            "outcome_promoted": False}
        raw = (json.dumps(value, sort_keys=True, indent=2, allow_nan=False)+"\n").encode()
        need(len(raw) <= 16*1024, "metadata du diagnostic index16KiB limit")
        # 두 raw 뒤 index를 게시하고, 그 뒤에만 기존 output/metric 조건을 판정한다.
        write(self.out/f"{stem}.json", raw)

    def build_metadata_controls(self):
        # 첫 Cargo 전 실제 private metadata만 사용한다. 모델/원본 실행이나 기대값 생성이 아니다.
        fixture = self.target / "model482-budget-controls"
        fixture.mkdir(mode=0o700)
        fixture_before = self.build_identity(fixture)
        paths = []
        owned = {}
        directory_before = None
        negatives = []
        def expect(name, operation, message):
            try:
                operation()
            except RuntimeError as problem:
                need(str(problem) == message, "unexpected build metadata control: "+name)
            else:
                raise RuntimeError("build metadata negative accepted: "+name)
            negatives.append(name)
        try:
            for name, sparse in (("regular", False), ("sparse", True)):
                path = fixture/name
                with path.open("xb") as stream:
                    if sparse:
                        stream.truncate(MIB)
                    else:
                        stream.write(b"metadata-control\n"*256)
                paths.append(path)
                owned[path] = self.m.ident(path.lstat())[:5]
            os.link(fixture/"regular", fixture/"hardlink", follow_symlinks=False)
            paths.append(fixture/"hardlink")
            owned[paths[-1]] = self.m.ident(paths[-1].lstat())[:5]
            for name, destination in (("link", "sparse"), ("outside-link", "/proc")):
                os.symlink(destination, fixture/name)
                paths.append(fixture/name)
                owned[paths[-1]] = self.m.ident(paths[-1].lstat())[:5]
            directory = fixture/"directory"
            directory.mkdir(mode=0o700)
            directory_before = self.build_identity(directory)
            measured = self.measure_build()
            du = Path("/usr/bin/du")
            self.holds.add(du, elf=True)
            expected = []
            for option in ([], ["--apparent-size"]):
                argv = [str(du), "-sx", "--block-size=1", *option, "--", str(self.build_root)]
                started, result = time.monotonic(), None
                seconds = min(10, max(0.001, self.deadline-started))
                try:
                    result = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
                                            timeout=seconds, env=self.env)
                except BaseException as error:
                    try:
                        self.metadata_control_du_record(argv, bool(option), started, seconds, result, error)
                    except BaseException as evidence_error:
                        error.add_note(f"metadata control du evidence incomplete: {evidence_error}")
                    raise
                self.metadata_control_du_record(argv, bool(option), started, seconds, result)
                match = re.fullmatch(rb"([0-9]+)\t"+re.escape(os.fsencode(self.build_root))+b"\n", result.stdout)
                need(match and not result.stderr, "metadata control GNU du output")
                expected.append(int(match[1]))
            need(measured == tuple(expected), "build allocated/apparent differs from actual GNU du")
            missing = fixture/"missing"
            absent_error = FileNotFoundError(errno.ENOENT, "authored metadata control", str(missing))
            need(self.build_missing(missing, absent_error) == str(missing), "build ENOENT control")
            expect("target-root", lambda: self.build_missing(self.target, absent_error), "build ENOENT outside exact target child")
            expect("outside-target", lambda: self.build_missing(self.out/"missing", absent_error), "build ENOENT outside exact target child")
            expect("permission", lambda: self.build_missing(missing, PermissionError(errno.EACCES, "authored")), "build scan accepts only ENOENT")
            expect("currently-present", lambda: self.build_missing(fixture/"regular", absent_error), "build ENOENT entry is currently present")
            expect("non-ENOENT", lambda: self.build_missing(missing, FileNotFoundError(errno.EIO, "authored")), "build scan accepts only ENOENT")
            expect("third-incomplete", lambda: self.build_complete(2, [str(missing)]), "build metadata scan exhausted3attempts")
            expect("deadline", lambda: self.build_scan_time(time.monotonic()-1), "build metadata scan10s/global deadline")
            for name, allocated, apparent, prefix, cap in (
                ("allocated-cap", 2, 0, (0, 0), 1), ("apparent-cap", 0, 2, (0, 0), 1),
                ("group-cap", 1, 0, (8*GIB, 0), 8*GIB)):
                expect(name, lambda a=allocated, b=apparent, p=prefix, c=cap: self.build_charge([0, 0], a, b, p, c),
                       "build/registry allocated or apparent cap")
            original = self.build_root/"target-metadata-control-original"
            need(not original.exists() and not original.is_symlink(), "metadata control backup absent")
            for replace in (False, True):
                self.target.rename(original)
                replacement = None
                try:
                    if replace:
                        self.target.mkdir(mode=0o700)
                        replacement = self.build_identity(self.target)
                    expect("root-replaced" if replace else "root-missing", self.build_roots,
                           "mutable target root changed" if replace else "build scan root unavailable")
                finally:
                    if replacement is not None:
                        need(self.build_identity(self.target) == replacement, "metadata control replacement changed")
                        self.target.rmdir()
                    need(self.build_identity(original) == self.target_before, "metadata control original changed")
                    try:
                        self.target.lstat()
                    except FileNotFoundError as problem:
                        need(problem.errno == errno.ENOENT, "metadata control restore absence")
                    else:
                        raise RuntimeError("metadata control foreign target preserved")
                    original.rename(self.target)
                    self.build_roots()
        finally:
            self.build_roots()
            need(self.build_identity(fixture) == fixture_before, "metadata control fixture replaced")
            for path in reversed(paths):
                need(self.m.ident(path.lstat())[:5] == owned[path], "metadata control file replaced")
                path.unlink()
            if directory_before is not None:
                need(self.build_identity(fixture/"directory") == directory_before, "metadata control directory replaced")
                (fixture/"directory").rmdir()
            fixture.rmdir()
        need(len(negatives) == len(set(negatives)) == 12, "metadata negative roster12")
        self.build_roots()
        self.holds.verify()
        receipt = {"schema": 1, "source": os.environ["GITHUB_SHA"], "input_origin": "authored-metadata-controls",
            "helper_sha256": self.m.regular(Path(__file__).resolve())["sha256"],
            "target": str(self.target), "target_identity": self.target_before,
            "positive_controls": ["actual-du-allocated-apparent-sparse-hardlink-symlink", "strict-child-ENOENT"],
            "negative_controls": negatives, "positive_count": 2, "negative_count": 12,
            "du_sha256": self.m.regular(du)["sha256"], "actual_du_bytes": expected,
            "metadata_bytes": list(measured), "max_attempts": 3, "scan_total_seconds": 10,
            "compiler_started": False, "actual_race_recovered": False, "pass": True}
        raw = (json.dumps(receipt, sort_keys=True, allow_nan=False)+"\n").encode()
        need(len(raw) <= 64*1024, "metadata controls receipt64KiB")
        path = self.out/"build-metadata-controls.json"
        self.m.write_new(path, raw)
        self.holds.add(path, sha(raw))

    def du_failure(self, argv, path, cap, apparent, started, result, error):
        # 첫 실패 원문만 보존한다. errno를 추정하거나 실패를 재시도하지 않는다.
        status = getattr(error, "returncode", None) if result is None else result.returncode
        timed_out = isinstance(error, subprocess.TimeoutExpired)
        streams = {}
        for name, attribute in (("stdout", "output"), ("stderr", "stderr")):
            raw = getattr(error, attribute, None)
            if raw is None:
                raw = getattr(result, name, b"") if result is not None else b""
            need(isinstance(raw, bytes), "du diagnostic requires original bytes")
            retained = raw[:64*1024]
            filename = f"budget-du-first-failure.{name}"
            write(self.out / filename, retained)
            streams[name] = {"file": filename, "captured_bytes": len(raw), "retained_bytes": len(retained),
                "sha256": sha(retained), "truncated": len(raw) != len(retained),
                "capture_complete": status is not None and not timed_out}
        message = str(error)
        value = {"schema": 1, "source": os.environ["GITHUB_SHA"], "phase": self.phase,
            "root": str(path), "scope_cap_bytes": cap, "kind": "apparent" if apparent else "allocated",
            "argv": argv, "returncode": status, "timed_out": timed_out, "timeout_seconds": 10,
            "elapsed_seconds": time.monotonic()-started, "exception": type(error).__name__,
            "exception_message": message[:4096], "exception_message_truncated": len(message) > 4096,
            "raw_prefix_limit_bytes": 64*1024, "streams": streams, "retried": False, "failure_ignored": False}
        raw = (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()
        need(len(raw) <= 16*1024, "du diagnostic index16KiB limit")
        # index는 두 원문 파일 뒤에 게시한다. 세 파일 모두 기존 O_EXCL/NOFOLLOW 경계다.
        write(self.out / "budget-du-first-failure.json", raw)

    def budget(self, force=False, extra_build=0):
        if not force and time.monotonic() - self.last_budget < 2:
            for path in (self.job, self.root):
                v = os.statvfs(path)
                need(v.f_bavail*v.f_frsize >= 512*MIB, "finish reserve exhausted")
            return
        self.last_budget = time.monotonic()
        # registry도 build8GiB에 포함하고 SDK3GiB는 rustup+bootstrap의 합계다.
        scope = [([self.root], 2*GIB), ([self.job / "cargo-home", self.job / "build"], 8*GIB),
                 ([self.job / "rustup-home", self.job / "rust-bootstrap"], 3*GIB),
                 ([self.job / "venv"], GIB), ([self.job / "proof"], 512*MIB)]
        need_by_device, representatives = {}, {}
        for paths, cap in scope:
            allocated = apparent = 0
            for path in paths:
                if path == self.build_root:
                    built = self.measure_build((allocated, apparent), cap)
                    allocated += built[0]
                    apparent += built[1]
                    continue
                if path.exists():
                    for option in ([], ["--apparent-size"]):
                        argv = ["/usr/bin/du", "-sx", "--block-size=1", *option, "--", str(path)]
                        started, result = time.monotonic(), None
                        try:
                            result = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                                    check=True, timeout=10)
                            raw = result.stdout.decode()
                            match = re.fullmatch(r"([0-9]+)\t" + re.escape(str(path)) + "\n", raw)
                            need(match, "unknown scoped du output")
                        except BaseException as error:
                            try:
                                self.du_failure(argv, path, cap, bool(option), started, result, error)
                            except BaseException as evidence_error:
                                error.add_note(f"du failure evidence incomplete: {evidence_error}")
                            raise
                        if option: apparent += int(match[1])
                        else: allocated += int(match[1])
            need(max(allocated, apparent) <= cap, f"scope cap: {paths}")
            if extra_build and cap == 8*GIB:
                need(max(allocated, apparent) + extra_build <= cap, "C projected build/registry8GiB cap")
            parent = paths[0] if paths[0].exists() else paths[0].parent
            device = parent.stat().st_dev
            need(all((p if p.exists() else p.parent).stat().st_dev == device for p in paths), "grouped budget filesystem differs")
            need_by_device[device] = need_by_device.get(device, 0) + cap - allocated
            representatives[device] = parent
        for device, growth in need_by_device.items():
            v = os.statvfs(representatives[device])
            need(v.f_bavail*v.f_frsize >= growth + 512*MIB, "actual filesystem remaining growth admission")

    def command(self, argv, label, seconds, env=None):
        self.phase = label
        need(not self.children.scan(), "live child before command")
        if self.target_before is None and label in ("current-bins", "current-model-bins"):
            need(list(map(str, argv[:2])) == [str(self.sdk/"bin/cargo"), "build"], "first exact Cargo build")
            self.prepare_build_target()
        self.budget(True)
        started, failure, process = time.monotonic(), None, None
        operation_deadline = min(self.deadline, started + seconds)
        if label.startswith('writer-pair-'):
            operation_deadline = min(operation_deadline, self.writer_deadline)
            need(started < operation_deadline, 'D aggregate deadline before process')
        def limits():
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
            resource.setrlimit(resource.RLIMIT_FSIZE, (512*MIB, 512*MIB))
            if label == "triplet-readonly":
                resource.setrlimit(resource.RLIMIT_CPU, (60, 60))
                resource.setrlimit(resource.RLIMIT_AS, (4*GIB, 4*GIB))
            elif label in ('fresh-original','original-plain','original-tapped','native-plain','native-tapped',
                           'cached-context-export','cached-context-native') or label.startswith('writer-pair-'):
                resource.setrlimit(resource.RLIMIT_AS, (4*GIB, 4*GIB))
        try:
            with (self.out / f"{label}.stdout").open("xb") as stdout, (self.out / f"{label}.stderr").open("xb") as stderr:
                process = subprocess.Popen(list(map(str, argv)), cwd=self.root, env=env or self.env,
                    stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, start_new_session=True, preexec_fn=limits)
                self.children.process = process
                while True:
                    rows = self.children.scan()
                    self.maps(rows)
                    self.children.reap()
                    need(time.monotonic() < operation_deadline, f"command deadline: {label}")
                    need(stdout.tell() <= 32*MIB and stderr.tell() <= 32*MIB, "command log32MiB limit")
                    self.budget()
                    if process.returncode is not None:
                        need(not self.children.scan(), "owned child survived leader")
                        need(process.returncode == 0, f"{label}: exit{process.returncode}; preserve stderr")
                        break
                    time.sleep(0.1)
        except BaseException as problem:
            failure = str(problem)
            raise
        finally:
            cleanup_error = None
            live = True
            try:
                self.children.stop()
                live = bool(self.children.scan())
            except BaseException as e:
                cleanup_error = str(e)
            self.commands.append({"label": label, "argv": list(map(str, argv)), "returncode": process.returncode if process else None,
                "error": failure, "cleanup_error": cleanup_error, "elapsed_seconds": time.monotonic()-started, "all_owned_children_reaped": not live})
            self.children.process = None
            need(cleanup_error is None and not live, f'owned cleanup failed: {cleanup_error}')
        self.budget(True)
        return self.m.read(self.out / f"{label}.stdout", 32*MIB)

    def maps(self, rows):
        # 표본 maps이며 dlopen 이전 전체 관측으로 주장하지 않는다. 실제 provider는 끝까지 FD hold한다.
        if time.monotonic()-self.last_maps < 0.5: return
        self.last_maps = time.monotonic()
        for pid, identity in rows.items():
            try:
                raw = Path(f"/proc/{pid}/maps").read_text()
            except (FileNotFoundError, ProcessLookupError):
                continue
            seen = self.provider_maps.setdefault(f"{pid}:{identity['start']}", set())
            for line in raw.splitlines():
                fields = line.split(maxsplit=5)
                if len(fields) != 6 or not fields[5].startswith("/") or "x" not in fields[1]: continue
                need(not fields[5].endswith(" (deleted)"), "deleted executable mapping")
                path = Path(fields[5]).resolve(strict=True)
                need(any(path.is_relative_to(r) for r in (Path('/usr'), Path('/lib').resolve(), self.sdk,
                    self.job / 'venv', Path(os.environ['ORIGINAL_PYTHON_BASE']), self.job / 'build')), "unexpected mapped provider root")
                if path not in seen:
                    self.holds.add(path, elf=True)
                    seen.add(path)

    def sources(self):
        head = self.command(['/usr/bin/git','rev-parse','HEAD'],'source-head',60).decode().strip()
        need(head==os.environ['GITHUB_SHA'], 'actual source HEAD binding')
        raw = self.command(["/usr/bin/git", "ls-files", "-z"], "source-index", 60)
        paths = [self.root / os.fsdecode(p) for p in raw.split(b"\0") if p]
        self.baselines["source"] = (self.root, paths, self.m.snapshot(self.root, paths))
        for name, path in (("sdk", self.sdk), ("python", self.job / "venv"),
                           ("python-base", Path(os.environ["ORIGINAL_PYTHON_BASE"]))):
            self.baselines[name] = (path, None, self.m.snapshot(path, self.m.walk(path)))
        self.holds.add(self.root / COMMON, COMMON_SHA)
        self.holds.add(Path(os.environ["LLVM_PATH"]), LLVM_SHA, elf=True)
        self.holds.add(Path(os.environ["ORIGINAL_PYTHON"]), elf=True)
        self.holds.add(Path('/proc/self/exe').resolve(strict=True), elf=True)
        for name in ("cargo", "rustc", "rustdoc"):
            self.holds.add(self.sdk / f"bin/{name}", elf=True)
        for name in ("git", "du", "clang", "ld", "cc"):
            self.holds.add(Path("/usr/bin") / name, elf=True)
        save(self.out / "baselines.json", {k: v[2] for k, v in self.baselines.items()})

    def test(self, binary, selector, label, seconds, env=None):
        raw = self.command([binary, "--ignored", "--exact", selector, "--nocapture", "--test-threads=1", "--color=never"], label, seconds, env=env)
        text = raw.decode()
        need(len(re.findall(r"^running 1 test$", text, re.M)) == 1 and f"test {selector} ... ok" in text
             and len(re.findall(r"^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; [0-9]+ filtered out;", text, re.M)) == 1,
             f"exact selector1/0/0 summary: {selector}")

    def dso(self, binary, label):
        self.holds.add(binary,elf=True)
        raw=self.command(['/usr/bin/ldd',binary],label,60).decode()
        if raw.strip() == 'statically linked':
            need(len(raw.splitlines()) == 1, 'single zero-needed ldd response')
            binary = Path(binary)
            site = self.job/'venv/lib/python3.12/site-packages'
            need(binary.resolve(strict=True) == binary and binary.is_relative_to(site), 'exact venv extension root')
            relative = str(binary.relative_to(site))
            need(relative in ZERO_NEEDED_NUMPY, 'unknown zero-needed provider')
            size, pin = ZERO_NEEDED_NUMPY[relative]
            data = self.m.read(binary, 64*1024)
            need(len(data) == size and sha(data) == pin, 'source-locked zero-needed NumPy extension')
            elf = zero_needed_elf(data)
            # 실제 admission bytes의 한 경계씩 변조한다. 이 통제는 GHA에서만 실행한다.
            needed, missing, duplicate, executable = (bytearray(data) for _ in range(4))
            struct.pack_into('<q', needed, elf['dynamic_offset'], 1)
            struct.pack_into('<I', missing, elf['dynamic_header'], 0)
            other = elf['phoff'] if elf['phoff'] != elf['dynamic_header'] else elf['phoff']+56
            duplicate[other:other+56] = data[elf['dynamic_header']:elf['dynamic_header']+56]
            struct.pack_into('<H', executable, 16, 2)
            for changed in (needed, missing, duplicate, data[:63], executable):
                try:
                    zero_needed_elf(changed)
                except RuntimeError:
                    pass
                else:
                    raise RuntimeError('zero-needed ELF mutation accepted')
            self.holds.verify()
            save(self.out/(label+'-zero-needed.json'), {'path':str(binary), 'bytes':size, 'sha256':pin,
                'elf':elf, 'declared_needed':0, 'negative_data_controls':5,
                'scope':'exact wheel extension; Python runtime symbol/provider closure is not established'})
            return
        count=0
        for line in raw.splitlines():
            line=line.strip()
            if re.fullmatch(r'linux-vdso\.so\.[0-9]+ \(0x[0-9a-f]+\)',line): continue
            match=re.fullmatch(r'(?:[^ ]+ => )?(/[^ ]+) \(0x[0-9a-f]+\)',line)
            need(match, f'unknown/missing DSO provider: {line}')
            path=Path(match[1]).resolve(strict=True)
            need(any(path.is_relative_to(p) for p in (Path('/usr'),Path('/lib').resolve(),self.sdk,
                self.job/'venv',Path(os.environ['ORIGINAL_PYTHON_BASE']))),'DSO provider root')
            self.holds.add(path,elf=True)
            count+=1
        need(count>0,'empty dependency closure')

    def select(self, raw, name, kind, source, tested):
        rows = [self.m.unique_json(line) for line in raw.splitlines()]
        need(rows and rows[-1] == {"reason": "build-finished", "success": True}
             and sum(r["reason"] == "build-finished" for r in rows) == 1, "Cargo terminal JSONL")
        need(all(r.get("reason") in ("compiler-artifact", "compiler-message", "build-script-executed", "build-finished") for r in rows), "Cargo unknown reason")
        seen, matches = set(), []
        manifest = self.root / "rust/crates/model-runtime/Cargo.toml"
        for row in rows:
            if row["reason"] != "compiler-artifact": continue
            key = json.dumps(row, sort_keys=True)
            need(key not in seen, "duplicate Cargo artifact"); seen.add(key)
            target = row["target"]
            if target["name"] != name or target["kind"] != [kind] or row["profile"]["test"] is not tested: continue
            need(row["package_id"] == f"path+file://{manifest.parent}#openpilot-model-runtime@0.1.0"
                 and row["manifest_path"] == str(manifest) and target["src_path"] == str(self.root / source)
                 and set(row["features"]) == {"cached-program-isa", "native-skip-miri"}, "exact Cargo source/features")
            need(isinstance(row["executable"], str) and row["executable"], "non-null Cargo executable")
            path = Path(row["executable"])
            need(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(self.job / "build/target"), "current private ELF")
            self.holds.add(path, elf=True)
            matches.append(path)
        need(len(matches) == 1, f"unique exact Cargo target: {name}")
        return matches[0]

    def run(self):
        self.sources()
        need(not (self.job/'build/target').exists() and not (self.job/'build/target').is_symlink(), 'fresh current Cargo target must be absent')
        cargo = self.sdk / "bin/cargo"
        common_args = ["--manifest-path", self.root / "rust/Cargo.toml", "--locked", "-p", "openpilot-model-runtime",
                       "--no-default-features", "--features", "cached-program-isa", "--message-format=json"]
        raw = self.command([cargo, "build", *common_args, "--bin", "model-run", "--bin", "offline-llvm-guarded-add",
                            "--bin", "offline-cached-code"], "current-bins", 900)
        binary = self.select(raw, "model-run", "bin", "rust/crates/model-runtime/src/bin/model-run.rs", False)
        guarded = self.select(raw, "offline-llvm-guarded-add", "bin", "rust/crates/model-runtime/src/bin/offline-llvm-guarded-add.rs", False)
        cached_issuer = self.select(raw, "offline-cached-code", "bin", "rust/crates/model-runtime/src/bin/offline-cached-code.rs", False)
        raw = self.command([cargo, "test", *common_args, "--lib", "--no-run"], "current-libtest", 900)
        test = self.select(raw, "openpilot_model_runtime", "lib", "rust/crates/model-runtime/src/lib.rs", True)
        self.command([cargo, "clippy", *common_args, "--lib", "--tests", "--", "-D", "warnings"],
                     "current-clippy", 900)
        registry=self.job/'cargo-home/registry'
        self.baselines['registry']=(registry,None,self.m.snapshot(registry,self.m.walk(registry)))
        for i,path in enumerate((binary,guarded,test,Path(os.environ['ORIGINAL_PYTHON']),Path(sys.executable),Path(os.environ['LLVM_PATH']),self.sdk/'bin/rustc',self.sdk/'bin/cargo',cached_issuer)):
            self.dso(path,f'provider-dso-{i}')
        for i,path in enumerate(sorted((self.job/'venv').rglob('*.so*'))):
            need(path.is_file() and not path.is_symlink(),'extension provider must be regular')
            self.dso(path,f'python-dso-{i}')
        isa_controls = self.command([test, "cached_isa::instruction_isa::tests::", "--nocapture",
                                     "--test-threads=1", "--color=never"], "current-isa-feature-tests", 60).decode()
        need(len(re.findall(r"^test result: ok\. 14 passed; 0 failed; 0 ignored;", isa_controls, re.M)) == 1,
             "all fourteen cached ISA contracts must pass")
        for selector in ("observed_avx512_vbmi_image_requires_its_own_host_feature",
                         "avx512_vbmi_at_roster_end_and_after_return_preserves_other_requirements",
                         "current_host_avx512_vbmi_matches_os_aware_rust_detection"):
            need(isa_controls.splitlines().count("test cached_isa::instruction_isa::tests::" + selector + " ... ok") == 1,
                 "exact VBMI regression missing: " + selector)
        controls = self.command([test, "cached_code::tests::", "--nocapture", "--test-threads=1", "--color=never"],
                                "cached-code-controls", 180).decode()
        need(len(re.findall(r"^test result: ok\. 15 passed; 0 failed; 0 ignored;", controls, re.M)) == 1,
             "all fifteen cached source/object/ABI contracts must pass")
        for selector in ("bool_buffer_pointer_retains_triplet_and_global_argument_binding",
                         "bool_pointer_does_not_admit_other_llvm_parameter_abis"):
            need(controls.splitlines().count("test cached_code::tests::" + selector + " ... ok") == 1,
                 "exact bool pointer ABI regression missing: " + selector)
        closure = ["guarded_add.rs", "bin/offline-llvm-guarded-add.rs", "owned_outputs.rs", "lib.rs", "cached_code.rs",
                   "bin/offline-cached-code.rs", "cached_elf.rs", "cached_isa.rs", "cpu.rs", "graph.rs", "error.rs", "entrypoint.rs", "assets.rs", "buffer.rs", "cpu/diagnostic_482_tests.rs"]
        closure = ["rust/crates/model-runtime/src/"+p for p in closure] + ["rust/crates/model-runtime/Cargo.toml", "rust/Cargo.toml", "rust/Cargo.lock", "rust/rust-toolchain.toml"]
        self.env["MODEL482_SOURCE_PATHS"] = json.dumps(closure)
        self.env["MODEL_LLVM_GUARDED_ADD_BINARY"] = str(guarded)
        self.env["MODEL_LLVM_GUARDED_ADD_SHA256"] = self.m.regular(guarded)["sha256"]
        helper = self.root / "tools/ci/model_current_triplet_482.py"
        python = Path(os.environ["ORIGINAL_PYTHON"])
        self.command([python, "-I", "-B", helper, "collect"], "fresh-original", 900)
        request = self.build / "triplet-request.json"
        self.holds.add(request)
        data = self.m.unique_json(self.m.read(request, 4*MIB))
        for row in data["programs"]:
            for key in ("source", "object", "image"):
                self.holds.add(Path(row[key]["path"]), row[key]["sha256"])
        self.holds.add(Path(data['model']['path']), data['model']['sha256'])
        self.holds.add(Path(data['events']['path']), data['events']['sha256'])
        save(self.out/'holds-before-runtime.json', self.holds.records())
        self.env.update(MODEL482_REQUEST=str(request), MODEL482_REQUEST_SHA256=self.m.regular(request)["sha256"])
        self.test(test, READONLY, "triplet-readonly", 90)
        readonly = self.out / "triplet-readonly.json"
        self.holds.add(readonly)
        admission = self.m.unique_json(self.m.read(readonly, 4*MIB))
        need(admission['strict_triplet_all'] is True and admission['host_isa_all'] is True, "A receipt required before B")
        self.command([python, "-I", "-B", helper, "plain"], "original-plain", 180)
        traffic = self.build / "traffic"
        for path in self.m.walk(traffic):
            if path.is_file(): self.holds.add(path)
        self.dso(traffic/'bundle/kernels.so','bundle-dso')
        sequence = traffic / "sequence.json"
        self.command([binary, "--trusted-bundle", traffic / "bundle", sequence], "native-plain", 180)
        self.command([python, "-I", "-B", helper, "tapped"], "original-tapped", 180)
        b_request = {"schema": 1, "triplet_request": spec_file(self.m, request, 4*MIB), "readonly": spec_file(self.m, readonly, 4*MIB),
                     "traffic": str(traffic), "bundle": str(traffic / "bundle"),
                     "files": {str(p.relative_to(traffic)): spec_file(self.m, p, 512*MIB)
                               for p in self.m.walk(traffic) if p.is_file()}}
        bpath = self.build / "tap-request.json"
        save(bpath, b_request)
        env = {**self.env, "MODEL482_TAP_REQUEST": str(bpath), "MODEL482_TAP_REQUEST_SHA256": self.m.regular(bpath)["sha256"]}
        self.test(test, TAPPED, "native-tapped", 180, env=env)
        native_tap = self.m.unique_json(self.m.read(self.out/'native-worker-cells.json',4*MIB))
        original_tap = self.m.unique_json(self.m.read(self.out/'original-worker-cells.json',4*MIB))
        need(native_tap['plain_tapped_all_outputs_equal'] is True
             and original_tap['plain_tapped_all_outputs_equal'] is True
             and len(native_tap['events'])==len(original_tap['events'])==4, 'B two-frame worker receipt coverage')
        self.phase = 'strict-output-compare'
        descriptors = self.m.unique_json(self.m.read(traffic / "outputs.json"))
        mismatches = []
        for name in descriptors:
            original, native = self.m.read(traffic / "original" / name), self.m.read(traffic / name)
            need(self.m.read(traffic / "original-tapped" / name) == original, "original whole-output tap parity")
            if original != native:
                differing = [i for i, (a, b) in enumerate(zip(original, native)) if a != b]
                mismatches.append({"name": name, "original_sha256": sha(original), "native_sha256": sha(native),
                    "different_byte_offsets": differing[:4096], "offsets_complete": len(differing) <= 4096,
                    "different_byte_count": len(differing) + abs(len(original)-len(native)),
                    "original_bytes": len(original), "native_bytes": len(native)})
        first_original = self.m.read(traffic/'original/model-1.bin')
        first_native = self.m.read(traffic/'model-1.bin')
        context = False
        if len(first_original) == len(first_native) == 2576*4:
            delta = [i for i in range(2576) if first_original[i*4:i*4+4] != first_native[i*4:i*4+4]]
            context = delta == list(CELLS) and all(first_original[i*4:i*4+4] == b'\x00\x00\x00\x80'
                and first_native[i*4:i*4+4] == b'\0'*4 for i in CELLS)
        save(self.out / "comparison.json", {"diagnostic_complete": True, "plain_tapped_bytes_equal": True,
            "original_native_bytes_equal": not mismatches, "mismatches": mismatches,
            "target_context_reproduced": context, "all_other_outputs_equal": {r['name'] for r in mismatches} <= {'model-1.bin'},
            "first_producer_call_observed": native_tap['first_producer_call_observed'],
            "cached_issuer_invoked": False, "cached_runtime_enabled": False, "ssa_first_producer_observed": False,
            "historical_half_snan5_resolved": False, "full128_acceptance": False, "full_model_runtime_acceptance": False,
            "historical_provider_proven": False, "retirement_credit": 0})
        # C is additive. Even a successful C can never promote the existing B mismatch.
        c_error = None
        try:
            self.cached_context(data, request, readonly, traffic, guarded, cached_issuer, test, helper, python)
        except Exception:
            c_error = traceback.format_exc()
            save(self.out / "cached-context-failure.json", {"source": os.environ["GITHUB_SHA"],
                "error": c_error, "b_failure_sticky": bool(mismatches), "numerical_acceptance": False})
        d_error = None
        if c_error is None:
            try:
                self.writer_pair(data, traffic, test, helper, python)
            except Exception:
                d_error = traceback.format_exc()
                save(self.out/'writer-pair-failure.json', {'source': os.environ['GITHUB_SHA'], 'error': d_error,
                    'b_failure_sticky': bool(mismatches), 'numerical_acceptance': False})
        else:
            save(self.out/'writer-pair-skipped.json', {'source': os.environ['GITHUB_SHA'], 'attempted': False,
                'reason': 'D requires successful strict C12', 'b_failure_sticky': bool(mismatches)})
        need(not mismatches, "strict original/native byte mismatch retained; diagnostic is not numerical acceptance")
        need(c_error is None, "additive cached-context diagnostic failed; existing A/B retained")
        need(d_error is None, "additive writer-pair diagnostic failed; existing A/B/C retained")

    def cached_context(self, data, request, readonly, traffic, guarded, issuer, test, helper, python):
        self.holds.verify()  # A/B identities must still match before deriving C expectations.
        canonical = lambda v: json.dumps(v, sort_keys=True, separators=(',', ':'), ensure_ascii=False, allow_nan=False).encode()
        proof = self.m.unique_json(self.m.read(readonly, 4*MIB))
        need(proof['strict_triplet_all'] is True and proof['host_isa_all'] is True
             and proof['source'] == data['source_sha256'] == os.environ['GITHUB_SHA']
             and proof['complete_roster'] == len(data['programs']) == len(proof['programs']) == 91,
             "C requires independent A complete fresh91")
        destination = self.build/'cached-context'
        need(not destination.exists() and not destination.is_symlink(), "fresh C namespace")
        held_originals = self.holds.records()
        original_seals = {path: {key: held_originals[str(path)][key] for key in ('identity', 'sha256')}
                          for path in (guarded, issuer)}
        helper_copy_bytes = sum(seal['identity'][5] for seal in original_seals.values())
        # Existing 8GiB build/512MiB proof/finish reserve and global deadline stay fixed.
        existing_outputs = self.m.unique_json(self.m.read(traffic/'outputs.json', 4*MIB))
        need(len(existing_outputs) == 12 and all(Path(name).name == name for name in existing_outputs), "C existing twelve B outputs")
        output_forecast = sum((traffic/'original'/name).stat().st_size for name in existing_outputs)
        need(output_forecast <= 64*MIB, "C existing output total64MiB")
        forecast = ((traffic/'bundle/weights.bin').stat().st_size + output_forecast + helper_copy_bytes
                    + 4*sum(p['image']['bytes'] for p in data['programs']) + 64*MIB)
        need(forecast <= 256*MIB, "C additive growth forecast256MiB")
        self.budget(force=True, extra_build=forecast)
        v = os.statvfs(self.build)
        need(v.f_bavail*v.f_frsize >= forecast + 512*MIB, "C growth plus existing finish reserve")
        destination.mkdir(mode=0o700)
        temporary = self.target/'model482-cached-tmp'
        need(not temporary.exists() and not temporary.is_symlink(), "fresh C temporary namespace")
        temporary.mkdir(mode=0o700)
        save(self.out/'cached-context-capacity.json', {'forecast_bytes': forecast, 'existing_output_bytes': output_forecast,
            'helper_copy_bytes': helper_copy_bytes,
            'candidate_cap_bytes': 256*MIB, 'existing_build_registry_cap_bytes': 8*GIB,
            'existing_proof_cap_bytes': 512*MIB, 'remaining_seconds': self.deadline-time.monotonic(),
            'temporary': str(temporary), 'old_caps_or_deadline_extended': False})
        helper_directory = destination/'helpers'
        helper_directory.mkdir(mode=0o700)
        info = helper_directory.lstat()
        need(stat.S_ISDIR(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o700
             and info.st_uid == os.geteuid() and helper_directory.resolve(strict=True) == helper_directory,
             "C private helper directory")
        copied_helpers = []
        def copy_held_helper(original):
            before = original_seals[original]
            need(original.resolve(strict=True) == original and self.m.regular(original, 128*MIB) == before,
                 "C original helper held identity/SHA before copy")
            need(64 <= before['identity'][5] <= 128*MIB and before['identity'][3] == os.geteuid()
                 and before['identity'][2] & 0o111, "C source-selected owned executable")
            target = helper_directory/original.name
            # Pinned common copy_new streams to O_CREAT|O_EXCL|O_NOFOLLOW at mode0400.
            # Only this newly created destination is then made executable.
            self.m.copy_new(original, target, 128*MIB)
            fd = os.open(target, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
            try:
                info = os.fstat(fd)
                need(stat.S_ISREG(info.st_mode) and info.st_uid == os.geteuid() and info.st_nlink == 1
                     and info.st_size == before['identity'][5] and target.resolve(strict=True) == target
                     and self.m.ident(info) == self.m.ident(target.lstat()), "C helper copy regular/owner/link/size")
                os.fchmod(fd, 0o700)
                after = os.fstat(fd)
                need(stat.S_IMODE(after.st_mode) == 0o700 and after.st_nlink == 1
                     and self.m.digest(fd) == before['sha256']
                     and self.m.ident(after) == self.m.ident(os.fstat(fd)) == self.m.ident(target.lstat()),
                     "C helper copy mode/SHA/identity")
            finally:
                os.close(fd)
            source_after = self.m.regular(original, 128*MIB)
            need(source_after == before, "C original helper held identity/SHA after copy")
            self.holds.add(target, before['sha256'], elf=True)
            copied_helpers.append({'source': str(original), 'source_before': before, 'source_after': source_after,
                                   'copy': str(target), **self.m.regular(target, 128*MIB)})
            return target
        guarded, issuer = copy_held_helper(guarded), copy_held_helper(issuer)
        self.holds.verify()  # Original Cargo aliases and both new nlink1 copies stay held.
        copy_receipt = self.out/'cached-context-helper-copies.json'
        save(copy_receipt, {'source': os.environ['GITHUB_SHA'], 'copied_bytes': helper_copy_bytes,
                            'originals_unchanged': True, 'copies': copied_helpers})
        self.holds.add(copy_receipt)
        def held_binding(path):
            row = spec_file(self.m, path, 128*MIB)
            info = path.stat()
            return {'enabled': True, **row,
                    'identity': [info.st_dev, info.st_ino, info.st_size, info.st_mode, info.st_mtime_ns, info.st_ctime_ns]}
        guarded_binding, issuer_binding = held_binding(guarded), held_binding(issuer)
        guarded_paths = ['guarded_add.rs', 'bin/offline-llvm-guarded-add.rs', 'owned_outputs.rs', 'lib.rs']
        cached_paths = ['cached_code.rs', 'bin/offline-cached-code.rs', 'cached_elf.rs', 'cached_isa.rs',
                        'cpu.rs', 'graph.rs', 'error.rs', 'entrypoint.rs', 'assets.rs', 'buffer.rs']
        guarded_paths = ['rust/crates/model-runtime/src/'+p for p in guarded_paths] + [
            'rust/crates/model-runtime/Cargo.toml', 'rust/Cargo.toml', 'rust/Cargo.lock', 'rust/rust-toolchain.toml']
        cached_paths = ['rust/crates/model-runtime/src/'+p for p in cached_paths]
        seals = lambda paths: [{'path': p, 'sha256': data['sources'][p]} for p in paths]
        exporter = {'source': os.environ['GITHUB_SHA'], 'files': {
            str(p.relative_to(self.root)): spec_file(self.m, p, 32*MIB)['sha256']
            for p in self.m.walk(self.root/'rust/tools/model_export') if p.is_file()}}
        roster_path = destination/'exporter-roster.json'
        save(roster_path, exporter)
        isa = {'status': 'COMPLETE_INERT_ISA_ADMISSION', 'generated_code_executed': False,
               'ABI_or_numeric_acceptance': False, 'host_features': proof['host_features'], 'programs': []}
        for index, (row, decoded) in enumerate(zip(data['programs'], proof['programs'], strict=True)):
            need(row['index'] == decoded['index'] == index, "C original/A index order")
            isa['programs'].append({'index': index, 'source_sha256': row['source']['sha256'],
                'image_sha256': row['image']['sha256'], 'text_bytes': decoded['text_bytes'],
                'instructions': decoded['instructions'], 'required': decoded['required']})
        isa_path = destination/'independent-isa.json'
        save(isa_path, isa)
        current = {'helper_sha256': guarded_binding['sha256'], 'helper_binding_sha256': sha(canonical(guarded_binding)),
            'cached_helper_sha256': issuer_binding['sha256'], 'cached_helper_binding_sha256': sha(canonical(issuer_binding)),
            'guarded_add_sources': seals(guarded_paths), 'cached_sources': seals(cached_paths),
            'exporter_roster_sha256': spec_file(self.m, roster_path, 4*MIB)['sha256'],
            'isa_verifier_sha256': spec_file(self.m, test, 128*MIB)['sha256'],
            'isa_verifier_source_sha256': data['sources']['rust/crates/model-runtime/src/cached_isa.rs'],
            'isa_receipt_sha256': spec_file(self.m, isa_path, 4*MIB)['sha256']}
        admission = {'original_manifest_sha256': spec_file(self.m, request, 4*MIB)['sha256'],
            'original_model_sha256': data['model']['sha256'], 'current': current,
            'programs': [row['original'] for row in data['programs']]}
        admission_path = destination/'independent-admission.json'
        save(admission_path, admission)
        issuer_request = {'source_root': str(self.root), 'original_manifest': str(request),
            'original_model': data['model']['path'], 'isa_receipt': str(isa_path), 'admission': admission,
            'programs': [{'original_index': row['index'], 'kernel': row['kernel'], 'source': row['source']['path'],
                          'image': row['image']['path'], 'reconstruction_object': row['object']['path']}
                         for row in data['programs']], 'library': None, 'graph': None}
        issuer_path = destination/'issuer-request.json'
        save(issuer_path, issuer_request)
        for path in (roster_path, isa_path, admission_path, issuer_path): self.holds.add(path)
        env = {**self.env, 'MODEL_CACHED_CODE_BINARY': str(issuer), 'MODEL_CACHED_CODE_SHA256': issuer_binding['sha256'],
            'MODEL_CACHED_CODE_REQUEST': str(issuer_path), 'MODEL_CACHED_CODE_REQUEST_SHA256': spec_file(self.m, issuer_path, 4*MIB)['sha256'],
            'MODEL_LLVM_GUARDED_ADD_BINARY': str(guarded), 'MODEL_LLVM_GUARDED_ADD_SHA256': guarded_binding['sha256'],
            'TMPDIR': str(temporary)}
        self.command([python, '-I', '-B', helper, 'cached-context-export'], 'cached-context-export', 180, env=env)
        self.holds.verify()  # Never re-pin changed B original outputs as C goldens.
        need(sum(p.stat().st_size for p in self.m.walk(destination) if p.is_file()) + 68*MIB <= 256*MIB,
             "C candidate plus output64MiB/request4MiB within256MiB")
        bundle = destination/'bundle'
        self.dso(bundle/'kernels.so', 'cached-context-bundle-dso')
        for path in self.m.walk(bundle):
            if path.is_file(): self.holds.add(path)
        c_request = {'schema': 1, 'triplet_request': spec_file(self.m, request, 4*MIB),
            'readonly': spec_file(self.m, readonly, 4*MIB), 'admission': spec_file(self.m, admission_path, 4*MIB),
            'issuer': spec_file(self.m, issuer, 128*MIB), 'guarded': spec_file(self.m, guarded, 128*MIB),
            'exporter_roster': spec_file(self.m, roster_path, 4*MIB), 'traffic': str(traffic), 'bundle': str(bundle),
            'diagnostic': spec_file(self.m, self.root/'rust/crates/model-runtime/src/cpu/diagnostic_482_cached.rs', 32*MIB),
            'bundle_files': {str(p.relative_to(bundle)): spec_file(self.m, p, 512*MIB)
                             for p in self.m.walk(bundle) if p.is_file()},
            'files': {str(p.relative_to(traffic)): spec_file(self.m, p, 512*MIB)
                      for p in self.m.walk(traffic) if p.is_file()}}
        cpath = destination/'native-request.json'
        need(len(json.dumps(c_request, sort_keys=True, indent=2).encode()) + 1 <= 4*MIB, "C request4MiB before write")
        save(cpath, c_request)
        self.holds.add(cpath)
        env = {**self.env, 'MODEL482_CACHED_REQUEST': str(cpath),
               'MODEL482_CACHED_REQUEST_SHA256': spec_file(self.m, cpath, 4*MIB)['sha256']}
        self.test(test, CACHED_CONTEXT, 'cached-context-native', 180, env=env)
        result = self.m.unique_json(self.m.read(self.out/'cached-context.json', 4*MIB))
        need(result['diagnostic_complete'] is True and result['existing_original_all12_equal'] is True
             and set(result['outputs']) == set(existing_outputs), "C complete existing-output comparison")
        self.holds.verify()
        for name, row in result['outputs'].items():
            actual = self.m.read(destination/'outputs'/name, 64*MIB)
            original = self.m.read(traffic/'original'/name, 64*MIB)
            need(actual == original and sha(actual) == row['cached_sha256'] == row['existing_original_sha256'],
                 "C retained raw/current original bytes differ: " + name)
        for path in self.m.walk(destination/'outputs'):
            if path.is_file(): self.holds.add(path)
        need(sum(p.stat().st_size for p in self.m.walk(destination) if p.is_file()) <= 256*MIB,
             "C actual retained namespace256MiB cap")

    def writer_pair(self, data, traffic, test, helper, python):
        self.holds.verify()
        parent = self.build/'cached-context'
        destination = parent/'writer-pair'
        need(not destination.exists() and not destination.is_symlink(), 'fresh D namespace')
        baseline = self.m.unique_json(self.m.read(self.out/'cached-context.json', 4*MIB))
        need(baseline['diagnostic_complete'] is True and baseline['existing_original_all12_equal'] is True
             and baseline['source'] == os.environ['GITHUB_SHA'], 'D requires actual C12 success')
        selected = [p for p in data['programs'] if p['original']['source_sha256'] == STORE_SOURCE]
        need(len(selected) == 1 and selected[0]['kernel'] == 92, 'D exact writer92')
        selected = selected[0]
        outputs = self.m.unique_json(self.m.read(traffic/'outputs.json', 4*MIB))
        output_bytes = sum((traffic/'original'/n).stat().st_size for n in outputs)
        forecast = 2*output_bytes + 4*selected['source']['bytes'] + 8*selected['image']['bytes'] + 32*MIB
        used = lambda: sum(p.stat().st_size for p in self.m.walk(parent) if p.is_file())
        need(used() + forecast <= 256*MIB, 'C+D aggregate forecast256MiB')
        self.budget(force=True, extra_build=forecast)
        destination.mkdir(mode=0o700)
        deadline = min(self.deadline, time.monotonic()+180)
        self.writer_deadline = deadline
        def remaining():
            value = int(deadline-time.monotonic())
            need(value > 0, 'D aggregate180s/global deadline')
            return value
        save(self.out/'writer-pair-capacity.json', {'forecast_bytes': forecast, 'existing_C_bytes': used(),
            'aggregate_candidate_cap_bytes': 256*MIB, 'output_bytes_per_candidate': output_bytes,
            'aggregate_deadline_seconds': 180, 'old_caps_or_deadline_extended': False})
        events = self.m.unique_json(self.m.read(Path(data['events']['path']), 16*MIB))
        chain = events['chains'][selected['compile_event']]
        need(chain['source'] == selected['source'] and chain['object'] == selected['object'], 'D actual A config chain')
        config_path = destination/'original-config.json'
        save(config_path, chain['config'])
        files = {'original': selected['source'], 'guarded': spec_file(self.m, traffic/'bundle/kernel-92.compiled.ll'),
                 'rewrite': spec_file(self.m, traffic/'bundle/kernel-92.guarded-add.json'), 'config': spec_file(self.m, config_path)}
        compile_path = destination/'compile-request.json'
        save(compile_path, {'schema': 1, 'source': os.environ['GITHUB_SHA'], 'files': files})
        for path in (config_path, compile_path): self.holds.add(path)
        cpath = parent/'native-request.json'
        c_request = self.m.unique_json(self.m.read(cpath, 4*MIB))
        env = {**self.env, 'MODEL482_WRITER_COMPILE_REQUEST_SHA256': spec_file(self.m, compile_path)['sha256'],
               'MODEL_LLVM_GUARDED_ADD_BINARY': c_request['guarded']['path'],
               'MODEL_LLVM_GUARDED_ADD_SHA256': c_request['guarded']['sha256'],
               'TMPDIR': str(self.target/'model482-cached-tmp')}
        self.command([python, '-I', '-B', helper, 'writer-pair-compile'], 'writer-pair-compile', remaining(), env=env)
        self.holds.verify()
        compiler_receipt = destination/'compiler-receipt.json'
        variants = {name: {'source': spec_file(self.m, destination/(name+'.ll')),
                           'object': spec_file(self.m, destination/(name+'.o')), 'library': None}
                    for name in ('rename-only', 'guarded')}
        # 실제 object로 후속 image/container/DSO 증가량을 재산정하며 같은 cap을 유지한다.
        transport_forecast = 4*sum(v['object']['bytes']+64*1024 for v in variants.values()) + 2*output_bytes + 4*MIB
        need(used() + transport_forecast <= 256*MIB, 'C+D measured object transport/output forecast256MiB')
        self.budget(force=True, extra_build=transport_forecast)
        params = {'schema': 1, 'cached_request': spec_file(self.m, cpath), 'baseline_receipt': spec_file(self.m, self.out/'cached-context.json'),
                  'diagnostic': spec_file(self.m, self.root/'rust/crates/model-runtime/src/cpu/diagnostic_482_writer.rs'),
                  'compile_request': spec_file(self.m, compile_path), 'compiler_receipt': spec_file(self.m, compiler_receipt),
                  'variants': variants}
        def test_request(name):
            self.holds.verify()
            path = destination/(name+'-request.json')
            save(path, params)
            for p in self.m.walk(destination):
                if p.is_file(): self.holds.add(p)
            return {**self.env, 'MODEL482_WRITER_REQUEST': str(path),
                    'MODEL482_WRITER_REQUEST_SHA256': spec_file(self.m, path)['sha256']}
        prepare_env = test_request('prepare')
        self.test(test, WRITER_PREPARE, 'writer-pair-prepare', remaining(), env=prepare_env)
        for name in variants:
            package, library = destination/(name+'.container.o'), destination/(name+'.so')
            self.holds.add(package)
            self.command([self.env['CC'], '-shared', '-nostdlib', '-Wl,--no-undefined', '-Wl,-z,noexecstack',
                          package, '-o', library], 'writer-pair-link-'+name, remaining())
            self.holds.add(library, elf=True)
            variants[name]['library'] = spec_file(self.m, library)
        need(used() + 2*output_bytes + 4*MIB <= 256*MIB, 'C+D actual plus output/request256MiB')
        native_env = test_request('native')
        self.test(test, WRITER_EXECUTE, 'writer-pair-native', remaining(), env=native_env)
        self.holds.verify()
        result = self.m.unique_json(self.m.read(self.out/'writer-pair.json', 4*MIB))
        need(result['diagnostic_complete'] is True and result['retained_original_programs'] == 90
             and set(result['variants']) == set(variants), 'D complete paired execution receipt')
        for name, candidate in result['variants'].items():
            need(set(candidate['outputs']) == set(outputs), 'D exact existing output12 roster')
            for output, row in candidate['outputs'].items():
                actual = self.m.read(destination/'outputs'/name/output, 64*MIB)
                original = self.m.read(traffic/'original'/output, 64*MIB)
                need(sha(actual) == row['actual_sha256'] and sha(original) == row['existing_original_sha256']
                     and (actual == original) == row['equal'], 'D independently checked raw comparison')
        for p in self.m.walk(destination):
            if p.is_file(): self.holds.add(p)
        need(used() <= 256*MIB, 'C+D aggregate actual256MiB')
        remaining()

    def finish(self, error):
        failures = []
        try:
            self.children.stop()
        except BaseException as e:
            failures.append(str(e))
        for name, (root, paths, before) in self.baselines.items():
            try:
                after = self.m.snapshot(root, paths if paths is not None else self.m.walk(root))
                save(self.out / f"{name}-after.json", after)
                need(before == after, f"{name} seal changed")
            except BaseException as e:
                failures.append(f"{name}: {e}")
        try:
            self.holds.verify()
        except BaseException as e:
            failures.append(f"held: {e}")
        try:
            self.retain()
        except BaseException as e:
            failures.append(f"retention: {e}")
        save(self.out / "holds-after.json", self.holds.records())
        save(self.out / "sampled-provider-maps.json", {k: sorted(map(str, v)) for k, v in self.provider_maps.items()})
        save(self.out / "commands.json", self.commands)
        try:
            live = list(self.children.scan())
        except BaseException as e:
            failures.append(f'child state unknown: {e}')
            live = ['unknown']
        save(self.out / "children.json", {"subreaper": True, "live": live, "events": self.children.history})
        missing={'source','sdk','python','python-base','registry'}-set(self.baselines)
        failures.extend(f'baseline unavailable: {name}' for name in sorted(missing))
        save(self.out / "final-seals.json", {"pass": not failures and not live,
            "failures": failures, "source": os.environ["GITHUB_SHA"], "phase": self.phase,
            "error": error, "all_owned_children_reaped": not live})
        self.holds.close()
        os.close(self.children.journal)
        need(not failures and not live, "independent final seals failed")

    def retain(self):
        # build 전체/weights/PKL를 artifact로 복제하지 않는다. 선택 원문과 실제 raw 출력을 보존한다.
        selected = set()
        request = self.build / 'triplet-request.json'
        for name in ('triplet-request.json', 'tap-request.json', 'compile-events.json'):
            if (self.build/name).is_file(): selected.add(self.build/name)
        if request.is_file():
            value = self.m.unique_json(self.m.read(request, 4*MIB))
            for row in value['programs']:
                selected.update(Path(row[k]['path']) for k in ('source','object','image'))
        elif (self.build/'triplets').is_dir():
            selected.update(p for p in self.m.walk(self.build/'triplets') if p.is_file())
        traffic = self.build/'traffic'
        if traffic.is_dir():
            for path in self.m.walk(traffic):
                if path.is_file() and path.name != 'weights.bin': selected.add(path)
        cached = self.build/'cached-context'
        if cached.is_dir():
            for path in self.m.walk(cached):
                if path.is_file() and path.name != 'weights.bin': selected.add(path)
        total = sum(p.stat().st_size for p in selected)
        proof_size = sum(p.stat().st_size for p in self.m.walk(self.job/'proof') if p.is_file())
        need(total + proof_size <= 512*MIB, 'retained proof512MiB apparent cap')
        free = os.statvfs(self.out)
        need(free.f_bavail*free.f_frsize >= total + 512*MIB, 'retention plus finish reserve')
        destination = self.out/'retained'
        destination.mkdir(mode=0o700)
        records = []
        for path in sorted(selected):
            need(path.is_relative_to(self.build) and stat.S_ISREG(path.lstat().st_mode), 'retention exact regular source')
            relative = path.relative_to(self.build)
            target = destination/relative
            target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            self.m.copy_new(path, target, 512*MIB)
            records.append({'runtime_path':str(path),'artifact_path':str(target.relative_to(self.job/'proof')),
                            **self.m.regular(target)})
        save(self.out/'retained-index.json', {'files':records,'bytes':total,'model_pkl_uploaded':False,
            'all_selected_original_triplets_uploaded':request.is_file(),'full_weights_uploaded':False})


def main():
    def interrupted(number, _frame):
        raise InterruptedError(f'model482 interrupted by signal{number}')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    root, job = trusted()
    m = common(root)
    if len(sys.argv) == 2:
        need(sys.argv[1] in ("collect", "plain", "tapped", "cached-context-export", "writer-pair-compile"), "unknown worker mode")
        if sys.argv[1] == "collect": collect(root, job, m)
        elif sys.argv[1] == "cached-context-export": cached_context_export(root, job, m)
        elif sys.argv[1] == "writer-pair-compile": writer_pair_compile(root, job, m)
        else: two_frames(root, job, m, sys.argv[1] == "tapped")
        return
    need(len(sys.argv) == 1 and sys.flags.isolated and sys.flags.no_site, "stdlib controller requires -I -B -S")
    controller = Controller(root, job, m)
    error = None
    try:
        controller.run()
    except BaseException:
        error = traceback.format_exc()
        raise
    finally:
        controller.finish(error)


if __name__ == "__main__":
    main()
