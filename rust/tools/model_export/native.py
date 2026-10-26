from __future__ import annotations

import contextlib
import hashlib
import json
import os
import platform
import selectors
import signal
import stat
import subprocess
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

from tinygrad.runtime.support.compiler_cpu import ClangCompiler, CPULLVMCompiler
from tinygrad.uop.ops import UOp

from .schema import CompilationEvidence, ExportError, Kernel


@dataclass(frozen=True, slots=True)
class NativeKernel:
    signature: Kernel
    source: str
    object_code: bytes
    wrapper: str
    evidence: CompilationEvidence | None = None


def compile_kernel(operation: UOp, index: int, compiler: ClangCompiler | CPULLVMCompiler, *, fresh_in_cached: bool = False,
                   preserve_original_flags: bool = False) -> NativeKernel:
    info = operation.arg
    source = operation.src[3].arg
    if not isinstance(source, str):
        raise ExportError("compiled program has no source")
    if type(preserve_original_flags) is not bool:
        raise ExportError("native source policy must be explicit boolean metadata")
    symbol = f"op_fresh_impl_{index}" if fresh_in_cached else f"op_impl_{index}"
    original = f"void {info.function_name}(" if isinstance(compiler, ClangCompiler) else f"@{info.function_name}("
    renamed = f"void {symbol}(" if isinstance(compiler, ClangCompiler) else f"@{symbol}("
    if source.count(original) != 1:
        raise ExportError("unrecognized kernel declaration")
    body = source.replace(original, renamed)
    kernel = Kernel(len(info.globals), len(info.vars))
    arguments = [f"buffers[{i}]" for i in range(kernel.buffers)] + [f"scalars[{i}]" for i in range(kernel.scalars)]
    types = ["void *"] * kernel.buffers + ["int32_t"] * kernel.scalars
    wrapper = (f"extern void {symbol}({', '.join(types) or 'void'});\n" +
               f"void op_kernel_{index}(void **buffers, const int32_t *scalars) {{ {symbol}({', '.join(arguments)}); }}")
    evidence = None
    with _guarded_add_helper() as helper:
        if helper is not None:
            if not isinstance(compiler, CPULLVMCompiler):
                raise ExportError("guarded add requires CPU:LLVM")
            renamed_sha = hashlib.sha256(body.encode()).hexdigest()
            if preserve_original_flags:
                # Parity callers preserve the original arithmetic IR. The held
                # helper is still authenticated before/after actual compilation.
                if len(body.encode()) > _SOURCE_LIMIT:
                    raise ExportError("guarded add source exceeds bound")
                receipt = _canonical({
                    "version": 1, "policy": "preserve-original-flags", "symbol_rename_only": True,
                    "math_rewrite_applied": False, "helper_executed": False,
                    "original_source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                    "renamed_source_sha256": renamed_sha, "compiled_source_sha256": renamed_sha,
                    "original_source_bytes": len(source.encode()), "compiled_source_bytes": len(body.encode()),
                })
                evidence = CompilationEvidence(body, receipt, {
                    "helper": helper[1], "original_source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                    "renamed_source_sha256": renamed_sha, "source_policy": "preserve-original-flags",
                    "source_policy_receipt_sha256": hashlib.sha256(receipt).hexdigest(),
                })
            else:
                body, receipt = _guarded_add(body, helper[0])
                evidence = CompilationEvidence(body, receipt, {
                    "helper": helper[1], "original_source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                    "renamed_source_sha256": renamed_sha, "rewrite_receipt_sha256": hashlib.sha256(receipt).hexdigest(),
                })
        object_code = compiler.compile_to_obj(body)
    if evidence is not None:
        evidence.binding["object_sha256"] = hashlib.sha256(object_code).hexdigest()
    return NativeKernel(kernel, source, object_code, wrapper, evidence)


_SOURCE_LIMIT = 16 * 1024 * 1024
_OUTPUT_LIMIT = 32 * 1024 * 1024
_STDERR_LIMIT = 64 * 1024


def _identity(value: os.stat_result) -> list[int]:
    return [value.st_dev, value.st_ino, value.st_size, value.st_mode, value.st_mtime_ns, value.st_ctime_ns]


@contextlib.contextmanager
def _admitted_helper(binary, expected):
    if binary is None and expected is None:
        yield None
        return
    if not binary or not expected or len(expected) != 64 or any(c not in "0123456789abcdef" for c in expected):
        raise ExportError("guarded add requires paired explicit ELF path and SHA256")
    path = Path(binary)
    fd = None
    try:
        if not path.is_absolute() or path.resolve(strict=True) != path:
            raise ExportError("guarded add helper must have an absolute path without aliases")
        fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
        metadata = os.fstat(fd)
        identity = _identity(metadata)
        if not stat.S_ISREG(metadata.st_mode) or not 64 <= metadata.st_size <= 128 * 1024 * 1024 or not metadata.st_mode & 0o111:
            raise ExportError("guarded add helper must be a bounded executable regular ELF")
        header = os.pread(fd, 64, 0)
        machine = {"x86_64":62, "aarch64":183}.get(platform.machine())
        if (header[:7] != b"\x7fELF\x02\x01\x01" or int.from_bytes(header[16:18], "little") not in (2, 3)
                or machine is None or int.from_bytes(header[18:20], "little") != machine):
            raise ExportError("guarded add helper is not a native ELF")

        def verify():
            digest = hashlib.sha256()
            offset = 0
            while offset < metadata.st_size:
                chunk = os.pread(fd, min(65536, metadata.st_size - offset), offset)
                if not chunk:
                    raise ExportError("guarded add helper truncated")
                digest.update(chunk)
                offset += len(chunk)
            if (path.resolve(strict=True) != path or _identity(path.lstat()) != identity
                    or _identity(os.fstat(fd)) != identity or digest.hexdigest() != expected):
                raise ExportError("guarded add helper identity or hash changed")
        verify()
        try:
            yield fd, {"enabled":True, "path":str(path), "bytes":metadata.st_size, "sha256":expected, "identity":identity}
        finally:
            verify()
    except (OSError, ValueError) as error:
        raise ExportError("guarded add helper admission failed: " + str(error)) from error
    finally:
        if fd is not None:
            os.close(fd)


def _guarded_add_helper():
    return _admitted_helper(os.environ.get("MODEL_LLVM_GUARDED_ADD_BINARY"), os.environ.get("MODEL_LLVM_GUARDED_ADD_SHA256"))


def guarded_add_binding() -> dict:
    with _guarded_add_helper() as helper:
        return {"enabled":False} if helper is None else helper[1]


def _receipt_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ExportError("guarded add receipt contains duplicate keys")
        value[key] = item
    return value


def _validate_rewrite_records(records: list, source: bytes) -> None:
    names = ("function", "result_name", "condition_name", "input_name", "selected_value_name", "generated_sum_name")
    fields = set(names) | {"line", "zero_arm_when_true", "removed_flag", "retained_flags"}
    lines = source.count(b"\n") + 1
    for record in records:
        if (not isinstance(record, dict) or set(record) != fields
                or any(not isinstance(record[name], str) or not 1 <= len(record[name].encode("utf-8")) <= 1024 for name in names)
                or type(record["line"]) is not int or not 1 <= record["line"] <= lines
                or type(record["zero_arm_when_true"]) is not bool or record["removed_flag"] != "nsz"
                or not isinstance(record["retained_flags"], list)
                or any(not isinstance(flag, str) or flag not in ("arcp", "contract", "afn") for flag in record["retained_flags"])
                or len(set(record["retained_flags"])) != len(record["retained_flags"])):
            raise ExportError("guarded add rewrite record is invalid")


def _guarded_add(body: str, fd: int) -> tuple[str, bytes]:
    source = body.encode("utf-8")
    if len(source) > _SOURCE_LIMIT:
        raise ExportError("guarded add source exceeds bound")
    with tempfile.TemporaryDirectory(prefix="guarded-add-") as temporary:
        receipt_path = Path(temporary) / "receipt.json"
        child = None
        try:
            child = subprocess.Popen([f"/proc/self/fd/{fd}", "--receipt", str(receipt_path)], pass_fds=(fd,),
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     start_new_session=True, bufsize=0)
            output, errors = bytearray(), bytearray()
            deadline = time.monotonic() + 30
            offset = 0
            with selectors.DefaultSelector() as selector:
                for stream, event in ((child.stdin, selectors.EVENT_WRITE), (child.stdout, selectors.EVENT_READ),
                                      (child.stderr, selectors.EVENT_READ)):
                    os.set_blocking(stream.fileno(), False)
                    selector.register(stream, event)
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise ExportError("guarded add helper deadline exceeded")
                    if receipt_path.exists() and receipt_path.lstat().st_size > _OUTPUT_LIMIT:
                        raise ExportError("guarded add receipt exceeds bound")
                    for key, _ in selector.select(min(remaining, 0.1)):
                        stream = key.fileobj
                        if stream is child.stdin:
                            if offset < len(source):
                                try:
                                    offset += os.write(stream.fileno(), source[offset:offset + 65536])
                                except BlockingIOError:
                                    continue
                            if offset == len(source):
                                selector.unregister(stream)
                                stream.close()
                        else:
                            try:
                                chunk = os.read(stream.fileno(), 65536)
                            except BlockingIOError:
                                continue
                            if not chunk:
                                selector.unregister(stream)
                                stream.close()
                                continue
                            target, limit = (output, _OUTPUT_LIMIT) if stream is child.stdout else (errors, _STDERR_LIMIT)
                            if len(target) + len(chunk) > limit:
                                raise ExportError("guarded add helper output exceeds bound")
                            target.extend(chunk)
            # Keep the leader unreaped until owned-group cleanup. A reaped PID
            # could be reused before killpg, targeting an unrelated process.
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ExportError("guarded add helper deadline exceeded")
                exited = os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                if exited is not None:
                    break
                time.sleep(min(remaining, 0.01))
            if exited.si_code != os.CLD_EXITED or exited.si_status != 0:
                raise ExportError("guarded add helper failed: " + errors.decode("utf-8", errors="replace"))
            receipt_fd = os.open(receipt_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
            with os.fdopen(receipt_fd, "rb") as receipt_file:
                before = _identity(os.fstat(receipt_file.fileno()))
                if not stat.S_ISREG(before[3]) or before[2] > _OUTPUT_LIMIT:
                    raise ExportError("guarded add receipt is not bounded regular data")
                receipt = receipt_file.read(_OUTPUT_LIMIT + 1)
                if (len(receipt) > _OUTPUT_LIMIT or _identity(os.fstat(receipt_file.fileno())) != before
                        or _identity(receipt_path.lstat()) != before):
                    raise ExportError("guarded add receipt changed")
            text = bytes(output).decode("utf-8")
            value = json.loads(receipt.decode("utf-8"), object_pairs_hook=_receipt_object)
            if not isinstance(value, dict):
                raise ExportError("guarded add receipt must be an object")
            count = value.get("rewrite_count")
            if (set(value) != {"status", "original_source_sha256", "output_source_sha256", "input_bytes", "output_bytes", "rewrite_count", "rewrites"}
                    or value.get("original_source_sha256") != hashlib.sha256(source).hexdigest()
                    or value.get("output_source_sha256") != hashlib.sha256(output).hexdigest()
                    or type(value.get("input_bytes")) is not int or type(value.get("output_bytes")) is not int
                    or value.get("input_bytes") != len(source) or value.get("output_bytes") != len(output)
                    or type(count) is not int or not 0 <= count <= 16384
                    or not isinstance(value.get("rewrites"), list) or len(value["rewrites"]) != count
                    or value.get("status") != ("rewritten" if count else "unchanged")
                    or (count == 0 and output != source)):
                raise ExportError("guarded add source/receipt binding differs")
            _validate_rewrite_records(value["rewrites"], source)
            return text, receipt
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            raise ExportError("guarded add helper failed: " + str(error)) from error
        finally:
            if child is not None:
                # The owned group must not outlive an error or a closed stream.
                try:
                    # WNOWAIT also confirms this is still our child, including
                    # if the caller has configured automatic SIGCHLD reaping.
                    os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                except ChildProcessError:
                    pass
                else:
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                child.wait()
                for stream in (child.stdin, child.stdout, child.stderr):
                    if stream is not None:
                        stream.close()


def _canonical(value) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def _run_cached_issuer(fd: int, request: dict, directory: Path, *, certificate: bool) -> dict:
    """Only the admitted native issuer executes; complete input images stay inert."""
    directory.mkdir()
    request_path = directory / "request.json"
    raw = _canonical(request)
    if len(raw) > 4 * 1024 * 1024:
        raise ExportError("cached issuer request exceeds bound")
    request_path.write_bytes(raw)
    outputs = {"object":directory / "container.o", "isa-receipt":directory / "isa.json", "admission":directory / "admission.json"}
    if certificate:
        outputs["certificate"] = directory / "certificate.json"
    argv = [f"/proc/self/fd/{fd}", "--request", str(request_path), "--request-sha256", hashlib.sha256(raw).hexdigest()]
    for name, path in outputs.items():
        argv.extend(["--" + name, str(path)])
    child = None
    errors = bytearray()
    code = None
    try:
        child = subprocess.Popen(argv, pass_fds=(fd,), stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.PIPE, start_new_session=True, bufsize=0)
        deadline = time.monotonic() + 90
        with selectors.DefaultSelector() as selector:
            os.set_blocking(child.stderr.fileno(), False)
            selector.register(child.stderr, selectors.EVENT_READ)
            while selector.get_map():
                if time.monotonic() >= deadline:
                    raise ExportError("cached issuer deadline exceeded")
                for key, _ in selector.select(0.1):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                    else:
                        errors.extend(chunk)
                        if len(errors) > 65536:
                            raise ExportError("cached issuer stderr exceeds bound")
            while True:
                # Keep the leader unreaped until owned process-group cleanup;
                # Popen.poll/wait here would allow its numeric PID to be reused.
                result = os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOWAIT | os.WNOHANG)
                if result is not None:
                    code = result.si_status if result.si_code == os.CLD_EXITED else 128 + result.si_status
                    break
                if time.monotonic() >= deadline:
                    raise ExportError("cached issuer did not exit")
                time.sleep(0.01)
    finally:
        if child is not None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=5)
            child.stderr.close()
    (directory / "stderr.txt").write_bytes(errors)
    (directory / "status.json").write_bytes(_canonical({"exit":code, "request_sha256":hashlib.sha256(raw).hexdigest(),
                                                      "generated_code_executed":False, "libraries_loaded":False}))
    if code != 0:
        raise ExportError("cached issuer rejected complete program roster: " + errors.decode(errors="replace"))
    result = {"request":raw, "status":(directory / "status.json").read_bytes()}
    for name, path in outputs.items():
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 64 * 1024 * 1024:
            raise ExportError("cached issuer output regular-file/size guard")
        result[name] = path.read_bytes()
    if json.loads(result["admission"]) != request["admission"]:
        raise ExportError("cached issuer changed independently authenticated admission")
    return result


class CachedExport:
    """An explicit native preflight and per-original-program admission session."""
    def __init__(self, fd: int, binding: dict, request: dict, digest: str, temporary: Path):
        self.fd, self.binding, self.request, self.digest, self.temporary = fd, binding, request, digest, temporary
        self.count = 0
        if request.get("library") is not None or request.get("graph") is not None:
            raise ExportError("cached initial request must be preflight-only")
        current = request["admission"]["current"]
        if (current["cached_helper_sha256"] != binding["sha256"]
                or current["cached_helper_binding_sha256"] != hashlib.sha256(_canonical(binding)).hexdigest()
                or current["helper_binding_sha256"] != hashlib.sha256(_canonical(guarded_add_binding())).hexdigest()):
            raise ExportError("cached original/current helper binding differs")
        self.preflight = _run_cached_issuer(fd, request, temporary / "preflight", certificate=False)

    def for_model(self, jit):
        from .pipeline import linear_calls
        from tinygrad.uop.ops import Ops
        if jit.captured is None:
            raise ExportError("cached mode requires an original captured model")
        expected = self.request["admission"]["programs"]
        by_identity = {}
        for index, row in enumerate(expected):
            key = (row["source_sha256"], row["image_sha256"], tuple(row["global_slots"]), row["scalars"], row["workers"], row["core_id"])
            if key in by_identity:
                raise ExportError("ambiguous duplicate original PROGRAM identity")
            by_identity[key] = index
        operations = {}
        for call in linear_calls(jit.captured.linear):
            operation = call.src[0]
            if operation.op is not Ops.PROGRAM or operation in operations:
                continue
            info = operation.arg
            source, image = operation.src[3].arg, operation.src[4].arg
            if not isinstance(source, str) or not isinstance(image, bytes) or len(image) > 16 * 1024 * 1024:
                raise ExportError("cached original PROGRAM requires complete SOURCE/BINARY")
            key = (hashlib.sha256(source.encode()).hexdigest(), hashlib.sha256(image).hexdigest(), tuple(info.globals),
                   len(info.vars), info.global_size[0], info.runtimevars.get("core_id"))
            if key not in by_identity:
                raise ExportError("actual PROGRAM source/image/argument/launch ABI differs from independent admission")
            operations[operation] = by_identity[key]
        if len(operations) != len(expected) or set(operations.values()) != set(range(len(expected))):
            raise ExportError("actual original PROGRAM roster incomplete or aliased")
        return CachedModel(self, operations)

    def receipt(self) -> dict:
        return {"enabled":True, "helper":self.binding, "request_sha256":self.digest,
                "admission_sha256":hashlib.sha256(self.preflight["admission"]).hexdigest(),
                "complete_preflight":True, "generated_code_executed":False}


@dataclass(frozen=True, slots=True)
class CachedModel:
    session: CachedExport
    operations: dict

    def package(self, captured, directory: Path, *, final: bool = False):
        expected = self.session.request["admission"]["programs"]
        records = {row.original_index:row for row in captured.cached_programs}
        if set(records) != set(range(len(expected))):
            raise ExportError("cached capture omitted an original PROGRAM")
        source_files = {row["original_index"]:row for row in self.session.request["programs"]}
        files = []
        for original_index in range(len(expected)):
            row = records[original_index]
            source = directory / f"kernel-{row.kernel}.txt"
            image = directory / f"kernel-{row.kernel}.cached.bin"
            if not final:
                image.write_bytes(row.image)
            files.append({"original_index":original_index, "kernel":row.kernel, "source":str(source), "image":str(image),
                          "reconstruction_object":source_files[original_index]["reconstruction_object"]})
        request = self.session.request | {"programs":files, "library":str(directory / "kernels.so") if final else None,
                                          "graph":str(directory / "graph.json") if final else None}
        return _run_cached_issuer(self.session.fd, request, directory / ("cached-seal" if final else "cached-package"), certificate=final)


@contextlib.contextmanager
def cached_export_session():
    names = ("MODEL_CACHED_CODE_BINARY", "MODEL_CACHED_CODE_SHA256", "MODEL_CACHED_CODE_REQUEST", "MODEL_CACHED_CODE_REQUEST_SHA256")
    values = [os.environ.get(name) for name in names]
    if all(value is None for value in values):
        yield None
        return
    if not all(values):
        raise ExportError("cached mode requires explicit native issuer path/SHA and request path/SHA")
    binary, expected, name, digest = values
    if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
        raise ExportError("invalid cached request SHA256")
    path = Path(name)
    if not path.is_absolute() or path.resolve(strict=True) != path:
        raise ExportError("cached request must be an absolute alias-free path")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 4 * 1024 * 1024:
            raise ExportError("cached request regular-file/size guard")
        raw = os.pread(fd, metadata.st_size + 1, 0)
        def verify():
            if (_identity(os.fstat(fd)) != _identity(metadata) or _identity(path.lstat()) != _identity(metadata)
                    or path.resolve(strict=True) != path or len(raw) != metadata.st_size
                    or hashlib.sha256(os.pread(fd, metadata.st_size + 1, 0)).hexdigest() != digest):
                raise ExportError("cached request bytes or identity changed")
        verify()
        request = json.loads(raw, object_pairs_hook=_receipt_object)
        with _admitted_helper(binary, expected) as helper, tempfile.TemporaryDirectory(prefix="native-cached-preflight-") as temporary:
            session = CachedExport(helper[0], helper[1], request, digest, Path(temporary))
            try:
                yield session
            finally:
                verify()
    finally:
        os.close(fd)
