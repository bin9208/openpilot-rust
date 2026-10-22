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


def compile_kernel(operation: UOp, index: int, compiler: ClangCompiler | CPULLVMCompiler) -> NativeKernel:
    info = operation.arg
    source = operation.src[3].arg
    if not isinstance(source, str):
        raise ExportError("compiled program has no source")
    symbol = f"op_impl_{index}"
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
def _guarded_add_helper():
    binary = os.environ.get("MODEL_LLVM_GUARDED_ADD_BINARY")
    expected = os.environ.get("MODEL_LLVM_GUARDED_ADD_SHA256")
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
