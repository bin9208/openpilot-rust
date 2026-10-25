#!/usr/bin/env python3
"""#482 PR1344 same-VM A→D1→D2. Actions 전용, 로컬 import/실행 금지.
기존 strict 실패는 sticky이며 expected/수학/production/cached issuer를 바꾸지 않는다.
"""
from __future__ import annotations
import ctypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import stat
import sys
import threading
import time
import traceback

MIB, GIB = 1024**2, 1024**3
BASE = "tools/ci/model_current_triplet_482.py"
BASE_SHA = "6b173bb32ec045bbef9a0d2a968555a9318ff031e1d4cb1574a5254033bf2759"
WORKFLOW = "bin9208/openpilot-rust/.github/workflows/python-rust-523-model-pr1344-replay.yml@refs/heads/codex/feat-523-python-rust"
FIXTURE_SHA = "afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d"
ARCHIVE_SHA = "3ddfb1213cccbbf0d4d0ef7854b79cef5637acdfd7b59d883b13063d6968b4fe"
OBSERVATION_SHA = "ed174bb4610ee55108b3f2502d7ed284cb66abb3cd153777ca546bf8fe0d3fca"
PKL_SHA = "487ef8698775a593726c79ef0651940480f755f5ded57f672d0ee3a4bd8a624f"
CALIBRATION_SHA = "b8ba3f01c8fbca2308dd17d7d3077bd68cb81e78eaa678b6538f331b54fa0637"
FROZEN_RAW_SHA = "9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe"
PREFIX = "cpu::diagnostic_482_tests::pr1344::"
READONLY = PREFIX + "frozen_source_whole_triplets_before_runtime"
NATIVE = PREFIX + "first_packet_whole_outputs_and_writer99"
D1 = "model_482_pr1344::same_vm_current_dual_first_packet"
STORE_SOURCE = "e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0"


def need(value, message):
    if not value: raise RuntimeError(message)


def load_base(root):
    path = root / BASE
    fd = os.open(path, os.O_RDONLY|os.O_CLOEXEC|os.O_NOFOLLOW|os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        raw = os.pread(fd, 128*1024+1, 0)
        identity = lambda s:(s.st_dev,s.st_ino,s.st_mode,s.st_uid,s.st_size,s.st_mtime_ns,s.st_ctime_ns)
        need(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid() and len(raw) == before.st_size <= 128*1024
             and hashlib.sha256(raw).hexdigest() == BASE_SHA
             and identity(before)==identity(os.fstat(fd))==identity(path.lstat()), "pinned base source/FD identity")
        module = importlib.util.module_from_spec(importlib.util.spec_from_file_location("_pr1344_base",path))
        exec(compile(raw,str(path),"exec"),module.__dict__)
        return module
    finally: os.close(fd)


# 아래 이름은 검증된 기존 adapter의 같은 계약을 사용한다. 새 controller/worker에서만 바인딩한다.
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
    events, chains, runtime_rows = [], [], []
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

    def initialized(*args, **kwargs):
        raise RuntimeError("A collection forbids CPUProgram construction/execution")

    device.diskcache_get, device.diskcache_put, device.Compiler.compile_cached = get, put, cached_call
    compiler_cpu.LLVMCompiler.compile_to_obj, compiler_cpu.jit_loader, elf.elf_loader = obj, link, layout
    CPUProgram.__init__ = initialized
    for name,delegate in setters.items(): setattr(llvm,name,setter(name,delegate))
    model = build / "fixture/models/driving_tinygrad.pkl"
    try:
        from model_export.original import driving
        from model_export.pipeline import linear_calls
        from model_export.buffers import host_buffer, BufferTable
        from tinygrad.engine.realize import resolve_params
        from tinygrad.uop.ops import Ops
        original = driving(model, (1344, 760))
        compiler = compiler_cpu.CPULLVMCompiler(["x86_64", "native"])
        table = BufferTable()
        def view(buffer): return table.view(host_buffer(buffer))
        input_views, output_views = {}, {}
        for mapping, tensors in ((input_views, original.bindings.inputs), (output_views, original.bindings.outputs)):
            for name, tensor in tensors.items():
                buffer = host_buffer(tensor.uop.base.buffer if mapping is input_views else tensor.uop.buffer)
                table.read(buffer)
                mapping[name] = view(buffer)
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
                table.read(buffers[1]); table.write(buffers[0])
                calls.append({"index": len(calls), "program": None, "kernel": kernels[key], "workers": 1, "core_id": None,
                              "views":[view(b) for b in buffers],"scalars":[]})
                continue
            info = op.arg
            need(all(v.expr=='core_id' for v in info.vars) and all(type(v) is int for v in info.global_size)
                 and tuple(info.global_size[1:])==(1,1) and (info.local_size is None or tuple(info.local_size)==(1,1,1)),
                 'static original CPU launch/scalars required')
            source = op.src[3].arg.encode()
            old_image = op.src[4].arg
            image = compiler.compile_cached(source.decode()) if op not in operations else None
            kernels.setdefault(source, len(kernels))
            if op not in operations:
                matches = [r for r in chains if r["source"]["sha256"] == sha(source) and r["image"]["sha256"] == sha(image)]
                unique = {(r["object"]["sha256"], r["layout"]["text_bytes"], r["layout"]["image_alignment"]) for r in matches}
                need(len(unique) == 1 and matches, "missing/ambiguous actual first compile object")
                chain = matches[0]
                alignment = chain["layout"]["image_alignment"]
                need(alignment > 0, "object layout alignment")
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
                runtime_rows.append({"key":op.key.hex(), "device":buffers[info.globals[0]].device,
                    "name":info.function_name,"image":chain["image"],"alignment":alignment,
                    "old_binary_sha256":sha(old_image),"source_sha256":sha(source)})
            for i in info.ins: table.read(buffers[i])
            for i in info.outs: table.write(buffers[i])
            calls.append({"index": len(calls), "program": operations[op], "kernel": kernels[source],
                          "workers": info.global_size[0], "core_id": info.runtimevars.get("core_id"),
                          "views":[view(buffers[i]) for i in info.globals], "scalars":[0 for _ in info.vars]})
        need(len(programs) == 91 and len(calls) <= 1_000_000, "frozen complete91 PROGRAM roster bounds")
        need(len({r["key"] for r in runtime_rows}) == len(runtime_rows), "unique runtime key roster")
        save(build / "fresh-runtime.json", runtime_rows)
        save(build / "entrypoints.json", [{"name":e.name,"start":e.start,"end":e.end} for e in original.pipeline.entrypoints])
        save(build / "initial.json", [{"bytes":len(raw),"sha256":sha(raw)} for raw in table.initial])
        allocations = [len(raw) for raw in table.initial]
        views = [dict(allocation=v.allocation,offset=v.offset,bytes=v.bytes) for v in table.views]
        event_bytes = (json.dumps({"events": events, "chains": chains, "runtime_mapping_observed": False},
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
            "original_capture_executes_generated_code": False, "cached_issuer_invoked": False})
    finally:
        device.diskcache_get, device.diskcache_put, device.Compiler.compile_cached = cache_get, cache_put, cached
        compiler_cpu.LLVMCompiler.compile_to_obj, compiler_cpu.jit_loader, elf.elf_loader = compile_obj, loader, elf_loader
        CPUProgram.__init__ = program_init
        for name,delegate in setters.items(): setattr(llvm,name,delegate)
        os.close(journal)


def read_spec(m, value, limit=512*MIB):
    path = Path(value['path'])
    raw = m.read(path, limit)
    need(len(raw) == value['bytes'] and sha(raw) == value['sha256'], 'sealed input bytes/SHA')
    return raw


def original_replay(root, job, m, tapped):
    """A 후에만 real CPUProgram을 생성한다. frozen BINARY/runtime fallback은 거부한다."""
    worker_environment(root)
    from tinygrad import Device
    from tinygrad.engine import realize
    from tinygrad.runtime.ops_cpu import CPUProgram, CPUComputeQueue
    from tinygrad.helpers import mv_address
    from model_export.original import driving
    from model_export.pipeline import linear_calls
    from model_export.buffers import host_buffer, BufferTable
    from tinygrad.uop.ops import Ops
    from check_pipeline_reference import execute
    build, out = job/'build/model482', job/'proof/model482'
    request = m.unique_json(m.read(build/'triplet-request.json',4*MIB))
    admission = m.unique_json(m.read(out/'triplet-readonly.json',4*MIB))
    need(admission['source'] == os.environ['GITHUB_SHA'] and admission['strict_triplet_all'] is True
         and admission['host_isa_all'] is True and admission['complete_roster'] == len(request['programs']), 'A before runtime')
    need(sha(m.read(Path(request['model']['path']),128*MIB)) == PKL_SHA, 'frozen PKL identity')
    imports_before = import_seal(m,root,job)
    previous_init, previous_runtime, previous_exec = CPUProgram.__init__, realize.get_runtime, CPUComputeQueue._exec
    allowed_init = None
    mappings, events, runtimes, known = [], [], {}, {}
    lock = threading.Lock()
    def initialized(self, dev, name, image, *args, **kwargs):
        need(allowed_init is not None and (name,sha(image)) == allowed_init[:2], 'unbound/old PROGRAM construction')
        previous_init(self,dev,name,image,*args,**kwargs)
        address = ctypes.cast(self.fxn,ctypes.c_void_p).value
        need(address == mv_address(self.mem) and address % allowed_init[2] == 0
             and bytes(self.mem[:]) == image, 'actual mapped bytes/function/alignment before first execution')
        mappings.append({'name':name,'image_sha256':sha(image),'function_address':address,'alignment':allowed_init[2]})
    CPUProgram.__init__ = initialized
    try:
        # restore_inputs/compose만 허용; 이 동안 CPUProgram 생성은 위 sentinel이 거부한다.
        original = driving(Path(request['model']['path']),(1344,760))
        need([{'name':e.name,'start':e.start,'end':e.end} for e in original.pipeline.entrypoints]
             == m.unique_json(m.read(build/'entrypoints.json',4*MIB)),'original stage boundaries changed')
        source_rows = m.unique_json(m.read(build/'fresh-runtime.json',4*MIB))
        rows = {r['key']:r for r in source_rows}
        need(len(rows) == len(source_rows) == len(request['programs']) and not realize.runtime_cache, 'finite absent runtime cache')
        for row,program in zip(source_rows,request['programs'],strict=True):
            need(row['image']==program['image'] and row['source_sha256']==program['original']['source_sha256']
                 and row['alignment']==program['original']['image_alignment'],'runtime image bound to A admitted program')
        table = BufferTable()
        for tensors, base in ((original.bindings.inputs,True),(original.bindings.outputs,False)):
            for tensor in tensors.values(): table.read(host_buffer(tensor.uop.base.buffer if base else tensor.uop.buffer))
        selected = None
        for call in linear_calls(original.pipeline.jit.captured.linear):
            op = call.src[0]
            buffers = [host_buffer(x.buffer) for x in realize.resolve_params(call,())]
            if op.op is Ops.SLICE:
                need(len(buffers)==2 and buffers[0].base is buffers[1].base,'SLICE backing')
                table.view(buffers[0]); continue
            if op.op is Ops.COPY:
                table.read(buffers[1]); table.write(buffers[0])
                for b in buffers: table.view(b)
                continue
            need(op.op is Ops.PROGRAM and op.key.hex() in rows, 'runtime outside authenticated complete roster')
            row = rows[op.key.hex()]
            device = buffers[op.arg.globals[0]].device
            need(device == row['device'] and op.arg.function_name == row['name']
                 and sha(op.src[3].arg.encode()) == row['source_sha256']
                 and sha(op.src[4].arg) == row['old_binary_sha256'], 'frozen source/name/old BINARY binding')
            key = (op.key,device)
            if key not in runtimes:
                image = read_spec(m,row['image'],16*MIB)
                allowed_init = (row['name'],sha(image),row['alignment'])
                program = Device[device].runtime(op.arg.function_name,image,*op.arg.aux,runtimevars=op.arg.runtimevars,prg=op)
                allowed_init = None
                runtimes[key] = program
                known[id(program)] = (program,sha(image))
                realize.runtime_cache[key] = program
                if row['source_sha256'] == STORE_SOURCE:
                    need(selected is None,'ambiguous writer SOURCE')
                    selected = program
            for i in op.arg.ins: table.read(buffers[i])
            for i in op.arg.outs: table.write(buffers[i])
            for i in op.arg.globals: table.view(buffers[i])
        need(len(runtimes)==len(rows) and selected is not None,'complete runtime map before any call')
        initial = [{'bytes':len(raw),'sha256':sha(raw)} for raw in table.initial]
        need(initial == m.unique_json(m.read(build/'initial.json',4*MIB)), 'original initial allocation provenance changed')
        def runtime(device, ast, cache=True):
            key = (ast.key,device)
            need(key in runtimes and cache and realize.runtime_cache.get(key) is runtimes[key], 'unbound runtime/fallback/cache bypass')
            result = previous_runtime(device,ast,cache=cache)
            need(result is runtimes[key], 'real runtime delegate identity')
            return result
        realize.get_runtime = runtime
        buffer = original.bindings.outputs['model'].uop.buffer.ensure_allocated()
        need(buffer.offset==0 and buffer.nbytes==10304, 'writer output extent')
        address = buffer._buf.va_addr
        def observed(queue, tid, program, count, *args):
            need(id(program) in known and known[id(program)][0] is program, 'unbound executed program')
            is_writer = program is selected
            if is_writer:
                need(count==10 and len(args)==11 and args[0]==address and tid in (0,1), 'writer pointer/scalar/worker binding')
                need(sha(program.mem[:]) == known[id(program)][1], 'writer mapped image drift')
                before = ctypes.string_at(address+99*4,4).hex() if tid==0 else None
            previous_exec(queue,tid,program,count,*args)
            if is_writer:
                after = ctypes.string_at(address+99*4,4).hex() if tid==0 else None
                with lock:
                    need(len(events)<2,'writer callback coverage overflow')
                    events.append({'worker':tid,'index':99 if tid==0 else None,'before_le_hex':before,
                        'after_le_hex':after,'pointer_args':list(args[:count]),'scalar_args_before_core_override':list(args[count:]),
                        'image_sha256':known[id(program)][1],'function_address':ctypes.cast(program.fxn,ctypes.c_void_p).value})
        if tapped: CPUComputeQueue._exec = observed
        traffic = build/'traffic'
        sequence = m.unique_json(m.read(traffic/'sequence.json',4*MIB))
        need([s['entrypoint'] for s in sequence]==['prepare','prepare','policy'],'exact warmup/first policy sequence')
        destination = traffic/('original-tapped' if tapped else 'original')
        destination.mkdir(mode=0o700)
        stage_by_name = {s.name:s for s in original.stages}
        need(set(stage_by_name)=={'prepare','policy'},'stage roster')
        outputs = {}
        for index,step in enumerate(sequence):
            for name,filename in step['inputs'].items():
                need(Path(filename).name==filename and name in original.bindings.inputs,'finite flat input name')
                tensor = original.bindings.inputs[name]
                raw = m.read(traffic/filename,16*MIB)
                target = tensor.uop.base.buffer
                need(target.nbytes==len(raw),'whole input extent')
                target.copyin(memoryview(bytearray(raw)))
            execute(stage_by_name[step['entrypoint']])
            Device.default.synchronize()
            if index==2:
                need(set(step['outputs'])==set(original.bindings.outputs),'whole output roster')
                for name,tensor in original.bindings.outputs.items():
                    raw = bytes(host_buffer(tensor.uop.buffer).as_memoryview())
                    filename = step['outputs'][name]
                    need(Path(filename).name==filename,'finite output path')
                    write(destination/filename,raw)
                    if tapped: need(m.read(traffic/'original'/filename,16*MIB)==raw,'original whole-output plain/tapped drift')
                    outputs[name]={'bytes':len(raw),'sha256':sha(raw)}
        if tapped: need(sorted(e['worker'] for e in events)==[0,1], 'both actual worker callbacks')
        need(len(mappings)==len(rows),'actual mapped roster')
        save(out/('original-tapped.json' if tapped else 'original-plain.json'), {'source':os.environ['GITHUB_SHA'],
            'initial':initial,'outputs':outputs,'events':events,'mappings':mappings,
            'fresh_runtime_not_frozen_binary':True,'plain_tapped_all_outputs_equal':tapped,
            'writer_call_observed':tapped,'ssa_first_producer_observed':False,'historical_provider_proven':False})
        imports_after = import_seal(m,root,job)
        need(all(imports_after.get(k)==v for k,v in imports_before.items()),'original source/import/provider drift')
        save(out/('original-tapped-imports.json' if tapped else 'original-plain-imports.json'),imports_after)
    finally:
        CPUProgram.__init__, realize.get_runtime, CPUComputeQueue._exec = previous_init, previous_runtime, previous_exec
        # 프로세스는 일회용이다. 이 cache는 실제 fresh runtime만 담았으며 old BINARY를 실행하지 않았다.


def prepare_traffic(controller, bundle):
    """fixture 원문 입력만 선택한다. warmup transform 근거가 없으면 HOLD한다."""
    m,build,out = controller.m,controller.build,controller.out
    fixture=build/'fixture'
    observation=fixture/'observations'
    raw=m.read(observation/'manifest.json',4*MIB)
    need(sha(raw)==OBSERVATION_SHA,'historical observations manifest SHA')
    manifest=m.unique_json(raw)
    cases=[c for c in manifest['scenarios'] if c['resolution']==[1344,760] and c['mode']=='dual']
    need(len(cases)==1,'unique dual scenario')
    case=cases[0]; first=case['frames'][0]; warm=case['warmup']
    need(case['frame_count']==len(case['frames'])==12 and first['index']==0 and first['frame_id']==3
         and first['prepare_only'] is False and warm['frame_id']==2 and warm['prepare_only'] is True,'original first schedule')
    def original(name):
        need(isinstance(name,str) and Path(name).is_relative_to('1344-dual') and '..' not in Path(name).parts,'finite original relative path')
        data=m.read(observation/name,16*MIB)
        pin=manifest['files'][name]
        need(len(data)==pin['bytes'] and sha(data)==pin['sha256'],'original inventoried input')
        return data
    calibration=controller.root/'rust/crates/modeld/src/calibration.rs'
    need(m.regular(calibration)['sha256']==CALIBRATION_SHA,'known default calibration source binding')
    controller.holds.add(calibration,CALIBRATION_SHA)
    initial=m.unique_json(original(case['initial_state']))
    warm_state=m.unique_json(original(warm['state']))
    # manifest에 warmup transform 파일은 없다. 원래 daemon의 default zero calibration과
    # 최초 원문 transform/초기 calibration-unseen 상태를 동시에 확인한 유한 경계다.
    need(initial['scope']['live_calib_seen'] is False and warm_state['scope']['live_calib_seen'] is False
         and 'liveCalibration' not in first['topics'],'HOLD: warmup calibration input unavailable')
    matrices=[original(first['transforms'][k]) for k in ('main','extra')]
    need(all(raw==bytes(36) for raw in matrices),'HOLD: authenticated unseen default transform differs')
    traffic=build/'traffic'; traffic.mkdir(mode=0o700)
    sequence=[]
    for index in range(2):
        values={'frame':original(warm['image'] if index==0 else first['images']['main']),
                'big_frame':original(warm['image'] if index==0 else first['images']['extra']),
                'tfm':matrices[0],'big_tfm':matrices[1],
                'packed_npy_inputs':original(warm['packed'] if index==0 else first['packed'])}
        expected={'frame':2428928,'big_frame':2428928,'tfm':36,'big_tfm':36,'packed_npy_inputs':2096}
        need({k:len(v) for k,v in values.items()}==expected,'complete1344 input lengths')
        mapping={}
        for name,data in values.items():
            filename=f'{name}-{index}.bin'; write(traffic/filename,data); mapping[name]=filename
        sequence.append({'entrypoint':'prepare','inputs':mapping,'outputs':{}})
    graph=m.unique_json(m.read(bundle/'graph.json',4*MIB))
    names={row['name'] for row in graph['outputs']}
    need(names=={'model','warped','img_q','big_img_q','feat_q','desire_q'},'whole6 outputs')
    sequence.append({'entrypoint':'policy','inputs':{},'outputs':{name:f'{name}-0.bin' for name in sorted(names)}})
    save(traffic/'sequence.json',sequence)
    frozen=original(first['raw_prediction'])
    need(len(frozen)==10304 and sha(frozen)==FROZEN_RAW_SHA,'frozen whole raw prediction')
    write(traffic/'frozen-model.bin',frozen)
    save(out/'input-provenance.json',{'observation_manifest_sha256':OBSERVATION_SHA,'fixture_sha256':FIXTURE_SHA,
        'fixture_source':'e25a14386582858dae25c6daca5ced3bfb4bacab','current_source':os.environ['GITHUB_SHA'],
        'warmup_transform_origin':'authenticated calibration-unseen default zero + exact frame0 transform bytes',
        'calibration_source':spec_file(m,controller.root/'rust/crates/modeld/src/calibration.rs'),
        'raw_input':{str(p.relative_to(traffic)):spec_file(m,p) for p in m.walk(traffic) if p.is_file()},
        'expected_regenerated':False,'new_capture_replaces_expected':False})

def controller_type(base):
    class ReplayController(base.Controller):
        def command(self, argv, label, seconds, env=None):
            need(re.fullmatch(r'[a-z0-9-]{1,80}',label),'fixed command label')
            selected = self.env if env is None else env
            need(not any(k in selected for k in ('GH_TOKEN','GITHUB_TOKEN','ACTIONS_RUNTIME_TOKEN')), 'secret-free command environment')
            save(self.out/(label+'-environment.json'),dict(selected))
            return super().command(argv,label,seconds,env=env)

        def select_current(self, raw, package, crate, name, kind, source, tested, features):
            rows=[self.m.unique_json(line) for line in raw.splitlines()]
            need(rows and rows[-1]=={'reason':'build-finished','success':True}
                 and sum(r.get('reason')=='build-finished' for r in rows)==1,'Cargo eager terminal JSONL')
            seen,matches=set(),[]
            manifest=self.root/f'rust/crates/{crate}/Cargo.toml'
            for row in rows:
                need(row.get('reason') in ('compiler-artifact','compiler-message','build-script-executed','build-finished'),'unknown Cargo JSONL reason')
                if row['reason']!='compiler-artifact': continue
                key=json.dumps(row,sort_keys=True)
                need(key not in seen,'duplicate Cargo artifact');seen.add(key)
                target=row['target']
                if (target['name'],target['kind'],row['profile']['test']) != (name,[kind],tested): continue
                need(row['package_id']==f'path+file://{manifest.parent}#{package}@0.1.0'
                     and row['manifest_path']==str(manifest) and target['src_path']==str(self.root/source)
                     and set(row['features'])==set(features) and row['fresh'] is False
                     and row['profile']['opt_level']=='0' and row['profile']['debuginfo']==0
                     and row['profile']['debug_assertions'] is True and row['profile']['overflow_checks'] is True,
                     'exact fresh current Cargo identity/features/profile')
                need(isinstance(row['executable'],str) and row['executable'],'nonnull current executable')
                path=Path(row['executable'])
                need(path.is_absolute() and path.resolve(strict=True)==path and path.is_relative_to(self.job/'build/target'),'current private ELF path')
                self.holds.add(path,elf=True);matches.append(path)
            need(len(matches)==1,f'unique current target: {name}')
            return matches[0]

        def hold_tree(self, name, root):
            paths=list(self.m.walk(root))
            for path in paths:
                if path.is_file() and (name != 'fixture' or path.parent == root/'models'
                        or path in (root/'fixture.json',root/'observations/manifest.json')):
                    self.holds.add(path)
            # fixture 전체는 native Fixture/Original과 독립 snapshot 전후검사한다.
            # 2048 FD 상한은 유지하며 5 PKL/manifest 및 뒤에서 선택한 direct 입력을 hold한다.
            self.baselines[name]=(root,None,self.m.snapshot(root,paths))

        def run(self):
            self.sources()
            self.holds.add(self.root/BASE,BASE_SHA)
            self.holds.add(Path('/usr/bin/capnp'),elf=True)
            self.holds.add(Path('/usr/bin/g++'),elf=True)
            self.holds.add(self.root/'tools/ci/model_pr1344_replay_482.py',os.environ['MODEL482_PR1344_HELPER_SHA256'])
            self.env.update(GITHUB_WORKFLOW_REF=os.environ['GITHUB_WORKFLOW_REF'],
                MODEL482_PR1344_HELPER_SHA256=os.environ['MODEL482_PR1344_HELPER_SHA256'],
                PYTHONPATH=':'.join(map(str,(self.root,self.root/'tinygrad_repo',self.root/'rust/tools'))))
            target=self.job/'build/target'
            need(not target.exists() and not target.is_symlink(),'fresh target absent')
            cargo=self.sdk/'bin/cargo'
            manifest=self.root/'rust/Cargo.toml'
            # 모든 Cargo 단계에서 동일 SHA profile을 사용해 이미 hold한 bin 재링크를 막는다.
            # 모델/테스트 본체 opt0과 수치·CPU·시간·후검증 계약은 유지한다.
            hash_profiles=['--config','profile.dev.package.sha2.opt-level=3',
                           '--config','profile.test.package.sha2.opt-level=3']
            model_args=['--manifest-path',manifest,'--locked','-p','openpilot-model-runtime','--no-default-features',
                        '--features','cached-program-isa','--message-format=json',*hash_profiles]
            raw=self.command([cargo,'build',*model_args,'--bin','offline-llvm-guarded-add'],'current-model-bins',900)
            guarded=self.select_current(raw,'openpilot-model-runtime','model-runtime','offline-llvm-guarded-add','bin',
                'rust/crates/model-runtime/src/bin/offline-llvm-guarded-add.rs',False,['cached-program-isa','native-skip-miri'])
            raw=self.command([cargo,'test',*model_args,'--lib','--no-run'],'current-model-libtest',900)
            test=self.select_current(raw,'openpilot-model-runtime','model-runtime','openpilot_model_runtime','lib',
                'rust/crates/model-runtime/src/lib.rs',True,['cached-program-isa','native-skip-miri'])
            features=['current-catalog-export-native','driving-daemon-native']
            args=['--manifest-path',manifest,'--locked','-p','openpilot-dev-tools','--no-default-features',
                  '--features',','.join(features),'--message-format=json',*hash_profiles]
            raw=self.command([cargo,'build',*args,'--bin','stage-driving-daemon-fixture','--bin','export-current-driving-catalog'], 'current-devtools-bins',900)
            stager=self.select_current(raw,'openpilot-dev-tools','dev-tools','stage-driving-daemon-fixture','bin',
                'rust/crates/dev-tools/src/bin/stage_driving_daemon_fixture.rs',False,features)
            exporter=self.select_current(raw,'openpilot-dev-tools','dev-tools','export-current-driving-catalog','bin',
                'rust/crates/dev-tools/src/bin/export_current_driving_catalog.rs',False,features)
            raw=self.command([cargo,'test',*args,'--test','driving_daemon_retirement','--no-run'],'current-retirement-test',900)
            d1test=self.select_current(raw,'openpilot-dev-tools','dev-tools','driving_daemon_retirement','test',
                'rust/crates/dev-tools/tests/driving_daemon_retirement.rs',True,features)
            raw=self.command([cargo,'build','--manifest-path',manifest,'--locked','-p','openpilot-driving-modeld',
                              '--no-default-features','--message-format=json',*hash_profiles],'current-daemon',900)
            daemon=self.select_current(raw,'openpilot-driving-modeld','drivingd','openpilot-driving-modeld','bin',
                'rust/crates/drivingd/src/main.rs',False,[])
            self.command([cargo,'clippy',*model_args,'--lib','--tests','--','-D','warnings'],'current-model-clippy',900)
            self.command([cargo,'clippy',*args,'--test','driving_daemon_retirement','--','-D','warnings'],'current-retirement-clippy',900)
            isa=self.command([test,'cached_isa::instruction_isa::tests::','--nocapture',
                              '--test-threads=1','--color=never'],'current-isa-feature-tests',60)
            need(b'test result: ok. 11 passed; 0 failed; 0 ignored;' in isa,
                 'complete current cached ISA unit roster')
            registry=self.job/'cargo-home/registry'
            self.baselines['registry']=(registry,None,self.m.snapshot(registry,self.m.walk(registry)))
            for index,path in enumerate((guarded,test,stager,exporter,d1test,daemon,Path(os.environ['ORIGINAL_PYTHON']),
                                         Path(sys.executable),Path(os.environ['LLVM_PATH']),self.sdk/'bin/rustc',self.sdk/'bin/cargo',Path('/usr/bin/g++').resolve(strict=True))):
                self.dso(path,f'provider-dso-{index}')
            for index,path in enumerate(sorted((self.job/'venv').rglob('*.so*'))):
                need(path.is_file() and not path.is_symlink(),'regular Python extension')
                self.dso(path,f'python-dso-{index}')
            source_closure=['guarded_add.rs','bin/offline-llvm-guarded-add.rs','owned_outputs.rs','lib.rs','cached_code.rs',
                'bin/offline-cached-code.rs','cached_elf.rs','cached_isa.rs','cpu.rs','graph.rs','error.rs','entrypoint.rs','assets.rs',
                'buffer.rs','cpu/diagnostic_482_tests.rs']
            self.env.update(MODEL482_SOURCE_PATHS=json.dumps(['rust/crates/model-runtime/src/'+p for p in source_closure]
                +['rust/crates/model-runtime/Cargo.toml','rust/Cargo.toml','rust/Cargo.lock','rust/rust-toolchain.toml']),
                MODEL_LLVM_GUARDED_ADD_BINARY=str(guarded),MODEL_LLVM_GUARDED_ADD_SHA256=self.m.regular(guarded)['sha256'],
                MODEL482_NATIVE_TEST_SHA256=self.m.regular(self.root/'rust/crates/model-runtime/src/cpu/diagnostic_482_pr1344.rs')['sha256'])
            archive=self.root/'rust/crates/dev-tools/tests/fixtures/driving-daemon-original/observations.tar.xz'
            need(archive.stat().st_size==59663776 and self.m.regular(archive)['sha256']==ARCHIVE_SHA,'frozen fixture tar size/SHA')
            self.holds.add(archive,ARCHIVE_SHA)
            fixture=self.build/'fixture'
            self.command([stager,'--archive',archive,'--output',fixture,'--blob-sha256',ARCHIVE_SHA,'--blob-bytes','59663776','--compact-images'],'stage-original-fixture',180)
            need(self.m.regular(fixture/'fixture.json')['sha256']==FIXTURE_SHA,'fixed historical fixture manifest')
            need(self.m.regular(fixture/'models/driving_tinygrad.pkl')['sha256']==PKL_SHA,'original frozen model SHA')
            self.hold_tree('fixture',fixture)
            helper=self.root/'tools/ci/model_pr1344_replay_482.py'
            python=Path(os.environ['ORIGINAL_PYTHON'])
            self.command([python,'-I','-B',helper,'collect'],'fresh-original',900)
            request=self.build/'triplet-request.json'
            data=self.m.unique_json(self.m.read(request,4*MIB))
            for row in data['programs']:
                for key in ('source','object','image'): self.holds.add(Path(row[key]['path']),row[key]['sha256'])
            for name in ('triplet-request.json','fresh-runtime.json','initial.json','entrypoints.json','compile-events.json'): self.holds.add(self.build/name)
            self.env.update(MODEL482_REQUEST=str(request),MODEL482_REQUEST_SHA256=self.m.regular(request)['sha256'])
            save(self.out/'holds-before-runtime.json',self.holds.records())
            self.test(test,READONLY,'triplet-readonly',90)
            readonly=self.out/'triplet-readonly.json';self.holds.add(readonly)
            a=self.m.unique_json(self.m.read(readonly,4*MIB))
            need(a['strict_triplet_all'] is True and a['host_isa_all'] is True,'complete A before any model runtime')
            catalog=self.build/'current-catalog';catalog_receipt=self.build/'current-catalog.json'
            self.command([exporter,'--root',self.root,'--fixture',fixture,'--catalog',catalog,'--receipt',catalog_receipt,
                          '--fixture-sha256',FIXTURE_SHA,'--python',python],'export-current-catalog',900)
            self.hold_tree('current-catalog',catalog);self.holds.add(catalog_receipt)
            pointer=self.m.unique_json(self.m.read(catalog/'current.json'))
            generation=pointer['generation']
            need(re.fullmatch('[0-9a-f]{64}',generation),'current generation hash')
            bundle=catalog/generation/'driving-1344x760'
            names={'graph.json','weights.bin','kernels.so'}
            for program in data['programs']:
                name=f"kernel-{program['kernel']}.txt"
                need(self.m.regular(bundle/name)['sha256']==program['original']['source_sha256'],
                     'current native kernel sidecar matches complete frozen SOURCE roster')
                names.add(name)
            bundle_files={name:spec_file(self.m,bundle/name,512*MIB) for name in sorted(names)}
            self.dso(bundle/'kernels.so','same-catalog-bundle-dso')
            self.env.update(EXPECTED_SOURCE=os.environ['GITHUB_SHA'],DRIVING_FIXTURE_DIRECTORY=str(fixture),
                DRIVING_FIXTURE_MANIFEST_SHA256=FIXTURE_SHA,DRIVING_CURRENT_CATALOG=str(catalog),
                DRIVING_CURRENT_CATALOG_RECEIPT=str(catalog_receipt),DRIVING_DAEMON_NATIVE_BINARY=str(daemon),
                MODEL482_A_RECEIPT=str(readonly))
            d1request=self.build/'d1-request.json'
            save(d1request,{'schema':1,'source':os.environ['GITHUB_SHA'],'readonly_sha256':self.m.regular(readonly)['sha256'],
                'daemon_sha256':self.m.regular(daemon)['sha256'],
                'test_source_sha256':self.m.regular(self.root/'rust/crates/dev-tools/tests/driving_daemon_retirement/model_482_pr1344.rs')['sha256']})
            self.holds.add(d1request)
            self.env.update(MODEL482_D1_REQUEST=str(d1request),MODEL482_D1_REQUEST_SHA256=self.m.regular(d1request)['sha256'])
            self.test(d1test,D1,'dual-first-packet',180)
            d1=self.m.unique_json(self.m.read(self.out/'d1.json',4*MIB))
            need(d1['diagnostic_complete'] is True and d1['all_words']==2576,'D1 packet classification complete')
            self.holds.add(self.out/'d1.json')
            prepare_traffic(self,bundle)
            for name in ('initial.json','entrypoints.json'):
                self.m.copy_new(self.build/name,self.build/'traffic'/name,4*MIB)
            for path in self.m.walk(self.build/'traffic'):
                if path.is_file(): self.holds.add(path)
            self.command([python,'-I','-B',helper,'plain'],'original-plain',180)
            traffic=self.build/'traffic'
            def replay_request(name):
                path=self.build/name
                save(path,{'schema':1,'triplet_request':spec_file(self.m,request,4*MIB),'readonly':spec_file(self.m,readonly,4*MIB),
                    'traffic':str(traffic),'bundle':str(bundle),'bundle_files':bundle_files,
                    'files':{str(p.relative_to(traffic)):spec_file(self.m,p,16*MIB) for p in self.m.walk(traffic) if p.is_file()}})
                self.holds.add(path)
                return {'MODEL482_REPLAY_REQUEST':str(path),'MODEL482_REPLAY_REQUEST_SHA256':self.m.regular(path)['sha256']}
            plain=replay_request('native-plain-request.json')
            self.test(test,NATIVE,'native-plain',180,{**self.env,**plain,'MODEL482_REPLAY_MODE':'plain'})
            # plain에서 생긴 출력까지 봉인한 뒤 두 tap을 각각 fresh subprocess에서 실행한다.
            for path in self.m.walk(traffic):
                if path.is_file(): self.holds.add(path)
            self.command([python,'-I','-B',helper,'tapped'],'original-tapped',180)
            tap=replay_request('native-tapped-request.json')
            self.test(test,NATIVE,'native-tapped',180,{**self.env,**tap,'MODEL482_REPLAY_MODE':'tapped'})
            self.compare(d1,traffic)

        def compare(self,d1,traffic):
            original=self.m.unique_json(self.m.read(self.out/'original-tapped.json',4*MIB))
            native=self.m.unique_json(self.m.read(self.out/'native-tapped.json',4*MIB))
            need(original['plain_tapped_all_outputs_equal'] is True and native['plain_tapped_all_outputs_equal'] is True
                 and sorted(e['worker'] for e in original['events'])==[0,1]
                 and sorted(e['worker'] for e in native['events'])==[0,1], 'both whole-output parity and actual worker coverage')
            sequence=self.m.unique_json(self.m.read(traffic/'sequence.json'))
            mismatches=[]
            for name,filename in sequence[-1]['outputs'].items():
                a=self.m.read(traffic/'original'/filename,16*MIB);b=self.m.read(traffic/filename,16*MIB)
                need(self.m.read(traffic/'original-tapped'/filename,16*MIB)==a,'original tap parity')
                if a!=b: mismatches.append({'output':name,'original_bytes':len(a),'native_bytes':len(b),
                    'original_sha256':sha(a),'native_sha256':sha(b)})
            frozen=self.m.read(traffic/'frozen-model.bin');a=self.m.read(traffic/'original/model-0.bin');b=self.m.read(traffic/'model-0.bin')
            need(len(frozen)==len(a)==len(b)==10304,'whole2576 scope')
            words=[]
            for index in range(2576):
                values=[x[4*index:4*index+4] for x in (frozen,a,b)]
                if len(set(values))!=1: words.append({'index':index,'frozen_le_hex':values[0].hex(),
                    'current_original_le_hex':values[1].hex(),'current_native_le_hex':values[2].hex()})
            save(self.out/'comparison.json',{'diagnostic_complete':True,'source':os.environ['GITHUB_SHA'],
                'whole_words':2576,'differences':words,'all_outputs_original_native_equal':not mismatches,'output_mismatches':mismatches,
                'plain_tapped_bytes_equal':True,'d1_strict_pass':d1['strict_pass'],'d1_strict_failure_sticky':not d1['strict_pass'],
                'd1_packet_equals_direct_native':d1['raw_sha256']==sha(b),'pr1344_first_failure_reproduced':d1['differences']==[{'index':99,'actual_bits':'00000000','frozen_bits':'80000000'}],
                'writer_call_observed':True,'ssa_first_producer_observed':False,'cached_issuer_invoked':False,
                'cached_runtime_enabled':False,'full128_acceptance':False,'full_model_runtime_acceptance':False,
                'historical_half_snan5_resolved':False,'historical_provider_proven':False,'retirement_credit':0})
            need(d1['strict_pass'] is True,'D1 strict frozen failure remains sticky after diagnostic D2')
            need(not mismatches,'strict current-original/current-native whole output mismatch')
            need(d1['raw_sha256']==sha(b),'D1 packet/direct native initial input-state link unresolved')

        def retain(self):
            # baseline의 PKL/weights를 복제하지 않는다. D1 원문 packet와 A triplet/D2 raw는 보존한다.
            selected=set()
            for name in ('triplet-request.json','fresh-runtime.json','initial.json','compile-events.json',
                         'entrypoints.json','d1-request.json','current-catalog.json','native-plain-request.json','native-tapped-request.json'):
                if (self.build/name).is_file(): selected.add(self.build/name)
            for directory in ('triplets','traffic','d1'):
                if (self.build/directory).is_dir():
                    selected.update(p for p in self.m.walk(self.build/directory) if p.is_file())
            catalog=self.build/'current-catalog'
            if catalog.is_dir(): selected.update(p for p in self.m.walk(catalog) if p.is_file() and p.name!='weights.bin')
            total=sum(p.stat().st_size for p in selected)
            proof=sum(p.stat().st_size for p in self.m.walk(self.job/'proof') if p.is_file())
            need(total+proof<=512*MIB,'proof512MiB cap including D1/D2 retention')
            fs=os.statvfs(self.out);need(fs.f_bavail*fs.f_frsize>=total+512*MIB,'retention and finish reserve')
            destination=self.out/'retained';destination.mkdir(mode=0o700)
            records=[]
            for path in sorted(selected):
                need(path.is_relative_to(self.build) and stat.S_ISREG(path.lstat().st_mode),'bounded regular retention')
                target=destination/path.relative_to(self.build)
                target.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
                self.m.copy_new(path,target,512*MIB)
                records.append({'runtime_path':str(path),'artifact_path':str(target.relative_to(self.job/'proof')),**self.m.regular(target)})
            save(self.out/'retained-index.json',{'files':records,'bytes':total,'model_pkl_uploaded':False,'full_weights_uploaded':False})
    return ReplayController


def main():
    need(os.environ.get('GITHUB_WORKFLOW_REF')==WORKFLOW,'exact same-source PR1344 workflow required')
    root=Path(os.environ['DIAG_SOURCE'])
    base=load_base(root)
    # 검증한 base의 같은 source/owner/SDK/limits contract와 source-only 유한 adapter를 결합한다.
    for name in ('sha','write','save','spec_file','config','worker_environment','import_seal'):
        globals()[name]=getattr(base,name)
    root,job=base.trusted();m=base.common(root)
    path=root/'tools/ci/model_pr1344_replay_482.py'
    need(m.regular(path)['sha256']==os.environ['MODEL482_PR1344_HELPER_SHA256'],'current helper literal pin')
    def interrupted(number,_frame): raise InterruptedError(f'PR1344 signal{number}')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
    if len(sys.argv)==2:
        need(sys.argv[1] in ('collect','plain','tapped'),'unknown PR1344 worker mode')
        if sys.argv[1]=='collect': collect(root,job,m)
        else: original_replay(root,job,m,sys.argv[1]=='tapped')
        return
    need(len(sys.argv)==1 and sys.flags.isolated and sys.flags.no_site,'stdlib controller -I -B -S required')
    controller=controller_type(base)(root,job,m)
    error=None
    try: controller.run()
    except BaseException:
        error=traceback.format_exc();raise
    finally: controller.finish(error)


if __name__=='__main__': main()
