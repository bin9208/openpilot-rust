#!/usr/bin/env python3
"""#523 normal40 frozen 원문 + 현재 CLI/trace의 Actions 전용 한정 gate.

새 임시 Python controller는 pending이다. 원본 Python/옛 ELF/모델을 실행하지 않는다.
원본 provider 미기록, 현재 normal40 수용, full runtime/device 수용을 분리한다.
로컬 기능/build/CI UNRUN; 문법/source/diff만 검토한다.
"""
import ctypes
import encodings.cp437
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import resource
import shutil
import signal
import stat
import struct
import subprocess
import sys
import time
import tomllib
import traceback
import zipfile
import zlib

MIB = 1024**2
GIB = 1024**3
REPO = "bin9208/openpilot-rust"
FEATURES = "controlsd-fixture-native"
DEV_FEATURES = ["controlsd-fixture-native", "paramsd-fixture-native"]
# #550 SHA 전용 후보이며 normal40의 단독 해결 여부는 Actions 검증 전이다.
SHA2_PROFILE_ARGS = (
    "--config", "profile.dev.package.sha2.opt-level=3",
    "--config", "profile.test.package.sha2.opt-level=3",
)
REFERENCE_ID = "legacy-normal40-12456242812-v1"
NEGATIVE = "frozen::frozen_normal40_admission_rejects_mixed_origins_and_mutation"
HELD_NEGATIVE = "controller::frozen::tests::frozen_held_files_and_rosters_reject_mutation_aliases_and_replacement"
ACTUAL = "frozen::frozen_normal40_actual_cli_matches_legacy_reference"
ARTIFACT = 12456242812
RUN = 39844308544
JOB = 120055270738
HEAD = "adff0392e61608c9a576550be78dd7eefb8b9065"
CHECKOUT = "6f25f161aae5eb3f887df6d0147172e4f85d935c"
ZIP_SIZE = 9895310
ZIP_SHA = "956326c29facb48c10f3dfeba18507e29327c48919f2cf35840edbd26bfe570f"
LOG_SIZE = 157617
LOG_SHA = "6bbf9799511ee423f32bc41ac1fbd9e18579e96d8b4a27c241ef63ef1382e5fe"
MEMBERS = {
    "loop/input.json": (101555623, "9441af7c071e49daaebfcbd8153136ef48bf6f248b3a9c995a8224073c3a0a5b"),
    "loop/source.json": (63477957, "448d2519bd0e0b2580d3d6921e3fab13df0ca4a7f7a66e39e72325b2e2f582d3"),
    "loop/results.json": (114, "84fd54e4bba2d6cfd542fcd3d9db47a0cbc035e22661df4408f9f1b0f6827cb9"),
    "loop/process.log": (1757, "fd72ae04d3bd1c0b5a81c57bd6d8bf6388120fe3aa9af3b5b4165706e7f72e61"),
}
CORE = {
    "rust/crates/dev-tools/src/controlsd_check.rs": "b25d1edf82232694daf47109f312ee97eaba51336ab9e7c1535ce5ed3a1acd0f",
    "rust/crates/dev-tools/src/controlsd_check/frozen.rs": "4ba524fef3123c5d14782bd2deae63c6d6347c2476328830bc155fccf8c1009c",
    "rust/crates/dev-tools/tests/controlsd_check.rs": "9e6a3b491a5b00874fe357dbbc0aa8bb9e27bd7e3a115f15ba7d784bd287f973",
    "rust/crates/dev-tools/tests/controlsd_check/frozen.rs": "aa8c9160bcab82028491ad93f1f501fe5fa36a43b3a6572d03d38b4a239eff76",
    "rust/crates/dev-tools/src/controlsd_check/reference.rs": "848c72970f453729059488f30978f37df24b4f33fbe08386b6bcd7c5d1b6e4f8",
    "rust/crates/dev-tools/src/paramsd_estimator_check/numeric.rs": "1a0e80bd495f45ec940eedb75431b026a229b5988f3e067c289b1ea7d587672a",
    "rust/crates/dev-tools/src/paramsd_estimator_check/reference.rs": "b9ac033904538e315602a07ae6f5ade4a29607076e182e0fa6b3b524e778c4d2",
    "rust/crates/dev-tools/src/paramsd_estimator_check/identity.rs": "ad49f0ee94ebcb37f997971c29118fc861c939fe261ceb7af9f53279edfaa402",
    "rust/crates/control-policy/src/numerics.rs": "f6c1516cf7990bd05d13f7aeffa7a45ae051517c5273e77dab26b82584585f44",
    "rust/crates/controlsd/examples/controlsd_trace.rs": "c0fee266a3c293b428918595af6dffbee8ff3fc050b944eb896fc0bd3d63743b",
    "rust/tools/check_controlsd.py": "0ea4b8dd0108b8f96955c24febb4ea27206ddda0860d757aed2ca85bf4df2749",
    "rust/tools/controlsd_compare.py": "52c4b064257fc09c9ff85c38ddd0e70b07d9572ad9cb61e2a46dc5c5a7fac830",
    "rust/tools/controlsd_source.py": "f5a51d38c64197cc1f8786e8c10a3c5c0b54301deb8d958eae46cafcb321b7b0",
}

SDK_PINS = {
    "rustc": "103b60e1b1339968c1d74202ea1d45686037e82c4ea3e0569de24b18a1e6836a",
    "cargo": "77f14b761b02b47e6747473f556b3bc9f98f7e4525b7c3b8d74898ff816e4636",
    "rustfmt": "d1c6d8c0e2fae4fbb686d421415d2f471e409ade6babb3e491d9ac5d9c70c602",
    "clippy-driver": "2ff45d07196eec4f57a631103a877631d3579e86db2770bcfaa3b9d92a0b9d15",
}


def need(value, message):
    if not value:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def unique_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, f"duplicate JSON key: {key}")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def ident(info):
    return [getattr(info, "st_" + key) for key in
            ("dev", "ino", "mode", "uid", "gid", "size", "nlink", "mtime_ns", "ctime_ns")]


def digest(fd):
    value = hashlib.sha256()
    offset = 0
    while block := os.pread(fd, MIB, offset):
        value.update(block)
        offset += len(block)
        need(offset <= 2 * GIB, "held file exceeds 2 GiB")
    return value.hexdigest()


def regular(path, limit=2 * GIB):
    info = path.lstat()
    need(stat.S_ISREG(info.st_mode) and info.st_size <= limit, f"bounded regular file: {path}")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        need(ident(info) == ident(os.fstat(fd)), f"open changed: {path}")
        value = {"identity": ident(info), "sha256": digest(fd)}
        need(ident(info) == ident(os.fstat(fd)) == ident(path.lstat()), f"read changed: {path}")
        return value
    finally:
        os.close(fd)


def read(path, limit=16 * MIB):
    before = regular(path, limit)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        raw = os.pread(fd, limit + 1, 0)
        need(len(raw) <= limit and sha(raw) == before["sha256"]
             and ident(os.fstat(fd)) == before["identity"] == ident(path.lstat()), f"bounded read changed: {path}")
        return raw
    finally:
        os.close(fd)


def save(path, value):
    raw = (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()
    need(len(raw) <= 32 * MIB, "metadata file exceeds 32 MiB")
    write_new(path, raw)


def write_new(path, raw):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def copy_new(source, target, limit):
    before = regular(source, limit)
    fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        need(ident(os.fstat(fd)) == before["identity"], f"copy source changed: {source}")
        out = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o400)
        count = 0
        value = hashlib.sha256()
        with os.fdopen(out, "wb") as stream:
            while chunk := os.pread(fd, MIB, count):
                count += len(chunk)
                need(count <= limit, f"copy source exceeds bound: {source}")
                value.update(chunk)
                stream.write(chunk)
            stream.flush()
            os.fsync(stream.fileno())
        need(count == before["identity"][5] and value.hexdigest() == before["sha256"]
             and ident(os.fstat(fd)) == before["identity"] == ident(source.lstat()), f"copy changed: {source}")
        need(regular(target, limit)["sha256"] == before["sha256"], f"copy target differs: {target}")
    finally:
        os.close(fd)


def publish_complete(path, value):
    pending = path.with_name("complete.pending.json")
    save(pending, value)
    held = pending.lstat()
    published = False
    try:
        os.link(pending, path, follow_symlinks=False)
        published = True
        pending.unlink()
        fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    except BaseException:
        # 외부 파일은 제거하지 않는다. 이 호출이 게시한 inode만 rollback한다.
        if published:
            current = path.lstat()
            if (current.st_dev, current.st_ino) == (held.st_dev, held.st_ino):
                path.unlink()
        raise


def walk(root, mutable=False, missing=None):
    yield root
    def onerror(error):
        if mutable and isinstance(error, FileNotFoundError):
            if missing is not None:
                missing(Path(error.filename))
            return
        raise error
    for base, dirs, files in os.walk(root, followlinks=False, onerror=onerror):
        need(len(Path(base).relative_to(root).parts) <= 64, "directory depth exceeds 64")
        for name in sorted(dirs + files):
            yield Path(base) / name
        dirs[:] = sorted(name for name in dirs if not (Path(base) / name).is_symlink())


def snapshot(root, paths, sdk=False):
    result = {}
    total = 0
    for path in paths:
        need(path.is_relative_to(root) and len(result) < 65000, "snapshot root/roster bound")
        info = path.lstat()
        row = {"identity": ident(info)}
        if sdk:
            need(info.st_uid == os.getuid() and not info.st_mode & 0o022, f"SDK owner/shared write: {path}")
        if stat.S_ISREG(info.st_mode):
            total += info.st_size
            need(total <= 8 * GIB, "snapshot input exceeds 8 GiB")
            row.update(regular(path))
        elif stat.S_ISLNK(info.st_mode):
            row["symlink"] = os.readlink(path)
        else:
            need(stat.S_ISDIR(info.st_mode), f"special snapshot input: {path}")
        result[str(path.relative_to(root))] = row
    return result


class Holds:
    def __init__(self):
        self.files = {}

    def add(self, alias, expected=None, elf=False):
        alias = Path(alias)
        path = alias.resolve(strict=True)
        if path not in self.files:
            need(len(self.files) < 2048, "held provider count exceeds 2048")
            seal = regular(path)
            fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
            need(ident(os.fstat(fd)) == seal["identity"], f"held open changed: {path}")
            self.files[path] = {"fd": fd, "seal": seal, "aliases": set()}
        item = self.files[path]
        item["aliases"].add(alias)
        need(expected is None or item["seal"]["sha256"] == expected, f"pin differs: {alias}")
        if elf:
            header = os.pread(item["fd"], 64, 0)
            need(header[:7] == b"\x7fELF\x02\x01\x01" and len(header) == 64
                 and int.from_bytes(header[18:20], "little") == 62, f"x86_64 ELF required: {path}")
        return path

    def records(self):
        return {str(path): {**item["seal"], "aliases": sorted(map(str, item["aliases"]))}
                for path, item in sorted(self.files.items())}

    def verify(self):
        for path, item in self.files.items():
            need(ident(os.fstat(item["fd"])) == item["seal"]["identity"]
                 and digest(item["fd"]) == item["seal"]["sha256"]
                 and regular(path) == item["seal"], f"held file changed: {path}")
            need(all(alias.resolve(strict=True) == path for alias in item["aliases"]), "provider alias changed")

    def close(self):
        for item in self.files.values():
            os.close(item["fd"])


def proc_stat(pid):
    raw = Path(f"/proc/{pid}/stat").read_text()
    fields = raw[raw.rindex(")") + 2:].split()
    return {"pid": pid, "ppid": int(fields[1]), "pgid": int(fields[2]),
            "session": int(fields[3]), "start": int(fields[19]), "state": fields[0]}


class Children:
    """한 번에 한 소유 command만 시작한다. 별도 daemon PGID도 ancestry로 추적한다."""
    def __init__(self, journal):
        need(hasattr(os, "pidfd_open") and hasattr(signal, "pidfd_send_signal"), "Linux pidfd API required")
        libc = ctypes.CDLL(None, use_errno=True)
        libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
        libc.prctl.restype = ctypes.c_int
        need(libc.prctl(36, 1, 0, 0, 0) == 0, "PR_SET_CHILD_SUBREAPER failed")
        enabled = ctypes.c_int()
        need(libc.prctl(37, ctypes.addressof(enabled), 0, 0, 0) == 0 and enabled.value == 1,
             "PR_GET_CHILD_SUBREAPER differs")
        self.live = {}
        self.history = []
        self.process = None
        self.journal = os.open(journal, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC | os.O_NOFOLLOW, 0o600)

    def remember(self, row):
        need(len(self.history) < 40000, "child lifecycle record bound")
        self.history.append(row)
        raw = (json.dumps(row, sort_keys=True) + "\n").encode()
        need(os.write(self.journal, raw) == len(raw), "short lifecycle journal write")
        os.fsync(self.journal)

    def scan(self):
        rows = {}
        for path in Path("/proc").iterdir():
            if not path.name.isdigit():
                continue
            try:
                if path.stat().st_uid == os.getuid():
                    rows[int(path.name)] = proc_stat(int(path.name))
            except (FileNotFoundError, ProcessLookupError):
                continue
        selected = {pid for pid, row in rows.items() if row["ppid"] == os.getpid()}
        selected.update(pid for pid, old in self.live.items()
                        if pid in rows and rows[pid]["start"] == old["start"])
        while True:
            expanded = selected | {pid for pid, row in rows.items() if row["ppid"] in selected}
            if expanded == selected:
                break
            selected = expanded
        need(len(selected) <= 256, "owned descendant bound exceeded")
        for pid in list(self.live):
            if pid not in rows or rows[pid]["start"] != self.live[pid]["start"]:
                os.close(self.live.pop(pid)["fd"])
        for pid in selected:
            row = rows[pid]
            if pid not in self.live:
                fd = None
                try:
                    fd = os.pidfd_open(pid, 0)
                    need(proc_stat(pid)["start"] == row["start"], "PID reused while opening pidfd")
                except (FileNotFoundError, ProcessLookupError):
                    if fd is not None:
                        os.close(fd)
                    continue
                except BaseException:
                    if fd is not None:
                        os.close(fd)
                    raise
                self.live[pid] = {**row, "fd": fd}
                self.remember({"event": "observed", **row})
            else:
                need(self.live[pid]["start"] == row["start"], "owned PID reused")
                self.live[pid].update(row)
        return {pid: self.live[pid] for pid in selected if pid in self.live}

    def reap(self):
        self.scan()
        while True:
            try:
                pid, status = os.waitpid(-1, os.WNOHANG)
            except ChildProcessError:
                break
            if pid == 0:
                break
            need(pid in self.live, "untracked adopted child exit")
            self.remember({"event": "reaped", "pid": pid, "start": self.live[pid]["start"],
                           "returncode": os.waitstatus_to_exitcode(status)})
            if self.process is not None and pid == self.process.pid:
                self.process.returncode = os.waitstatus_to_exitcode(status)
            os.close(self.live.pop(pid)["fd"])

    def stop(self):
        old = {number: signal.signal(number, signal.SIG_IGN) for number in (signal.SIGTERM, signal.SIGINT)}
        try:
            for number, seconds in ((signal.SIGTERM, 3), (signal.SIGKILL, 7)):
                deadline = time.monotonic() + seconds
                while time.monotonic() < deadline:
                    self.reap()
                    rows = self.scan()
                    if not rows:
                        return
                    # pidfd는 개별 identity를 유지한다. 별도 PGID의 daemon도 빠뜨리지 않는다.
                    for item in rows.values():
                        try:
                            signal.pidfd_send_signal(item["fd"], number)
                        except ProcessLookupError:
                            pass
                    time.sleep(0.02)
            self.reap()
            need(not self.scan(), "owned descendants survived SIGKILL/reap; preserve all owned evidence")
        finally:
            for number, handler in old.items():
                signal.signal(number, handler)


class BudgetTransition(Exception):
    """알려진 mutable namespace의 완료되지 않은 표본: 전체 재표본만 허용한다."""


class Probe:
    def __init__(self):
        expected = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": REPO, "GITHUB_EVENT_NAME": "push",
                    "GITHUB_REF": "refs/heads/codex/feat-523-python-rust",
                    "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64"}
        need(all(os.environ.get(key) == value for key, value in expected.items()) and os.uname().machine == "x86_64",
             "GitHub-hosted Actions Linux/X64 identity required")
        need(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "isolated stdlib-only diagnostic Python required")
        self.source = os.environ["GITHUB_SHA"]
        need(re.fullmatch(r"[0-9a-f]{40}", self.source), "source SHA required")
        self.root = Path(os.environ["DIAG_SOURCE"])
        self.job = Path(os.environ["CI_JOB_TEMP"])
        self.rustup = self.job / "rustup-home"
        self.sdk_root = Path(os.environ["CI_PREPARED_RUST_ROOT"])
        self.sdk_manifest = self.job / "proof/prepared-rust-files.before.sha256"
        need(self.root == Path(os.environ["GITHUB_WORKSPACE"]) == Path.cwd()
             and self.root.parent == Path(os.environ["DIAG_SOURCE_OWNER"])
             and self.root.parent.parent == Path(os.environ["DIAG_ACTIONS_WORKSPACE"]), "private source binding differs")
        need(re.fullmatch(r"\.diag296-source-" + re.escape(os.environ["GITHUB_RUN_ID"]) + "-"
             + re.escape(os.environ["GITHUB_RUN_ATTEMPT"]) + r"\.[A-Za-z0-9]{6}", self.root.parent.name)
             and self.root.name == "source", "private source owner path differs")
        need(":".join(map(str, (self.root.parent.stat().st_dev, self.root.parent.stat().st_ino)))
             == os.environ["DIAG_SOURCE_IDENTITY"], "private source owner identity differs")
        need(Path(os.environ["RUSTUP_HOME"]) == self.rustup and self.sdk_root == self.rustup / "toolchains/1.94.0-x86_64-unknown-linux-gnu"
             and self.sdk_root.resolve(strict=True) == self.sdk_root
             and os.environ["CI_PREPARED_RUST_MANIFEST"] == str(self.sdk_manifest)
             and os.environ["RUSTC"] == str(self.sdk_root / "bin/rustc")
             and os.environ["RUSTDOC"] == str(self.sdk_root / "bin/rustdoc"), "owned prepared SDK binding differs")
        self.target = self.root / "rust/target"
        self.out = self.job / "proof/controls-normal40"
        owner = ":".join(os.environ[key] for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB"))
        for path in (self.root, self.root.parent, self.job, self.rustup, self.out.parent):
            info = path.lstat()
            need(path.is_absolute() and path.resolve(strict=True) == path and stat.S_ISDIR(info.st_mode)
                 and info.st_uid == os.getuid(), f"owned canonical directory required: {path}")
        need(read(self.job / "owner", 512).decode().strip() == owner
             == read(self.root.parent / ".diag296-owner", 512).decode().strip(), "owner marker differs")
        need(Path(os.environ["CARGO_TARGET_DIR"]) == self.target, "private target differs")
        self.out.mkdir(mode=0o700)
        self.out_identity = ident(self.out.lstat())[:5]
        self.native_root_fd = None
        self.native_root_identity = None
        self.native_armed = False
        self.native_publication = None
        self.budget_resamples = []
        self.tmp = self.job / "normal40-tmp"
        self.tmp.mkdir(mode=0o700)
        self.holds = Holds()
        self.children = Children(self.out / "children-events.jsonl")
        self.commands = []
        self.tools = {}
        self.phase = "admission"
        self.source_before = self.sdk_before = self.numerics_before = None
        self.deadline = time.monotonic() + 52 * 60
        self.last_growth = 0
        self.reference_before = None
        self.native_before = None
        self.numerics = self.job / "cache/normal40-numerics"
        self.staged = self.job / "cache/normal40-staged"
        self.staged_before = None
        self.observed_runtime = set()
        self.runtime_pid_maps = {}
        self._runtime_sample = None
        self.runtime = {}
        self.build_started = False
        self.finalizing = False
        self.header_before = {}
        self.env = {"RUSTUP_HOME": str(self.rustup), "PATH": f"{self.sdk_root}/bin:/usr/bin:/bin", "HOME": str(self.job / "home"),
                    "TMPDIR": str(self.tmp), "CARGO_HOME": str(self.job / "cargo-home"),
                    "CARGO_TARGET_DIR": str(self.target), "RUSTC": str(self.sdk_root / "bin/rustc"),
                    "RUSTDOC": str(self.sdk_root / "bin/rustdoc"), "RUSTUP_TOOLCHAIN": "1.94.0",
                    "CARGO_BUILD_JOBS": "2", "CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0",
                    "CARGO_PROFILE_TEST_DEBUG": "0", "CARGO_TERM_COLOR": "never", "LC_ALL": "C", "TZ": "UTC"}
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        need(hard == resource.RLIM_INFINITY or hard >= 4096, "at least 4096 FDs required")
        resource.setrlimit(resource.RLIMIT_NOFILE, (max(soft, 4096), hard))


    def command(self, argv, label, seconds=60, env=None, limit=32 * MIB, allowed=None, trace=None):
        need(not self.children.scan(), "unreaped child before command")
        argv = list(map(str, argv))
        before = time.monotonic()
        observed = set()
        error = None
        process = None
        sample = None
        if not self.finalizing:
            self.budget(force=True)
        with (self.out / f"{label}.stdout").open("xb") as stdout, (self.out / f"{label}.stderr").open("xb") as stderr:
            try:
                process = subprocess.Popen(argv, cwd=self.root, env=env or self.env, start_new_session=True,
                                           stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                           preexec_fn=lambda: self.child_limits(allowed is not None))
                self.children.process = process
                if allowed is not None:
                    sample = self._start_runtime_sample(allowed, label, min(before + seconds, self.deadline), observed)
                while True:
                    if allowed is None:
                        rows = self.children.scan()
                        observed.update(self.maps(rows, allowed))
                    else:
                        self._runtime_heartbeat(force=True)
                    self.children.reap()
                    need(time.monotonic() <= min(before + seconds, self.deadline), f"command deadline: {label}")
                    need(stdout.tell() <= limit and stderr.tell() <= limit, f"command log bound: {label}")
                    if not self.finalizing:
                        self.budget()
                    if sample is not None:
                        need(time.monotonic() <= min(before + seconds, self.deadline), f"command deadline: {label}")
                    if process.returncode is not None:
                        need(not self.children.scan(), f"live adopted/escaped-group child after {label}")
                        need(process.returncode == 0, f"{label}: exit {process.returncode}; preserve stderr")
                        break
                    time.sleep(0.02)
            except BaseException as problem:
                error = str(problem)
                raise
            finally:
                self._runtime_sample = None
                self.children.stop()
                self.commands.append({"label": label, "argv": argv,
                    "returncode": process.returncode if process else None, "error": error,
                    "elapsed_seconds": time.monotonic() - before, "sampled_executable_mappings": sorted(observed),
                    "mapping_scope": ("20ms loop and budget-entry heartbeat; blocking/scheduling gaps remain; not complete/pre-dlopen observation"
                                      if sample is not None else "20ms loop samples plus hashing overhead; not complete/pre-dlopen observation"),
                    **({"mapping_heartbeat": self._runtime_sample_record(sample)} if sample is not None else {}),
                    "all_owned_children_reaped": not self.children.scan()})
                self.children.process = None
        if not self.finalizing:
            self.budget(force=True)
        return read(self.out / f"{label}.stdout", limit)


    def _start_runtime_sample(self, allowed, label, deadline, observed):
        need(self._runtime_sample is None and not self.finalizing, "runtime sampling already active or finalizing")
        now = time.monotonic()
        need(now <= deadline, f"command deadline: {label}")
        self._runtime_sample = {"allowed": allowed, "label": label, "deadline": deadline,
            "observed": observed, "started": now, "last": None, "samples": 0, "budget_samples": 0,
            "max_interval_seconds": 0, "max_sample_seconds": 0, "first_mappings": {}, "record_bytes": 0}
        return self._runtime_sample

    def _runtime_heartbeat(self, force=False, budget=False):
        sample = self._runtime_sample
        if sample is None:
            return
        now = time.monotonic()
        if not force and sample["last"] is not None and now - sample["last"] < 0.02:
            return
        need(now <= sample["deadline"], f"command deadline: {sample['label']}")
        previous = sample["last"] if sample["last"] is not None else sample["started"]
        sample["max_interval_seconds"] = max(sample["max_interval_seconds"], now - previous)
        sample["last"] = now
        rows = self.children.scan()
        sample["observed"].update(self.maps(rows, sample["allowed"]))
        after = time.monotonic()
        sample["max_sample_seconds"] = max(sample["max_sample_seconds"], after - now)
        need(after <= sample["deadline"], f"command deadline: {sample['label']}")
        sample["samples"] += 1
        sample["budget_samples"] += int(budget)

    @staticmethod
    def _runtime_sample_record(sample):
        result = {"period_seconds": 0.02, **{key: sample[key] for key in
            ("samples", "budget_samples", "max_interval_seconds", "max_sample_seconds")},
            "first_mappings": list(sample["first_mappings"].values()),
            "hard_interval_guarantee": False, "complete_mapping_closure": False}
        need(len(json.dumps(result, sort_keys=True, indent=2).encode()) <= MIB,
             "runtime mapping observation exceeds 1 MiB")
        return result

    def _require_runtime_mappings(self, pid, trace, numerical_library):
        mapped = self.runtime_pid_maps.get(pid, {}).get("files", set())
        need(str(trace) in mapped and str(numerical_library) in mapped,
             "actual trace and numerical library were not observed mapped by its PID")


    def git(self, args, label):
        return self.command([self.tools["git"], "-C", self.root, *args], label)


    def sources(self, label):
        need(self.git(["rev-parse", "HEAD"], f"head-{label}").decode().strip() == self.source, "HEAD differs")
        self.git(["diff", "--exit-code", "HEAD", "--"], f"diff-{label}")
        raw = self.git(["ls-files", "--stage", "-z"], f"index-{label}")
        paths = []
        for entry in raw.split(b"\0"):
            if not entry:
                continue
            meta, name = entry.split(b"\t", 1)
            mode, _, stage = meta.decode().split()
            relative = PurePosixPath(os.fsdecode(name))
            need(stage == "0" and mode != "160000" and not relative.is_absolute()
                 and ".." not in relative.parts, "unsafe/unmerged/submodule index")
            paths.append(self.root / str(relative))
        result = {"head": self.source, "index_sha256": sha(raw), "files": snapshot(self.root, paths)}
        save(self.out / f"source-{label}.json", result)
        return result


    def sdk(self, label):
        need(self.sdk_root.resolve(strict=True) == self.sdk_root, "SDK root alias")
        value = snapshot(self.sdk_root, walk(self.sdk_root), sdk=True)
        save(self.out / f"sdk-{label}.json", value)
        return value


    def dso(self, program, label):
        # ldd의 실패 문구를 예외 처리하지 않는다. hold한 ELF의 실제 program header로만
        # interpreter/dynamic table이 없는 static 실행 파일의 empty closure를 인정한다.
        program = Path(program).resolve(strict=True)
        held = self.holds.files[program]
        header = os.pread(held["fd"], 64, 0)
        need(len(header) == 64 and header[:7] == b"\x7fELF\x02\x01\x01", "provider ELF64 little-endian header differs")
        fields = struct.unpack("<16sHHIQQQIHHHHHH", header)
        _, kind, machine, version, entry, phoff, _, _, ehsize, phsize, phcount, _, _, _ = fields
        size = os.fstat(held["fd"]).st_size
        need(kind in (2, 3) and machine == 62 and version == 1 and ehsize == 64
             and phsize == 56 and 0 < phcount <= 128 and phoff >= 64
             and phoff + phsize * phcount <= size, "provider ELF program table differs")
        table = os.pread(held["fd"], phsize * phcount, phoff)
        need(len(table) == phsize * phcount, "provider ELF program table truncated")
        segments = [struct.unpack_from("<IIQQQQQQ", table, index * phsize) for index in range(phcount)]
        need(all(offset <= size and length <= size - offset for _, _, offset, _, _, length, _, _ in segments),
             "provider ELF segment outside held file")
        executable_loads = [segment for segment in segments if segment[0] == 1 and segment[1] & 1]
        need(executable_loads and all(segment[5] <= segment[6] for segment in segments if segment[0] == 1),
             "provider ELF executable LOAD missing or malformed")
        types = {segment[0] for segment in segments}
        if 2 not in types and 3 not in types:
            need(entry != 0 and any(segment[3] <= entry < segment[3] + segment[6] for segment in executable_loads),
                 "static provider ELF entry outside executable LOAD")
            save(self.out / f"{label}.static-elf.json", {"program": str(program), "elf_type": kind,
                 "machine": machine, "program_header_count": phcount, "has_pt_interp": False,
                 "has_pt_dynamic": False, "dso_paths": [], "closure_kind": "held-static-elf"})
            return set()
        raw = self.command([self.tools["ldd"], program], label, limit=MIB).decode()
        paths = set()
        for line in raw.splitlines():
            match = re.fullmatch(r"\s*(?:\S+\s+=>\s+)?(/\S+)\s+\(0x[0-9a-fA-F]+\)\s*", line)
            if match:
                path = Path(match[1]).resolve(strict=True)
                need(any(path.is_relative_to(root) for root in (Path("/usr/lib"), Path("/lib").resolve(), self.sdk_root, self.target, self.numerics)),
                     "DSO outside system/SDK/current target")
                paths.add(self.holds.add(Path(match[1]), elf=True))
            else:
                need(re.fullmatch(r"\s*linux-vdso\.so\.\d+\s+\(0x[0-9a-fA-F]+\)\s*", line), "unresolved/opaque ldd output")
        need(paths, "empty dynamic provider closure")
        return paths


    def providers(self):
        helpers = set()
        for name in ("bash", "sh", "ranlib", "nm", "git", "gh", "curl", "clang", "clang++", "cc", "c++", "ar", "as", "ld", "pkg-config", "capnp", "ldd", "strace", "timeout"):
            path = shutil.which(name, path=self.env["PATH"])
            need(path, f"prepared provider missing: {name}; no installation/fallback")
            self.tools[name] = self.holds.add(path)
        for name, pin in SDK_PINS.items():
            self.holds.add(self.sdk_root / "bin" / name, pin, elf=True)
        for name in ("rustdoc", "cargo-clippy"):
            self.holds.add(self.sdk_root / "bin" / name, elf=True)
        for name in ("prepared-rust-files.before.sha256", "prepared-rust-roster.before.bin", "prepared-rust-programs.sha256"):
            self.holds.add(self.job / "proof" / name)
        self.holds.add(Path("/etc/ld.so.cache"))
        self.holds.add(Path(sys.executable), elf=True)
        for module in tuple(sys.modules.values()):
            name = getattr(module, "__file__", None)
            if name and Path(name).is_file():
                self.holds.add(name)
        for line in Path("/proc/self/maps").read_text().splitlines():
            fields = line.split(None, 5)
            if len(fields) == 6 and fields[5].startswith("/"):
                self.holds.add(Path(fields[5]))
        self.env.update(CC=str(self.tools["cc"]), CXX=str(self.tools["c++"]), AR=str(self.tools["ar"]))
        for driver, names in (("cc", ("cc1", "collect2")), ("c++", ("cc1plus",))):
            for name in names:
                raw = self.command([self.tools[driver], "-print-prog-name=" + name], f"provider-{name}").decode().strip()
                path = Path(raw) if raw.startswith("/") else Path(shutil.which(raw, path=self.env["PATH"]) or raw)
                need(path.is_absolute(), "compiler helper provider unresolved")
                self.holds.add(path, elf=True)
                helpers.add(path.resolve())
                if name == "cc1":
                    self.header_before[path.resolve().parent] = None
        include = self.command([self.tools["clang"], "-print-resource-dir"], "clang-resource-dir").decode().strip()
        need(Path(include).is_absolute() and Path(include).resolve(strict=True) == Path(include), "clang resource root differs")
        self.header_before[Path(include)] = None
        self.header_before[Path("/usr/include")] = None
        for index, path in enumerate(self.header_before):
            self.header_before[path] = snapshot(path, walk(path))
            save(self.out / f"system-sdk-{index}-before.json", {"root": str(path), "files": self.header_before[path]})
        version = self.command([self.sdk_root / "bin/rustc", "-vV"], "rustc-version").decode()
        need("release: 1.94.0\n" in version and "host: x86_64-unknown-linux-gnu\n" in version, "Rust version/host differs")
        need("GNU coreutils" in self.command([self.tools["timeout"], "--version"], "timeout-version").decode(),
             "GNU timeout provider required")
        programs = set(self.tools.values()) | helpers | {self.sdk_root / "bin" / name for name in
                   [*SDK_PINS, "rustdoc", "cargo-clippy"]} | {Path(sys.executable).resolve()}
        for index, path in enumerate(sorted(programs)):
            if os.pread(self.holds.files[path]["fd"], 4, 0) == b"\x7fELF":
                self.dso(path, f"provider-dso-{index}")
        save(self.out / "diagnostic-python.json", {"version": sys.version, "executable": sys.executable,
             "stdlib_only": True, "binding_starts_after_interpreter_stdlib_imports": True,
             "original_python_executions": 0})

    @staticmethod
    def child_limits(_runtime):
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        resource.setrlimit(resource.RLIMIT_FSIZE, (512 * MIB, 512 * MIB))

    def native_root(self):
        root = self.out / "native"
        need(self.native_armed and self.phase == "actual-normal40" and not self.finalizing,
             "receipt publication is outside the one fresh actual command")
        need(ident(self.out.lstat())[:5] == self.out_identity and self.out.resolve(strict=True) == self.out,
             "owned evidence parent changed")
        before = root.lstat()
        need(stat.S_ISDIR(before.st_mode) and before.st_uid == os.getuid() and not before.st_mode & 0o077
             and root.resolve(strict=True) == root, "fresh native evidence root is not private/canonical")
        if self.native_root_fd is None:
            self.native_root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
            self.native_root_identity = ident(before)[:5]
        need(ident(before)[:5] == self.native_root_identity == ident(os.fstat(self.native_root_fd))[:5]
             == ident(root.lstat())[:5], "owned native root identity changed")
        return root

    def publication_pair(self, path, sampled):
        native = self.out / "native"
        parts = path.relative_to(native).parts if path.is_relative_to(native) else ()
        need(parts == ("results.json",) or (len(parts) == 2 and
             re.fullmatch(r"owned-trace-[A-Za-z0-9]{6}", parts[0]) and parts[1] == "results.pending.json"),
             "unrelated proof hardlink path")
        root = self.native_root()
        dirs = [child for child in root.iterdir() if re.fullmatch(r"owned-trace-[A-Za-z0-9]{6}", child.name)]
        if not dirs:
            raise BudgetTransition("owned trace directory disappeared during publication")
        need(len(dirs) == 1, "receipt publication needs exactly one owned trace directory")
        private = dirs[0]
        try:
            directory = private.lstat()
            canonical_private = private.resolve(strict=True)
        except FileNotFoundError as error:
            raise BudgetTransition("owned trace directory disappeared before receipt read") from error
        need(stat.S_ISDIR(directory.st_mode) and directory.st_uid == os.getuid()
             and not directory.st_mode & 0o077 and canonical_private == private,
             "receipt pending directory is not private/canonical")
        pair = (private / "results.pending.json", root / "results.json")
        need(path in pair and sampled.st_nlink == 2, "unrelated proof hardlink")
        fds = []
        try:
            for item in pair:
                fds.append(os.open(item, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC))
            infos = [os.fstat(fd) for fd in fds]
            expected = (sampled.st_dev, sampled.st_ino)
            for info in infos:
                need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                     and (info.st_dev, info.st_ino) == expected and info.st_size <= MIB,
                     "receipt pair kind/owner/inode/size differs")
                if info.st_nlink == 1:
                    raise BudgetTransition("receipt pending link was removed")
                need(info.st_nlink == 2, "receipt pair has an additional alias")
            raw = os.pread(fds[0], MIB + 1, 0)
            need(len(raw) == infos[0].st_size and raw == os.pread(fds[1], MIB + 1, 0),
                 "receipt pair byte/length differs")
            result = unique_json(raw)
            need(set(result) == {"pass", "cases", "frames", "numeric_comparisons", "max_absolute_error"}
                 and result["pass"] is True and result["cases"] == 40 and result["frames"] == 8655
                 and result["numeric_comparisons"] == 705948
                 and type(result["max_absolute_error"]) in (int, float)
                 and 0 <= result["max_absolute_error"] < float("inf"), "incomplete/invalid published result")
            def cleanup_data(after, before, fd):
                if ident(after) == ident(before):
                    return False
                need(ident(after)[:6] == ident(before)[:6] and after.st_mtime_ns == before.st_mtime_ns
                     and before.st_nlink == 2 and after.st_nlink in (1, 2), "receipt pair identity/data changed")
                need(os.pread(fd, MIB + 1, 0) == raw, "receipt bytes changed during link cleanup")
                return True
            cleanup = False
            for item, fd, before in zip(pair, fds, infos):
                cleanup |= cleanup_data(os.fstat(fd), before, fd)
                # FD 검사 직후 unlink가 일어나 pathname의 nlink/ctime만 바뀔 수도 있다.
                cleanup |= cleanup_data(item.lstat(), before, fd)
            after_directory = private.lstat()
            if ident(after_directory) != ident(directory):
                need(ident(after_directory)[:5] == ident(directory)[:5]
                     and after_directory.st_nlink == directory.st_nlink, "receipt pending directory replaced")
                # TempDir는 이 두 파일만 정리한다. 새 파일/하위 directory/alias는 예외가 아니다.
                for child in private.iterdir():
                    need(child.name in ("native-raw.json", "results.pending.json"), "unknown receipt cleanup member")
                    try:
                        child_info = child.lstat()
                    except FileNotFoundError:
                        if child != private / "native-raw.json":
                            raise
                        cleanup = True
                        continue
                    need(stat.S_ISREG(child_info.st_mode) and
                         ((child == pair[0] and child_info.st_nlink in (1, 2) and
                           (child_info.st_dev, child_info.st_ino) == expected) or
                          (child != pair[0] and child_info.st_nlink == 1)), "receipt cleanup member replaced/aliased")
                for fd, before in zip(fds, infos):
                    cleanup |= cleanup_data(os.fstat(fd), before, fd)
                    need(os.pread(fd, MIB + 1, 0) == raw, "receipt bytes changed during directory cleanup")
                cleanup = True
            if cleanup:
                raise BudgetTransition("exact receipt/owned TempDir cleanup crossed validation")
            self.native_root()
            if self.native_publication is not None:
                need(self.native_publication["dev_inode"] == list(expected)
                     and self.native_publication["pending"] == str(pair[0])
                     and self.native_publication["sha256"] == sha(raw), "observed receipt publication changed")
            self.native_publication = {"root_identity": self.native_root_identity, "pending": str(pair[0]),
                "results": str(pair[1]), "dev_inode": list(expected), "nlink": 2, "bytes": len(raw),
                "sha256": sha(raw), "complete_result": result, "scope": "observed exact atomic publication pair only"}
        except FileNotFoundError as error:
            # 이 함수는 위의 exact pair/owned directory만 읽는다. 누락을 성공으로
            # 바꾸지 않고 전체 표본을 다시 요구한다. parser/hash/permission 오류는 전파한다.
            if Path(error.filename) not in (*pair, private):
                raise
            raise BudgetTransition("owned receipt publication path disappeared") from error
        finally:
            for fd in fds:
                os.close(fd)

    def budget(self, force=False):
        if not force and time.monotonic() - self.last_growth < 0.5:
            return
        for attempt in range(3):
            totals, devices, transient = self.budget_scan()
            # 불완전한 표본도 초과 lower bound면 즉시 거부한다. 성공 판정은
            # ENOENT/게시 전이가 없는 전체 표본에만 가능하다.
            need(totals["source"] <= 2 * GIB and totals["build"] <= 8 * GIB and totals["proof"] <= GIB
                 and totals["sdk"] <= 3 * GIB, f"owned source/build/proof/SDK bound exceeded: {totals}")
            if not transient:
                break
            need(len(self.budget_resamples) < 256, "mutable/publication resample record bound")
            self.budget_resamples.append({"phase": self.phase, "attempt": attempt + 1, "paths": transient,
                                          "partial_totals": totals, "partial_sample_accepted": False})
            need(attempt < 2, "bounded mutable/publication resampling exhausted")
        if self.build_started and not self.finalizing:
            for device, path in devices.items():
                remaining = 0
                if device in (self.root.stat().st_dev, self.job.stat().st_dev):
                    remaining += 8 * GIB - totals["build"]
                if device == self.job.stat().st_dev:
                    remaining += 2 * GIB - totals["proof"]
                need(shutil.disk_usage(path).free >= remaining, f"remaining physical filesystem admission: {path}")
        self.usage = totals
        self.last_growth = time.monotonic()

    def budget_scan(self):
        totals = {"source": 0, "build": 0, "proof": 0, "sdk": 0}
        devices = {}
        seen = set()
        transient = []
        def missing(path):
            mutable = (self.target, self.job / "cargo-home", self.tmp, self.root / ".git")
            if any(path.is_relative_to(root) and path != root for root in mutable):
                transient.append(str(path))
                return
            native = self.out / "native"
            relative = path.relative_to(native) if path.is_relative_to(native) else None
            parts = relative.parts if relative is not None else ()
            if self.native_armed and parts and (parts == ("results.json",) or
                (re.fullmatch(r"owned-trace-[A-Za-z0-9]{6}", parts[0]) and
                 (len(parts) == 1 or (len(parts) == 2 and parts[1] in ("native-raw.json", "results.pending.json"))))):
                self.native_root()
                transient.append(str(path))
                return
            raise FileNotFoundError(f"unexpected missing immutable/unknown budget path: {path}")
        count = 0
        for root, default in ((self.root.parent, "source"), (self.job, "build")):
            for path in walk(root, mutable=True, missing=missing):
                self._runtime_heartbeat(budget=True)
                try:
                    info = path.lstat()
                except FileNotFoundError:
                    missing(path)
                    continue
                count += 1
                need(count <= 180000, "owned output roster exceeds 180000")
                key = (info.st_dev, info.st_ino)
                category = "build" if path.is_relative_to(self.target) else default
                if any(path.is_relative_to(area) for area in
                       (self.rustup, self.job / "rust-bootstrap", self.job / "cargo-home/bin")):
                    category = "sdk"  # 별도 준비된 immutable SDK가 기존 build8GiB를 소비하지 않는다.
                if path.is_relative_to(self.job / "proof"):
                    category = "proof"
                    need(stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode), "special proof output")
                    if path == self.out / "native" and self.native_armed and not self.finalizing:
                        self.native_root()
                    if stat.S_ISREG(info.st_mode):
                        need(info.st_size <= 512 * MIB, "raw/proof file exceeds existing 512MiB bound")
                        if info.st_nlink != 1:
                            try:
                                self.publication_pair(path, info)
                            except BudgetTransition as error:
                                transient.append(str(error))
                if key in seen:
                    continue
                seen.add(key)
                totals[category] += max(info.st_blocks * 512, info.st_size if stat.S_ISREG(info.st_mode) else 0)
                devices.setdefault(info.st_dev, path if path.is_dir() else path.parent)
        return totals, devices, transient

    def resources(self, label):
        cgroup = {}
        for name in ("memory.current", "memory.peak", "memory.events", "pids.current"):
            path = Path("/sys/fs/cgroup") / name
            if path.is_file():
                cgroup[name] = path.read_text()[:65536]
        save(self.out / f"resources-{label}.json", {"owned_bytes": self.usage, "cgroup": cgroup,
             "meminfo": Path("/proc/meminfo").read_text(),
             "maxrss_self_kib": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
             "maxrss_children_kib": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
             "scope": "sampled allocations; shared cgroup counters not exclusively attributable; no peak guarantee"})

    def maps(self, rows, allowed):
        observed = set()
        for pid, row in rows.items():
            try:
                need(proc_stat(pid)["start"] == row["start"], "mapping PID reused")
                for line in Path(f"/proc/{pid}/maps").read_text().splitlines():
                    fields = line.split(None, 5)
                    if len(fields) != 6 or "x" not in fields[1] or not fields[5].startswith("/"):
                        continue
                    need(not fields[5].endswith(" (deleted)"), "deleted executable mapping")
                    path = Path(fields[5])
                    canonical = path.resolve(strict=True)
                    if allowed is not None:
                        need(canonical in allowed, f"unbound runtime mapping: {canonical}")
                        need(not any(part in canonical.name for part in ("libpython", "ekf_sym_pyx", "libcar.so")),
                             "original/Python numerical backend mapped by native normal40")
                    elif not canonical.is_relative_to(self.root) and not canonical.is_relative_to(self.job):
                        self.holds.add(path, elf=True)
                    major, minor = (int(value, 16) for value in fields[3].split(":"))
                    info = canonical.stat()
                    need((info.st_dev, info.st_ino) == (os.makedev(major, minor), int(fields[4])),
                         "mapping inode differs")
                    observed.add(str(canonical))
                    if allowed is not None:
                        item = self.runtime_pid_maps.setdefault(pid, {"start": row["start"], "files": set()})
                        need(item["start"] == row["start"], "runtime mapping PID reused")
                        item["files"].add(str(canonical))
                        sample = self._runtime_sample
                        key = (pid, str(canonical))
                        if sample is not None and key not in sample["first_mappings"]:
                            record = {"pid": pid, "start": row["start"], "path": str(canonical),
                                      "dev_inode": [info.st_dev, info.st_ino],
                                      "elapsed_seconds": time.monotonic() - sample["started"]}
                            sample["record_bytes"] += len(json.dumps(record).encode())
                            need(sample["record_bytes"] <= MIB, "runtime mapping observation exceeds 1 MiB")
                            sample["first_mappings"][key] = record
            except (FileNotFoundError, ProcessLookupError):
                continue
        return observed

    def import_frozen(self):
        token_env = {**self.env, "GH_TOKEN": os.environ["GH_TOKEN"]}
        def api(endpoint, label, size=MIB):
            return unique_json(self.command([self.tools["gh"], "api", f"repos/{REPO}/{endpoint}"],
                                           label, env=token_env, limit=size))
        meta = api(f"actions/artifacts/{ARTIFACT}", "historical-artifact-api")
        run = api(f"actions/runs/{RUN}", "historical-run-api")
        job = api(f"actions/jobs/{JOB}", "historical-job-api")
        need(meta["id"] == ARTIFACT and meta["name"] == "rust-controls-evidence"
             and meta["size_in_bytes"] == ZIP_SIZE and meta["digest"] == "sha256:" + ZIP_SHA
             and meta["expired"] is False and meta["workflow_run"]["id"] == RUN
             and meta["workflow_run"]["head_sha"] == HEAD, "historical artifact identity differs")
        need(run["id"] == RUN and run["head_sha"] == HEAD and run["status"] == "completed"
             and run["repository"]["full_name"] == REPO, "historical run identity differs")
        need(job["id"] == JOB and job["run_id"] == RUN and job["head_sha"] == HEAD
             and job["status"] == "completed" and job["conclusion"] == "success", "historical job did not succeed")
        archive_path = self.out / "frozen-archive.stdout"
        self.command([self.tools["gh"], "api", f"repos/{REPO}/actions/artifacts/{ARTIFACT}/zip"],
                     "frozen-archive", seconds=180, env=token_env, limit=ZIP_SIZE + 1)
        need(archive_path.stat().st_size == ZIP_SIZE, "historical ZIP byte count differs")
        self.holds.add(archive_path, ZIP_SHA)
        # 고정 raw 로그는 private 파일에만 수집한다. ANSI 원문을 변환하지 않고 아래 size/SHA로 확인한다.
        log = self.command([self.tools["gh"], "api", "--allow-escape-sequences", f"repos/{REPO}/actions/jobs/{JOB}/logs"],
                           "historical-job-log", seconds=180, env=token_env, limit=LOG_SIZE + 1)
        need(len(log) == LOG_SIZE and sha(log) == LOG_SHA, "historical raw job log differs")
        self.holds.add(self.out / "historical-job-log.stdout", LOG_SHA)
        reference = self.out / "reference"
        reference.mkdir(mode=0o700)
        (reference / "loop").mkdir(mode=0o700)
        for old, name, pin in ((archive_path, "artifact.zip", ZIP_SHA),
                               (self.out / "historical-job-log.stdout", "historical-job.log", LOG_SHA)):
            copy_new(old, reference / name, ZIP_SIZE if name == "artifact.zip" else LOG_SIZE)
            self.holds.add(reference / name, pin)
        imported = {}
        with zipfile.ZipFile(archive_path) as archive:
            records = archive.infolist()
            need(len(records) == len({item.filename for item in records}) == 120
                 and sum(item.file_size for item in records) == 259913092, "historical ZIP roster differs")
            for item in records:
                name = item.filename
                path = PurePosixPath(name)
                mode = item.external_attr >> 16
                need(name and not path.is_absolute() and path.as_posix() == name and "\\" not in name
                     and all(part not in ("", ".", "..") for part in name.split("/"))
                     and not item.is_dir() and stat.S_IFMT(mode) in (0, stat.S_IFREG)
                     and not item.flag_bits & 1, "unsafe/encrypted/special ZIP member")
            for name, (size, pin) in MEMBERS.items():
                item = archive.getinfo(name)
                need(item.file_size == size and item.compress_size <= ZIP_SIZE
                     and item.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED), "raw member shape differs")
                target = reference / name
                fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o400)
                count = crc = 0
                digest = hashlib.sha256()
                with archive.open(item) as source, os.fdopen(fd, "wb") as output:
                    while chunk := source.read(MIB):
                        count += len(chunk)
                        need(count <= size, "raw frozen member exceeds literal size")
                        digest.update(chunk)
                        crc = zlib.crc32(chunk, crc)
                        output.write(chunk)
                    output.flush()
                    os.fsync(output.fileno())
                need(count == size and digest.hexdigest() == pin and crc == item.CRC,
                     "raw frozen member size/SHA/CRC differs")
                self.holds.add(target, pin)
                imported[name] = {"bytes": count, "sha256": pin, "crc32": crc}
                self.budget(force=True)
        historical_result = unique_json(read(reference / "loop/results.json", 114))
        need(historical_result == {"pass": True, "cases": 40, "frames": 8655,
                                  "numeric_comparisons": 705948, "max_absolute_error": 0.0},
             "historical normal40 result differs")
        index = {"schema": 1, "reference_kind": "historical-original-data", "reference_id": REFERENCE_ID,
             "repository": REPO, "artifact_id": ARTIFACT, "run_id": RUN, "job_id": JOB,
             "artifact_name": "rust-controls-evidence", "artifact_bytes": ZIP_SIZE, "artifact_sha256": ZIP_SHA,
             "workflow_head_sha": HEAD, "historical_checkout_sha": CHECKOUT, "historical_job_result": "success",
             "historical_runner": "GitHub Actions 1000027460", "historical_runner_image": "ubuntu-24.04",
             "historical_job_log": {"name": "historical-job.log", "bytes": LOG_SIZE, "sha256": LOG_SHA},
             "historical_original_provider_closure": "not-recorded",
             "historical_original_provider_closure_complete": False,
             "historical_provider_paths_dereferenced": False, "original_capture_executed": False,
             "members": [{"name": name, "bytes": size, "sha256": pin} for name, (size, pin) in MEMBERS.items()],
             "historical_result": historical_result}
        source = read(self.root / "rust/crates/dev-tools/src/controlsd_check/frozen.rs", 2 * MIB).decode()
        literal = re.findall(r'pub const REFERENCE_INDEX: &str = r#"(.*?)"#;', source, re.S)
        need(len(literal) == 1 and unique_json(literal[0]) == index, "Rust/importer fixed reference recipe differs")
        # source의 고정 recipe 원문을 복사한다. 재직렬화/newline 추가는 없다.
        write_new(reference / "frozen-reference-admission.json", literal[0].encode())
        self.holds.add(reference / "frozen-reference-admission.json")
        self.reference_before = snapshot(reference, walk(reference))
        save(self.out / "reference-before.json", self.reference_before)
        save(self.out / "import-receipt.json", {"raw_members": imported, "archive_sha256": ZIP_SHA,
             "selected_members_crc_checked": 4, "unselected_members_inflated": 0,
             "historical_checkout": CHECKOUT, "historical_run_conclusion": run["conclusion"],
             "historical_job_result": "success", "original_capture_executed": False,
             "historical_original_provider_closure_complete": False, "original_backend_execs": 0})
        self.reference = reference
        self.holds.verify()

    def cargo(self, args, label, package="openpilot-dev-tools", features=FEATURES):
        return self.command([self.sdk_root / "bin/cargo", *args, *SHA2_PROFILE_ARGS, "--manifest-path", self.root / "rust/Cargo.toml",
                             "--locked", "-p", package, "--no-default-features", "--features", features],
                            label, seconds=1200)

    def select(self, raw, name, kind, source, features, package="dev-tools", tested=False):
        rows = [unique_json(line) for line in raw.splitlines()]
        need(rows and all(isinstance(row, dict) and row.get("reason") in
             ("compiler-artifact", "compiler-message", "build-script-executed", "build-finished") for row in rows),
             "Cargo JSONL reason/shape differs")
        need(rows[-1] == {"reason": "build-finished", "success": True}
             and sum(row["reason"] == "build-finished" for row in rows) == 1, "Cargo build-finished differs")
        found = []
        seen = set()
        for row in rows:
            if row["reason"] != "compiler-artifact":
                continue
            key = json.dumps(row, sort_keys=True)
            need(key not in seen, "duplicate Cargo compiler artifact")
            seen.add(key)
            target = row["target"]
            if target["name"] != name or target["kind"] != [kind] or row["profile"]["test"] is not tested:
                continue
            manifest = self.root / f"rust/crates/{package}/Cargo.toml"
            package_name = "openpilot-" + package
            need(row["manifest_path"] == str(manifest) and target["src_path"] == str(self.root / source)
                 and row["package_id"] == f"path+file://{manifest.parent}#{package_name}@0.1.0"
                 and sorted(row["features"]) == sorted(features), "Cargo exact package/target/features differ")
            need(isinstance(row["executable"], str) and row["executable"], "Cargo executable must be non-null")
            path = Path(row["executable"])
            need(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(self.target),
                 "Cargo ELF escaped private current target")
            info = path.lstat()
            need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_mode & 0o111,
                 "Cargo ELF kind/owner/mode differs")
            self.holds.add(path, elf=True)
            found.append((path, row))
        need(len(found) == 1, f"missing/duplicate exact Cargo artifact: {name}")
        return found[0]

    def stage_numerics(self, stager):
        name = "numpy-2.5.3-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl"
        packages = tomllib.loads(read(self.root / "uv.lock", 4 * MIB).decode())["package"]
        numpy = [row for row in packages if row["name"] == "numpy"]
        need(len(numpy) == 1 and numpy[0]["version"] == "2.5.3", "source NumPy lock differs")
        wheels = [row for row in numpy[0]["wheels"] if row["url"].rsplit("/", 1)[-1] == name]
        need(len(wheels) == 1 and wheels[0]["size"] == 16717410
             and wheels[0]["hash"] == "sha256:b7e18c623bb5c95acb3b3328861272816ba199fb531921c5d6d0b675f1fde9e3",
             "source-locked numerical wheel differs")
        download = self.out / "numerics-wheel.stdout"
        self.command([self.tools["curl"], "--proto", "=https", "--fail", "--location", "--retry", "3",
                      "--max-time", "180", "--max-filesize", "16717410", wheels[0]["url"]],
                     "numerics-wheel", seconds=240, limit=16717411)
        need(download.stat().st_size == 16717410, "NumPy wheel size differs")
        self.holds.add(download, wheels[0]["hash"].removeprefix("sha256:"))
        # The stager requires the exact source-lock filename, with no install/import.
        wheel = self.job / "cache" / name
        copy_new(download, wheel, 16717410)
        self.holds.add(wheel, wheels[0]["hash"].removeprefix("sha256:"))
        self.command([stager, "--wheel", wheel, "--lock", self.root / "uv.lock", "--output", self.staged],
                     "numerics-stage", seconds=180)
        self.staged_before = snapshot(self.staged, walk(self.staged))
        save(self.out / "numerics-staged-before.json", self.staged_before)
        manifest = unique_json(read(self.staged / "manifest.json", MIB))
        need(set(manifest) == {"format", "numpy", "abi", "library", "files"}
             and manifest["format"] == 1 and manifest["numpy"] == "2.5.3" and manifest["abi"] == "scipy_dgesdd_64_",
             "staged numerics manifest differs")
        expected = {"manifest.json", "provenance.json"}
        directory_markers = set()
        with zipfile.ZipFile(wheel) as archive:
            for item in archive.infolist():
                if item.is_dir() and not item.filename.startswith("numpy.libs/"):
                    continue
                if item.filename.startswith("numpy.libs/"):
                    name = item.filename.rstrip("/").rsplit("/", 1)[-1]
                    if item.is_dir():
                        # 기존 stager는 wheel의 빈 numpy.libs/를 일반파일 marker로 보존한다.
                        need(item.filename == "numpy.libs/" and item.file_size == 0
                             and archive.read(item) == b"", "numerics directory marker differs")
                        directory_markers.add(name)
                    expected.add(name)
                elif ".dist-info/licenses/" in item.filename:
                    expected.add("licenses/" + item.filename.split("/licenses/", 1)[1])
        need({name for name, row in self.staged_before.items() if "sha256" in row} == expected,
             "staged numerics extra/missing file roster")
        declared = {}
        for row in manifest["files"]:
            need(set(row) == {"name", "sha256"} and row["name"] not in declared
                 and PurePosixPath(row["name"]).name == row["name"], "numerical file duplicate/path differs")
            declared[row["name"]] = row["sha256"]
        libraries = {name for name in expected if "/" not in name} - {"manifest.json", "provenance.json"}
        need(set(declared) == libraries and manifest["library"] in libraries - directory_markers,
             "numerical DSO roster incomplete")
        for name in directory_markers:
            need(declared[name] == sha(b"") and self.staged_before[name]["identity"][5] == 0
                 and self.staged_before[name]["sha256"] == sha(b""), "numerics marker bytes differ")
        save(self.out / "numerics-directory-markers.json", {
             "members": sorted(directory_markers), "size": 0, "sha256": sha(b""),
             "scope": "exact source-locked wheel directory markers preserved by the existing stager; not DSOs"})
        for name, row in self.staged_before.items():
            need(not "symlink" in row, "numerics symlink forbidden")
            if "sha256" in row:
                need(row["identity"][6] == 1, "numerics hardlink alias forbidden")
                self.holds.add(self.staged / name, declared.get(name))
        # Keep the licensed complete staging tree. Frozen Rust admission has a
        # separate exact runtime roster: manifest plus declared DSOs only.
        self.numerics.mkdir(mode=0o700)
        for name in ["manifest.json", *sorted(declared)]:
            copy_new(self.staged / name, self.numerics / name, 512 * MIB)
            self.holds.add(self.numerics / name, declared.get(name))
        self.numerics_before = snapshot(self.numerics, walk(self.numerics))
        save(self.out / "numerics-before.json", self.numerics_before)
        for index, name in enumerate(sorted(set(declared) - directory_markers)):
            self.dso(self.numerics / name, f"numerics-dso-{index}")
        self.numerical_library = (self.numerics / manifest["library"]).resolve(strict=True)
        self.env["CONTROLS_NUMERICS"] = str(self.numerics)
        self.env["CONTROLS_ASSETS"] = str(self.root / "opendbc_repo/opendbc/car/torque_data")

    def trace_actual(self, binary, args, allowed, extra_env):
        trace = self.out / "normal40.exec"
        argv = [str(binary), *map(str, args)]
        env = {**self.env, "PATH": "/nonexistent", **extra_env}
        self.runtime_pid_maps = {}
        output = self.command([self.tools["strace"], "-f", "-qq", "-s", "65535", "-e", "trace=execve,execveat",
                               "-o", trace, *argv], "normal40", seconds=1200, env=env, allowed=allowed)
        pending = {}
        executions = []
        signals = 0
        for raw in read(trace, 16 * MIB).decode("ascii").splitlines():
            match = re.fullmatch(r"([1-9][0-9]*) +(.*)", raw)
            need(match, "unknown runtime trace prefix")
            pid, body = int(match[1]), match[2]
            if body.endswith("<unfinished ...>"):
                need(pid not in pending and body.startswith("execve("), "unmatched/unknown unfinished exec")
                pending[pid] = body.removesuffix("<unfinished ...>")
                continue
            if body.startswith("<... "):
                resumed = re.fullmatch(r"<\.\.\. execve resumed>(.*)", body)
                need(resumed and pid in pending, "unmatched/unknown resumed exec")
                body = pending.pop(pid) + resumed[1]
            if body.startswith("execve"):
                invocation = re.fullmatch(r'execve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0', body)
                need(invocation, "failed/opaque/truncated runtime exec attempt")
                executions.append({"pid": pid, "program": unique_json(invocation[1]),
                                   "argv": unique_json(invocation[2])})
                continue
            metadata = (r"\+\+\+ exited with 0 \+\+\+",
                        r"--- SIGCHLD \{si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=[1-9][0-9]*, si_uid=[0-9]+, si_status=0, si_utime=[0-9]+(?: /\* [0-9]+\.[0-9]{2} s \*/)?, si_stime=[0-9]+(?: /\* [0-9]+\.[0-9]{2} s \*/)?\} ---",
                        r"--- SIGUSR1 \{si_signo=SIGUSR1, si_code=SI_TKILL, si_pid=[1-9][0-9]*, si_uid=[0-9]+\} ---")
            need(any(re.fullmatch(pattern, body) for pattern in metadata), "unknown runtime trace metadata")
            signals += int(body.startswith("---"))
        need(not pending and executions and executions[0]["program"] == str(binary)
             and executions[0]["argv"] == argv, "first exec/argv or EOF pairing differs")
        self.holds.add(trace)
        self.holds.verify()
        save(self.out / "runtime-trace.json", {"executions": executions, "received_signal_records": signals,
             "pending_at_eof": 0, "scope": "strict traced exec attempts; no whole Python absence claim",
             "sampled_pid_maps": {str(pid): {"start": row["start"], "files": sorted(row["files"])}
                                  for pid, row in self.runtime_pid_maps.items()}})
        return output.decode(), executions

    @staticmethod
    def test_result(raw, selector):
        output = raw.decode() if isinstance(raw, bytes) else raw
        results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", output)
        need(len(results) == 1 and results[0][:4] == ("1", "0", "0", "0")
             and re.findall(r"^test ([\w:]+) \.\.\. ok$", output, re.M) == [selector],
             "exact frozen selector did not pass once")

    def run(self):
        self.budget(force=True)
        need(shutil.disk_usage(self.job).free >= 2 * GIB, "existing proof2GiB free-space admission")
        self.resources("before")
        self.sdk_before = self.sdk("before")
        self.providers()
        _mapping_heartbeat_controls(self)
        self.source_before = self.sources("before")
        for name, pin in CORE.items():
            self.holds.add(self.root / name, pin)
        self.phase = "historical-raw4"
        self.import_frozen()
        self.phase = "current-build"
        self.build_started = True
        self.budget(force=True)
        save(self.out / "growth-build-admission.json", {"source_cap": 2 * GIB, "build_cap": 8 * GIB,
             "proof_total_cap": GIB, "per_raw_file_cap": 512 * MIB, "proof_free_space_admission": 2 * GIB,
             "separate_prepared_sdk_cap": 3 * GIB,
             "physical_filesystem_remaining_check": True, "sample_period_seconds": 0.5,
             "current": self.usage, "measured_peak_guarantee": False})
        binaries = self.cargo(["build", "--bin", "check-controlsd-native", "--bin", "stage-torque-numerics",
                               "--message-format=json"], "current-tools-build")
        trace_raw = self.cargo(["build", "--example", "controlsd_trace", "--message-format=json"],
                               "current-trace-build", "openpilot-controlsd", "native")
        need(shutil.disk_usage(self.root).free >= 512 * MIB, "existing auditor append512MiB admission")
        tests_raw = self.cargo(["test", "--test", "controlsd_check", "--no-run", "--message-format=json"], "frozen-tests-build")
        self.resources("built")
        self.phase = "strict-clippy"
        need(shutil.disk_usage(self.root).free >= 512 * MIB, "existing Clippy append512MiB admission")
        self.command([self.sdk_root / "bin/cargo", "clippy", *SHA2_PROFILE_ARGS, "--manifest-path", self.root / "rust/Cargo.toml", "--locked",
                      "-p", "openpilot-dev-tools", "--no-default-features", "--features", FEATURES,
                      "--bin", "check-controlsd-native", "--bin", "stage-torque-numerics", "--test", "controlsd_check",
                      "--", "-D", "warnings"], "current-clippy", seconds=1200)
        # Select/hold final paths only after every Cargo invocation; an earlier
        # no-run build may legitimately relink bins for its dependency graph.
        checker, checker_row = self.select(binaries, "check-controlsd-native", "bin",
            "rust/crates/dev-tools/src/bin/check_controlsd_native.rs", DEV_FEATURES)
        stager, stager_row = self.select(binaries, "stage-torque-numerics", "bin",
            "rust/crates/dev-tools/src/bin/stage_torque_numerics.rs", DEV_FEATURES)
        trace, trace_row = self.select(trace_raw, "controlsd_trace", "example",
            "rust/crates/controlsd/examples/controlsd_trace.rs", ["native"], "controlsd")
        auditor, auditor_row = self.select(tests_raw, "controlsd_check", "test",
            "rust/crates/dev-tools/tests/controlsd_check.rs", DEV_FEATURES, tested=True)
        test_checker, _ = self.select(tests_raw, "check-controlsd-native", "bin",
            "rust/crates/dev-tools/src/bin/check_controlsd_native.rs", DEV_FEATURES)
        need(test_checker == checker, "CARGO_BIN_EXE checker differs from the separately selected CLI")
        need(len({checker, stager, trace, auditor}) == 4, "four distinct exact current ELFs required")
        save(self.out / "current-artifacts.json", {"checker": checker_row, "trace": trace_row,
             "stager": stager_row, "auditor": auditor_row, "source": self.source})
        self.phase = "current-numerics"
        self.stage_numerics(stager)
        allowed = set(self.holds.files)
        for index, path in enumerate((checker, trace, auditor, self.tools["strace"])):
            allowed.update(self.dso(path, f"runtime-dso-{index}"))
        allowed.update(self.holds.files)
        save(self.out / "holds-before-runtime.json", self.holds.records())
        extra = {"CONTROLSD_CONTROLLER_ROOT": str(self.root), "CONTROLSD_FROZEN_REFERENCE": str(self.reference),
                 "CONTROLSD_CONTROLLER_TRACE": str(trace), "CONTROLSD_CONTROLLER_SOURCE_SHA": self.source,
                 "CONTROLSD_FROZEN_EVIDENCE": str(self.out / "native")}
        self.phase = "frozen-negative-control"
        raw = self.command([auditor, "--exact", NEGATIVE, "--nocapture", "--test-threads=1", "--color=never"],
                           "frozen-negative", seconds=240, env={**self.env, **extra})
        self.test_result(raw, NEGATIVE)
        raw = self.command([auditor, "--exact", HELD_NEGATIVE, "--nocapture", "--test-threads=1", "--color=never"],
                           "frozen-held-negative", seconds=240, env={**self.env, **extra})
        self.test_result(raw, HELD_NEGATIVE)
        self.phase = "actual-normal40"
        native = self.out / "native"
        need(not native.exists() and not native.is_symlink(), "actual native evidence must be absent before spawn")
        self.native_armed = True
        args = ["--ignored", "--exact", ACTUAL, "--nocapture", "--test-threads=1", "--color=never"]
        output, executions = self.trace_actual(auditor, args, allowed, extra)
        self.native_armed = False
        self.test_result(output, ACTUAL)
        cli_args = [str(checker), "--root", str(self.root), "--trace", str(trace), "--numerics", str(self.numerics),
                    "--evidence", str(self.out / "native"), "--source-commit", self.source,
                    "--frozen-reference", REFERENCE_ID, "--reference-dir", str(self.reference)]
        need(len(executions) == 3 and executions[1]["program"] == str(checker)
             and executions[1]["argv"] == cli_args and executions[2]["program"] == str(trace),
             "actual normal40 must invoke exact current CLI once and current trace once")
        evidence = self.out / "native"
        admission = unique_json(read(evidence / "admission.json", 16 * MIB))
        need(admission["argv"] == executions[2]["argv"] and len(executions[2]["argv"]) == 2
             and executions[2]["argv"][0] == str(trace)
             and Path(executions[2]["argv"][1]).is_relative_to(evidence), "actual trace output argv differs")
        self._require_runtime_mappings(executions[2]["pid"], trace, self.numerical_library)
        need(admission["reference_id"] == REFERENCE_ID and admission["reference_kind"] == "historical-original-data"
             and admission["current_source_commit"] == self.source and admission["original_source_commit"] == CHECKOUT
             and admission["original_backend_execs"] == 0 and admission["historical_original_provider_closure_complete"] is False
             and admission["historical_provider_paths_dereferenced"] is False and admission["original_capture_executed"] is False
             and admission["full_runtime_acceptance"] is False and admission["inventory_credit"] is False,
             "current/historical provenance fields differ")
        result_raw = read(evidence / "results.json", MIB)
        result = unique_json(result_raw)
        if self.native_publication is not None:
            need(sha(result_raw) == self.native_publication["sha256"], "completed receipt differs from published pair")
        need(self.native_root_fd is not None and ident(evidence.lstat())[:5] == self.native_root_identity
             == ident(os.fstat(self.native_root_fd))[:5], "completed native root identity differs")
        need(result["pass"] is True and result["cases"] == 40 and result["frames"] == 8655
             and result["numeric_comparisons"] == 705948 and type(result["max_absolute_error"]) in (float, int)
             and 0 <= result["max_absolute_error"] < float("inf"), "fresh normal40 result/counts differ")
        self.runtime = {"exact_ignored_tests": [ACTUAL], "new_negative_tests": [NEGATIVE, HELD_NEGATIVE], "normal40_cli_invocations": 1,
                        "normal40_trace_invocations": 1, "actual_traced_execs_including_auditor": 3,
                        "current_normal40": result, "fresh_legacy_regular_tests": 0, "new_frozen_negative_controls": 2,
                        "negative_control_cli_exec_scope": "rejection-only; separate from normal40 count; not individually traced",
                        "numeric_library_mapped_by_trace_pid": True}
        self.native_before = snapshot(evidence, walk(evidence))
        save(self.out / "native-before.json", self.native_before)
        for path in walk(evidence):
            if path.is_file():
                self.holds.add(path)
        self.budget(force=True)
        self.resources("normal40")

    def finish(self, error):
        self.finalizing = True
        self.deadline = time.monotonic() + 180
        failures = []
        operations = [("children", self.children.stop),
            ("source", lambda: need(self.source_before is not None and self.sources("after") == self.source_before,
                                     "source/index seal differs")),
            ("sdk", lambda: need(self.sdk_before is not None and self.sdk("after") == self.sdk_before, "SDK seal differs"))]
        for label, root, baseline in (("reference", self.out / "reference", self.reference_before),
                                      ("numerics", self.numerics, self.numerics_before),
                                      ("numerics-staged", self.staged, self.staged_before),
                                      ("native", self.out / "native", self.native_before),
                                      *((f"system-sdk-{i}", root, baseline) for i, (root, baseline) in enumerate(self.header_before.items()))):
            def verify_tree(label=label, root=root, baseline=baseline):
                need(baseline is not None, f"{label}: baseline unavailable")
                after = snapshot(root, walk(root))
                save(self.out / f"{label}-after.json", after)
                need(after == baseline, f"{label}: exact roster/identity/SHA changed")
            operations.append((label, verify_tree))
        # Source-final uses bound Git/providers. Recheck held files last, after
        # every attempted subprocess, not before final source commands.
        operations.extend((("held", self.holds.verify), ("budget", lambda: self.budget(force=True)),
                           ("resources", lambda: self.resources("final"))))
        for label, operation in operations:
            try:
                operation()
            except BaseException as problem:
                failures.append(f"{label}: {problem}")
        save(self.out / "commands.json", self.commands)
        save(self.out / "holds-after.json", self.holds.records())
        live = [{key: value for key, value in row.items() if key != "fd"} for row in self.children.scan().values()]
        if live:
            failures.append("owned children remain after finalizer")
        save(self.out / "children.json", {"subreaper": True, "live": live, "events": self.children.history})
        save(self.out / "final-seals.json", {"pass": not failures, "failures": failures, "source": self.source,
             "all_owned_children_reaped": not live, "phase": self.phase})
        save(self.out / "receipt-publication-observation.json", {"observed_pair": self.native_publication,
             "resamples": self.budget_resamples,
             "scope": "exact fresh actual-normal40 publication; no general hardlink exception"})
        if error is not None or failures:
            save(self.out / "failure.json", {"error": error, "final_failures": failures, "phase": self.phase,
                 "current_normal40_accepted": False, "original_capture_executed": False})
            return False
        self.budget(force=True)
        need(self.usage["proof"] + MIB <= GIB, "complete receipt reserve unavailable")
        publish_complete(self.out / "complete.json", {"schema": "controls-normal40-523-v1", "pass": True,
             "source": self.source, **self.runtime, "reference_id": REFERENCE_ID, "artifact_id": ARTIFACT,
             "historical_checkout": CHECKOUT, "historical_original_provider_closure_complete": False,
             "historical_provider_paths_dereferenced": False, "original_capture_executed": False,
             "original_python_executions": 0, "original_backend_execs": 0, "all_owned_children_reaped": True,
             "sources_retired": [], "inventory_credit": False, "full_runtime_acceptance": False,
             "rejection27_health5_continuous140x2_accepted": False, "device_acceptance": False,
             "temporary_python_controller": "pending", "growth": self.usage})
        return True


def _mapping_heartbeat_controls(probe):
    """GHA에서만 실제 owned child와 같은 mapping guard를 검사한다."""
    need(not probe.children.scan() and probe._runtime_sample is None, "mapping controls require idle ownership")
    bash = probe.tools["bash"]
    libraries = [path for path in probe.holds.files if path.name == "libc.so.6"]
    need(len(libraries) == 1, "mapping control requires one held libc")
    libc = libraries[0]
    absent = probe.tools["timeout"]
    allowed = set(probe.holds.files)
    previous_maps = probe.runtime_pid_maps
    probe.runtime_pid_maps = {}
    previous_phase = probe.phase
    probe.phase = "mapping-heartbeat-controls"
    deadline = min(time.monotonic() + 60, probe.deadline)
    report = {"schema": 1, "pass": False, "controls": {}, "actual_openblas_observed": False,
              "full_normal40_acceptance": False, "complete_mapping_closure": False}
    process = sample = None
    try:
        with (probe.out / "mapping-heartbeat-control.stderr").open("xb") as stderr:
            process = subprocess.Popen([bash, "--noprofile", "--norc", "-c",
                'printf "READY\\n"; IFS= read -r release; test "$release" = RELEASE'],
                cwd=probe.root, env=probe.env, start_new_session=True, stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=stderr, preexec_fn=lambda: probe.child_limits(True))
            probe.children.process = process
            os.set_blocking(process.stdout.fileno(), False)
            ready = b""
            while ready != b"READY\n":
                need(time.monotonic() <= deadline, "mapping control readiness deadline")
                probe.children.reap()
                need(process.returncode is None and stderr.tell() <= MIB, "mapping control exited/log overflow before READY")
                try:
                    block = os.read(process.stdout.fileno(), 64)
                    need(block, "mapping control READY EOF")
                    ready += block
                    need(b"READY\n".startswith(ready), "mapping control foreign READY bytes")
                except BlockingIOError:
                    time.sleep(0.02)
            sample = probe._start_runtime_sample(allowed, "mapping-heartbeat-controls", deadline, set())
            # 최초 표본을 command loop가 아닌 실제 전체 budget 순회에서 취한다.
            probe.budget(force=True)
            need(sample["budget_samples"] > 0, "budget traversal did not sample owned mappings")
            probe._require_runtime_mappings(process.pid, bash, libc)
            report["controls"]["budget_walk_observes_owned_elf_and_libc"] = True
            row = probe.children.scan()[process.pid]
            try:
                probe.maps({process.pid: {**row, "start": row["start"] + 1}}, allowed)
            except RuntimeError as error:
                need(str(error) == "mapping PID reused", "wrong-start control failed for another reason")
            else:
                raise RuntimeError("mapping guard accepted wrong start ticks")
            report["controls"]["wrong_start_ticks_rejected"] = True
            need(str(absent) not in probe.runtime_pid_maps[process.pid]["files"], "absent-provider control is not absent")
            try:
                probe._require_runtime_mappings(process.pid, bash, absent)
            except RuntimeError as error:
                need(str(error) == "actual trace and numerical library were not observed mapped by its PID",
                     "missing-provider control failed for another reason")
            else:
                raise RuntimeError("mapping postcondition accepted absent required provider")
            report["controls"]["missing_required_provider_rejected"] = True
            need(time.monotonic() <= deadline, "mapping control release deadline")
            probe._runtime_sample = None
            process.stdin.write(b"RELEASE\n")
            process.stdin.close()
            while process.returncode is None:
                need(time.monotonic() <= deadline, "mapping control release/reap deadline")
                probe.children.reap()
                time.sleep(0.02)
            need(process.returncode == 0 and not probe.children.scan() and stderr.tell() == 0,
                 "mapping control release/exit/children/stderr differs")
            for path in (bash, libc):
                item = probe.holds.files[path]
                need(ident(os.fstat(item["fd"])) == item["seal"]["identity"] and regular(path) == item["seal"],
                     "mapping control provider seal changed")
            need(time.monotonic() <= deadline, "mapping control final deadline")
            report["pid"] = process.pid
            report["start"] = row["start"]
            report["pass"] = True
    except BaseException as error:
        report["error"] = str(error)
        raise
    finally:
        probe._runtime_sample = None
        try:
            probe.children.stop()
        except BaseException as error:
            report["pass"] = False
            report["cleanup_error"] = str(error)
            raise
        finally:
            probe.children.process = None
            if process is not None:
                for stream in (process.stdin, process.stdout):
                    if stream is not None:
                        stream.close()
            report["live"] = sorted(probe.children.scan())
            if report["live"]:
                report["pass"] = False
            report["mapping_heartbeat"] = probe._runtime_sample_record(sample) if sample is not None else None
            probe.runtime_pid_maps = previous_maps
            save(probe.out / "mapping-heartbeat-controls.json", report)
            need(not report["live"], "mapping control left owned children")
            if report["pass"]:
                probe.phase = previous_phase


def interrupted(number, _frame):
    raise InterruptedError(f"normal40 qualification interrupted by signal {number}")


def main():
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    probe = None
    error = None
    success = False
    try:
        need(len(sys.argv) == 1, "no arguments; explicit Actions environment required")
        probe = Probe()
        probe.run()
    except BaseException:
        error = traceback.format_exc()
        print(error, file=sys.stderr)
    finally:
        if probe is not None:
            try:
                success = probe.finish(error)
            except BaseException:
                print(traceback.format_exc(), file=sys.stderr)
            finally:
                probe.holds.close()
                if probe.native_root_fd is not None:
                    os.close(probe.native_root_fd)
                os.close(probe.children.journal)
    return 0 if success else 1


if __name__ == "__main__":
    raise SystemExit(main())
