#!/usr/bin/env python3
"""#482 Actions 전용 fresh triplet/2frame 관측. 로컬 기능 실행 금지.

기존 수치 gate, cached issuer와 production 경로를 변경하지 않는다.
controller만 stdlib이며 worker의 원본 import/compile/PKL 실행은 Actions에서만 한다.
"""
from __future__ import annotations

import ctypes
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
COMMON_SHA = "937f9a0adca788625e291b147be48445d14235a4c1a0c61e9f0b4d61881c113b"
LLVM_SHA = "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
STORE_SOURCE = "e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0"
PREFIX = "cpu::diagnostic_482_tests::"
READONLY = PREFIX + "fresh_program_roster_triplets_and_host_isa_are_bound_without_execution"
TAPPED = PREFIX + "bound_road1928_two_frame_producer_capture"
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

    def budget(self, force=False):
        if not force and time.monotonic() - self.last_budget < 2:
            for path in (self.job, self.root):
                v = os.statvfs(path)
                need(v.f_bavail*v.f_frsize >= 512*MIB, "finish reserve exhausted")
            return
        self.last_budget = time.monotonic()
        # registry도 build8GiB에 포함하고 SDK3GiB는 rustup+bootstrap의 합계다.
        scope = [([self.root], 2*GIB), ([self.job / "build", self.job / "cargo-home"], 8*GIB),
                 ([self.job / "rustup-home", self.job / "rust-bootstrap"], 3*GIB),
                 ([self.job / "venv"], GIB), ([self.job / "proof"], 512*MIB)]
        need_by_device, representatives = {}, {}
        for paths, cap in scope:
            allocated = apparent = 0
            for path in paths:
                if path.exists():
                    for option in ([], ["--apparent-size"]):
                        raw = subprocess.check_output(["/usr/bin/du", "-sx", "--block-size=1", *option, "--", str(path)],
                                                      stderr=subprocess.PIPE, timeout=10).decode()
                        match = re.fullmatch(r"([0-9]+)\t" + re.escape(str(path)) + "\n", raw)
                        need(match, "unknown scoped du output")
                        if option: apparent += int(match[1])
                        else: allocated += int(match[1])
            need(max(allocated, apparent) <= cap, f"scope cap: {paths}")
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
        self.budget(True)
        started, failure, process = time.monotonic(), None, None
        def limits():
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
            resource.setrlimit(resource.RLIMIT_FSIZE, (512*MIB, 512*MIB))
            if label == "triplet-readonly":
                resource.setrlimit(resource.RLIMIT_CPU, (60, 60))
                resource.setrlimit(resource.RLIMIT_AS, (4*GIB, 4*GIB))
            elif label in ('fresh-original','original-plain','original-tapped','native-plain','native-tapped'):
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
                    need(time.monotonic() < min(self.deadline, started + seconds), f"command deadline: {label}")
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
        raw = self.command([cargo, "build", *common_args, "--bin", "model-run", "--bin", "offline-llvm-guarded-add"], "current-bins", 900)
        binary = self.select(raw, "model-run", "bin", "rust/crates/model-runtime/src/bin/model-run.rs", False)
        guarded = self.select(raw, "offline-llvm-guarded-add", "bin", "rust/crates/model-runtime/src/bin/offline-llvm-guarded-add.rs", False)
        raw = self.command([cargo, "test", *common_args, "--lib", "--no-run"], "current-libtest", 900)
        test = self.select(raw, "openpilot_model_runtime", "lib", "rust/crates/model-runtime/src/lib.rs", True)
        self.command([cargo, "clippy", *common_args, "--lib", "--tests", "--", "-D", "warnings"],
                     "current-clippy", 900)
        registry=self.job/'cargo-home/registry'
        self.baselines['registry']=(registry,None,self.m.snapshot(registry,self.m.walk(registry)))
        for i,path in enumerate((binary,guarded,test,Path(os.environ['ORIGINAL_PYTHON']),Path(sys.executable),Path(os.environ['LLVM_PATH']),self.sdk/'bin/rustc',self.sdk/'bin/cargo')):
            self.dso(path,f'provider-dso-{i}')
        for i,path in enumerate(sorted((self.job/'venv').rglob('*.so*'))):
            need(path.is_file() and not path.is_symlink(),'extension provider must be regular')
            self.dso(path,f'python-dso-{i}')
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
        need(not mismatches, "strict original/native byte mismatch retained; diagnostic is not numerical acceptance")

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
        need(sys.argv[1] in ("collect", "plain", "tapped"), "unknown worker mode")
        if sys.argv[1] == "collect": collect(root, job, m)
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
