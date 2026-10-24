#!/usr/bin/env python3
"""#523 Actions 전용 A 경계. 로컬 기능/빌드 UNRUN.

새 current child-FD1 + maintained generator exec1만 실행한다.
원본/1196/1202+8/CarParams9/NumPy9600/regular7/startup B는 실행하지 않는다.
source/SDK/FD/provider 봉인과 subreaper 회수 뒤에만 complete.json을 기록한다.
"""

import base64
import ctypes
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import resource
import shutil
import signal
import stat
import subprocess
import sys
import time
import tomllib
import traceback
import zipfile

MIB = 1024**2
GIB = 1024**3
REPO = "bin9208/openpilot-rust"
SDK = Path("/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu")
FEATURES = "paramsd-fixture-native,paramsd-startup-gate-native"
FD_TEST = "native_mapped_gate_holds_actual_paramsd_second_poll_and_shutdown"
EXEC_TEST = "maintained_atomic_gate_uses_exact_native_generator_exec"
SDK_PINS = {
    "rustc": "103b60e1b1339968c1d74202ea1d45686037e82c4ea3e0569de24b18a1e6836a",
    "cargo": "77f14b761b02b47e6747473f556b3bc9f98f7e4525b7c3b8d74898ff816e4636",
    "rustfmt": "d1c6d8c0e2fae4fbb686d421415d2f471e409ade6babb3e491d9ac5d9c70c602",
    "clippy-driver": "2ff45d07196eec4f57a631103a877631d3579e86db2770bcfaa3b9d92a0b9d15",
}
CORE = {
    "rust/crates/dev-tools/src/paramsd_startup_gate.rs": "1ba6dda4e8f935f9b0d1adc8981f7a502b7baef4e5d7e562c2263598ef199b2a",
    "rust/crates/dev-tools/tests/paramsd_startup_gate.rs": "dbe1c7a7673fd2610a1995753457078ed9df3ff32e046816513360368d9b08d4",
    "rust/crates/dev-tools/tests/paramsd_car_params_cli.rs": "3d6bf1684f9bf9979fc38ac5f19b2e5e536416e413de78a459f2cdceac931981",
    "rust/crates/dev-tools/src/bin/generate_paramsd_car_params.rs": "a766c0a47ed51a59f3c81ba34e6cd7a0e06f851a48dc6d9387c995f6e105e0b5",
    "rust/crates/dev-tools/src/paramsd_fixture_generate.rs": "cc7a6811f29d51f4d2662572176f12551aec1f7b56922ca67cd62d2d36bb46ac",
    "rust/tools/paramsd_startup_phase.c": "1b524e78cdfa1cb4013a35cb570fd2fbc03cce7a0153a5ef8ba1627102265a3b",
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
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)


def walk(root, mutable=False):
    yield root
    def onerror(error):
        if not (mutable and isinstance(error, FileNotFoundError)):
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


class Probe:
    def __init__(self):
        expected = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": REPO, "GITHUB_EVENT_NAME": "push",
                    "GITHUB_REF": "refs/heads/codex/feat-523-python-rust", "RUNNER_NAME": "bin-actions",
                    "RUNNER_ENVIRONMENT": "self-hosted", "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64"}
        need(all(os.environ.get(key) == value for key, value in expected.items()), "registered Actions identity required")
        need(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "isolated stdlib-only diagnostic Python required")
        self.source = os.environ["GITHUB_SHA"]
        need(re.fullmatch(r"[0-9a-f]{40}", self.source), "source SHA required")
        self.root = Path(os.environ["DIAG_SOURCE"])
        self.job = Path(os.environ["CI_JOB_TEMP"])
        self.target = self.root / "rust/target"
        self.out = self.job / "proof/paramsd-gate"
        owner = ":".join(os.environ[key] for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB"))
        for path in (self.root, self.job, self.out.parent):
            info = path.lstat()
            need(path.is_absolute() and path.resolve(strict=True) == path and stat.S_ISDIR(info.st_mode)
                 and info.st_uid == os.getuid(), f"owned canonical directory required: {path}")
        need(read(self.job / "owner", 512).decode().strip() == owner
             == read(self.root.parent / ".diag296-owner", 512).decode().strip(), "owner marker differs")
        need(Path(os.environ["CARGO_TARGET_DIR"]) == self.target, "private target differs")
        self.out.mkdir(mode=0o700)
        self.tmp = self.job / "paramsd-tmp"
        self.tmp.mkdir(mode=0o700)
        self.holds = Holds()
        self.children = Children(self.out / "children-events.jsonl")
        self.commands = []
        self.tools = {}
        self.phase = "admission"
        self.source_before = self.sdk_before = self.eigen_before = None
        self.deadline = time.monotonic() + 52 * 60
        self.last_growth = 0
        self.namespaces = {}
        self.namespace_creations = set()
        self.active_trace = None
        self.shm_baseline = {path.name for path in Path("/dev/shm").iterdir()
                             if path.name.startswith("msgq_rust-probe-params-")}
        self.runtime = {}
        self.build_started = False
        self.finalizing = False
        self.header_before = {}
        self.env = {"PATH": f"{SDK}/bin:/usr/bin:/bin", "HOME": str(self.job / "home"),
                    "TMPDIR": str(self.tmp), "CARGO_HOME": str(self.job / "cargo-home"),
                    "CARGO_TARGET_DIR": str(self.target), "RUSTC": str(SDK / "bin/rustc"),
                    "RUSTDOC": str(SDK / "bin/rustdoc"), "RUSTUP_TOOLCHAIN": "1.94.0",
                    "CARGO_BUILD_JOBS": "2", "CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0",
                    "CARGO_PROFILE_TEST_DEBUG": "0", "CARGO_TERM_COLOR": "never", "LC_ALL": "C", "TZ": "UTC"}
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        need(hard == resource.RLIM_INFINITY or hard >= 4096, "at least 4096 FDs required")
        resource.setrlimit(resource.RLIMIT_NOFILE, (max(soft, 4096), hard))

    def budget(self, force=False):
        if not force and time.monotonic() - self.last_growth < 0.5:
            return
        self.last_growth = time.monotonic()
        seen = set()
        totals = {"source": 0, "build": 0, "proof": 0}
        devices = {}
        roots = [(self.root.parent, "source"), (self.job, "build")]
        count = 0
        for root, default in roots:
            for path in walk(root, mutable=True):
                try:
                    info = path.lstat()
                except FileNotFoundError:
                    continue
                count += 1
                need(count <= 180000, "owned output roster exceeds 180000")
                key = (info.st_dev, info.st_ino)
                if key in seen:
                    continue
                seen.add(key)
                category = default
                if path.is_relative_to(self.target):
                    category = "build"
                elif path.is_relative_to(self.job / "proof"):
                    category = "proof"
                    need(stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode), "special proof output")
                    if stat.S_ISREG(info.st_mode):
                        need(info.st_nlink == 1 and info.st_size <= 64 * MIB, "proof file kind/64MiB limit")
                totals[category] += max(info.st_blocks * 512, info.st_size if stat.S_ISREG(info.st_mode) else 0)
                devices.setdefault(info.st_dev, path if path.is_dir() else path.parent)
        need(totals["source"] <= 2 * GIB and totals["build"] <= 8 * GIB
             and totals["proof"] <= 512 * MIB, f"owned growth bound exceeded: {totals}")
        if self.build_started and not self.finalizing:
            for device, path in devices.items():
                remaining = 0
                if device in (self.root.stat().st_dev, self.job.stat().st_dev):
                    remaining += 8 * GIB - totals["build"]
                if device == self.job.stat().st_dev:
                    remaining += 512 * MIB - totals["proof"]
                need(shutil.disk_usage(path).free >= remaining, f"remaining filesystem admission failed: {path}")
        self.usage = totals
        self.last_growth = time.monotonic()

    def namespace(self, path):
        parts = path.parts
        if len(parts) < 4 or parts[:3] != ("/", "dev", "shm") or not parts[3].startswith("msgq_rust-probe-params-"):
            return False
        root = Path(*parts[:4])
        if root not in self.namespaces:
            need(root.name not in self.shm_baseline, "pre-existing namespace is protected")
            info = root.lstat()
            need(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid(), "owned namespace directory required")
            fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
            need(ident(os.fstat(fd)) == ident(info), "namespace directory changed")
            self.namespaces[root] = {"fd": fd, "identity": ident(info)}
        return True

    def learn_namespaces(self, trace):
        if trace is None or not trace.exists():
            return
        # strace가 추적한 소유 자식의 mkdir 성공만 사용한다. 큐 내용/환경은 읽지 않는다.
        fd = os.open(trace, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
        try:
            need(os.fstat(fd).st_size <= 16 * MIB, "runtime trace exceeds 16 MiB")
            raw = os.pread(fd, 16 * MIB + 1, 0).decode()
        finally:
            os.close(fd)
        for line in raw.splitlines():
            match = re.search(r'\bmkdir(?:at)?\((?:AT_FDCWD, )?("(?:[^"\\]|\\.)*"), [0-7]+\) = 0$', line)
            if not match:
                continue
            path = Path(unique_json(match[1]))
            if path.parent == Path("/dev/shm") and path.name.startswith("msgq_rust-probe-params-"):
                need(path.name not in self.shm_baseline, "traced namespace existed before this probe")
                self.namespace_creations.add(path)
                if os.path.lexists(path):
                    self.namespace(path)

    def maps(self, rows, allowed):
        observed = set()
        for pid, row in rows.items():
            try:
                need(proc_stat(pid)["start"] == row["start"], "mapping PID reused")
                lines = Path(f"/proc/{pid}/maps").read_text().splitlines()
                for line in lines:
                    fields = line.split(None, 5)
                    if len(fields) != 6 or not fields[5].startswith("/"):
                        continue
                    need(not fields[5].endswith(" (deleted)"), "deleted mapped file")
                    path = Path(fields[5])
                    if self.namespace(path):
                        continue
                    if "x" not in fields[1]:
                        continue
                    canonical = path.resolve(strict=True)
                    if allowed is not None:
                        need(canonical in allowed, f"unbound runtime executable mapping: {canonical}")
                        need(not any(name in canonical.name for name in ("libpython", "ekf_sym_pyx", "libcar.so")),
                             "Python/original runtime mapped by A native process")
                    else:
                        # Cargo의 임시 build-script/output은 이후 합법적으로 재생성된다.
                        # 고정 provider는 즉시 hold하고 최종 native 산출물은 build 뒤 별도 hold한다.
                        if not canonical.is_relative_to(self.root) and not canonical.is_relative_to(self.job):
                            self.holds.add(path, elf=True)
                    info = canonical.stat()
                    major, minor = (int(value, 16) for value in fields[3].split(":"))
                    need((info.st_dev, info.st_ino) == (os.makedev(major, minor), int(fields[4])), "mapping inode differs")
                    observed.add(str(canonical))
            except (FileNotFoundError, ProcessLookupError):
                continue
        return observed

    @staticmethod
    def child_limits(runtime):
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        limit = (16 if runtime else 512) * MIB
        resource.setrlimit(resource.RLIMIT_FSIZE, (limit, limit))

    def command(self, argv, label, seconds=60, env=None, limit=32 * MIB, allowed=None, trace=None):
        need(not self.children.scan(), "unreaped child before command")
        argv = list(map(str, argv))
        before = time.monotonic()
        observed = set()
        error = None
        process = None
        self.budget(force=True)
        with (self.out / f"{label}.stdout").open("xb") as stdout, (self.out / f"{label}.stderr").open("xb") as stderr:
            try:
                process = subprocess.Popen(argv, cwd=self.root, env=env or self.env, start_new_session=True,
                                           stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                           preexec_fn=lambda: self.child_limits(allowed is not None))
                self.children.process = process
                while True:
                    rows = self.children.scan()
                    observed.update(self.maps(rows, allowed))
                    self.learn_namespaces(trace)
                    self.children.reap()
                    need(time.monotonic() <= min(before + seconds, self.deadline), f"command deadline: {label}")
                    need(stdout.tell() <= limit and stderr.tell() <= limit, f"command log bound: {label}")
                    self.budget()
                    if process.returncode is not None:
                        need(not self.children.scan(), f"live adopted/escaped-group child after {label}")
                        need(process.returncode == 0, f"{label}: exit {process.returncode}; preserve stderr")
                        break
                    time.sleep(0.02)
            except BaseException as problem:
                error = str(problem)
                raise
            finally:
                self.children.stop()
                self.learn_namespaces(trace)
                self.commands.append({"label": label, "argv": argv,
                    "returncode": process.returncode if process else None, "error": error,
                    "elapsed_seconds": time.monotonic() - before, "sampled_executable_mappings": sorted(observed),
                    "mapping_scope": "20ms loop samples plus hashing overhead; not complete/pre-dlopen observation",
                    "all_owned_children_reaped": not self.children.scan()})
                self.children.process = None
        self.budget(force=True)
        return read(self.out / f"{label}.stdout", limit)

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
        result = {"index_sha256": sha(raw), "files": snapshot(self.root, paths)}
        save(self.out / f"source-{label}.json", result)
        return result

    def sdk(self, label):
        need(SDK.resolve(strict=True) == SDK, "SDK root alias")
        value = snapshot(SDK, walk(SDK), sdk=True)
        save(self.out / f"sdk-{label}.json", value)
        return value

    def dso(self, program, label):
        raw = self.command([self.tools["ldd"], program], label, limit=MIB).decode()
        paths = set()
        for line in raw.splitlines():
            match = re.fullmatch(r"\s*(?:\S+\s+=>\s+)?(/\S+)\s+\(0x[0-9a-fA-F]+\)\s*", line)
            if match:
                path = Path(match[1]).resolve(strict=True)
                need(any(path.is_relative_to(root) for root in (Path("/usr/lib"), Path("/lib").resolve(), SDK, self.target)),
                     "DSO outside system/SDK/current target")
                paths.add(self.holds.add(Path(match[1]), elf=True))
            else:
                need(re.fullmatch(r"\s*linux-vdso\.so\.\d+\s+\(0x[0-9a-fA-F]+\)\s*", line), "unresolved/opaque ldd output")
        need(paths, "empty dynamic provider closure")
        return paths

    def providers(self):
        helpers = set()
        for name in ("git", "gh", "curl", "clang", "clang++", "cc", "c++", "ar", "as", "ld", "pkg-config", "capnp", "ldd", "strace", "timeout"):
            path = shutil.which(name, path=self.env["PATH"])
            need(path, f"prepared provider missing: {name}; no installation/fallback")
            self.tools[name] = self.holds.add(path)
        for name, pin in SDK_PINS.items():
            self.holds.add(SDK / "bin" / name, pin, elf=True)
        for name in ("rustdoc", "cargo-clippy"):
            self.holds.add(SDK / "bin" / name, elf=True)
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
        version = self.command([SDK / "bin/rustc", "-vV"], "rustc-version").decode()
        need("release: 1.94.0\n" in version and "host: x86_64-unknown-linux-gnu\n" in version, "Rust version/host differs")
        need("GNU coreutils" in self.command([self.tools["timeout"], "--version"], "timeout-version").decode(),
             "GNU timeout provider required")
        programs = set(self.tools.values()) | helpers | {SDK / "bin" / name for name in
                   [*SDK_PINS, "rustdoc", "cargo-clippy"]} | {Path(sys.executable).resolve()}
        for index, path in enumerate(sorted(programs)):
            if os.pread(self.holds.files[path]["fd"], 4, 0) == b"\x7fELF":
                self.dso(path, f"provider-dso-{index}")
        save(self.out / "diagnostic-python.json", {"version": sys.version, "executable": sys.executable,
             "stdlib_only": True, "binding_starts_after_interpreter_stdlib_imports": True,
             "original_python_executions": 0})

    def historical(self):
        token_env = {**self.env, "GH_TOKEN": os.environ["GH_TOKEN"]}
        meta = unique_json(self.command([self.tools["gh"], "api", f"repos/{REPO}/actions/artifacts/12556331299"],
                                       "prior41c-metadata", env=token_env, limit=MIB))
        need(meta["id"] == 12556331299 and meta["size_in_bytes"] == 2539986 and meta["expired"] is False
             and meta["digest"] == "sha256:5ed71546d1a7d50ffc0a94c1b8ba9266267fb2a7772de83d8ebfe4bcc3a2e279"
             and meta["workflow_run"]["id"] == 40082949276
             and meta["workflow_run"]["head_sha"] == "41cfaffd310059f4f999c8d24dd491d9694baa91", "historical artifact metadata differs")
        raw = self.command([self.tools["gh"], "api", f"repos/{REPO}/actions/artifacts/12556331299/zip"],
                           "prior41c", seconds=180, env=token_env, limit=2539987)
        need(len(raw) == 2539986 and sha(raw) == "5ed71546d1a7d50ffc0a94c1b8ba9266267fb2a7772de83d8ebfe4bcc3a2e279", "41cf ZIP pin differs")
        with zipfile.ZipFile(io.BytesIO(raw)) as archive:
            items = archive.infolist()
            need(len(items) == len({item.filename for item in items}) == 114
                 and sum(item.file_size for item in items) <= 64 * MIB, "41cf ZIP roster/bound")
            def member(name, limit=2 * MIB):
                need(archive.getinfo(name).file_size <= limit, "selected member bound")
                return archive.read(name)
            need(member("SOURCE_COMMIT").decode().strip() == "41cfaffd310059f4f999c8d24dd491d9694baa91", "prior source differs")
            for prefix, suffix in (("source", "sha256"), ("index", "bin")):
                need(member(f"{prefix}-before.{suffix}") == member(f"{prefix}-after.{suffix}"), "historical source/index seal differs")
            for name in ("source", "providers", "executables", "captured"):
                need(member(f"paramsd-boundaries/{name}.before.json") == member(f"paramsd-boundaries/{name}.after.json"), "historical final seal differs")
            complete = unique_json(member("paramsd-boundaries/complete.json"))
            need(complete["car_pairs"] == 9 and complete["numpy_cases"] == 9600 and complete["passed_ignored_selectors"] == 2
                 and complete["maintained_atomic_gate_executed"] is False, "prior scope differs")
            sources = unique_json(member("paramsd-boundaries/source.before.json"))
            need(len(sources) == 41, "prior source roster differs")
            for name, row in sources.items():
                relative = PurePosixPath(name)
                need(not relative.is_absolute() and ".." not in relative.parts, "unsafe historical source key")
                if name != "rust/crates/dev-tools/Cargo.toml":
                    self.holds.add(self.root / name, row["sha256"])
            for profile in range(3):
                for fingerprint in ("default", "empty", "custom"):
                    prefix = "paramsd-boundaries/car-params/"
                    need(member(f"{prefix}original-{profile}-{fingerprint}.bin") == member(f"{prefix}native-{profile}-{fingerprint}.bin"), "historical wire pair differs")
            need(sha(member("paramsd-boundaries/numpy-controls.json")) == "47fdf2e8e712161f906aa02cbdbef0506387619a27e4499d87be2199ae7d59b2", "historical NumPy corpus differs")
            for name, selector in (("car-test", "all_cli_profiles_and_fingerprints_match_fresh_python_fixture_bytes"),
                                   ("numpy-test", "matches_fresh_numpy_comparison_boundary_observations")):
                log = member(f"paramsd-boundaries/{name}.stdout").decode()
                need(f"test {selector} ... ok" in log and "1 passed; 0 failed; 0 ignored;" in log, "historical exact test differs")
            oracle = unique_json(member("paramsd-boundaries/oracle.json"))
        manifest = unique_json(self.command([self.tools["gh"], "api", f"repos/{REPO}/contents/rust/crates/dev-tools/Cargo.toml?ref=41cfaffd310059f4f999c8d24dd491d9694baa91"], "old-manifest", env=token_env, limit=MIB))
        old_raw = base64.b64decode(manifest["content"])
        need(sha(old_raw) == sources["rust/crates/dev-tools/Cargo.toml"]["sha256"], "old manifest raw pin differs")
        old = tomllib.loads(old_raw.decode())
        current = tomllib.loads(read(self.root / "rust/crates/dev-tools/Cargo.toml", MIB).decode())
        entries = [("test", {"name": "developer_cli_original", "required-features": ["developer-ssh-native"]}),
                   ("test", {"name": "route_sanitizer_original", "required-features": ["route-sanitizer-native"]}),
                   ("bin", {"name": "prepare-changed-files-context", "path": "src/bin/prepare_changed_files_context.rs", "required-features": ["changed-files-native"]})]
        for kind, entry in entries:
            need(current[kind].count(entry) == 1, "reviewed manifest overlay entry differs")
            current[kind].remove(entry)
        need(old == current, "unreviewed dependency/feature/target change invalidates Paramsd history reuse")
        save(self.out / "reused-scopes.json", {"car_params9_numpy9600_ignored2": {"source": complete["source"],
             "artifact": 12556331299, "historical_oracle": oracle, "rerun": False},
             "estimator1196": {"run": 39866247802, "job": 120129048262, "source": "c5e37a69b34d66129ad0ce7c4d79c58a1921227e",
                "artifact": 12466165545, "zip_sha256": "889828e04ceba8c19d88a4d7e1e3399ac0e8f8df6d4d2a95c72a0329ed252f29",
                "scope": "historical accepted 7cases/1196rows; not current numerical admission", "rerun": False},
             "regular7": {"run": 39844308544, "job": 120055270784, "scope": "historical step9 success; later job compile failure", "rerun": False},
             "continuous1202_restart8": {"run": 39510538880, "job": 118930155954,
                "source": "ba5def0ab46de9cab2d3542af573625761068512", "scope": "historical Ubuntu job success; entire run failed", "rerun": False},
             "startup_B": {"selected": False, "rerun": False}, "old_provider_paths_dereferenced": False})

    def cargo(self, args, label, package="openpilot-dev-tools", features=FEATURES):
        return self.command([SDK / "bin/cargo", *args, "--manifest-path", self.root / "rust/Cargo.toml",
                             "--locked", "-p", package, "--no-default-features", "--features", features], label, seconds=1200)

    def select(self, raw, name, kind, src, tested, package="dev-tools"):
        records = [unique_json(line) for line in raw.splitlines()]
        need(records and all(isinstance(row, dict) and isinstance(row.get("reason"), str) for row in records), "Cargo JSONL record invalid")
        need(all(row["reason"] in ("compiler-artifact", "compiler-message", "build-script-executed", "build-finished")
                 for row in records), "unknown Cargo JSONL reason")
        need(records[-1] == {"reason": "build-finished", "success": True}
             and sum(row["reason"] == "build-finished" for row in records) == 1, "Cargo build-finished invalid/duplicate")
        matches = []
        seen = set()
        for row in records:
            if row["reason"] != "compiler-artifact":
                continue
            key = json.dumps(row, sort_keys=True, allow_nan=False)
            need(key not in seen, "duplicate compiler-artifact record anywhere in JSONL")
            seen.add(key)
            target = row["target"]
            if target["name"] != name or target["kind"] != [kind] or row["profile"]["test"] is not tested:
                continue
            need(Path(row["manifest_path"]) == self.root / f"rust/crates/{package}/Cargo.toml"
                 and Path(target["src_path"]) == self.root / src and isinstance(row["executable"], str), "Cargo package/src/executable differs")
            if package == "dev-tools":
                expected_features = ["eigen-tools-native"] if name == "stage-eigen-headers" else FEATURES.split(",")
                need(sorted(row["features"]) == sorted(expected_features), "unexpected current dev-tools feature closure")
            path = Path(row["executable"])
            need(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(self.target), "Cargo ELF outside current target")
            info = path.lstat()
            need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_mode & 0o111,
                 "current Cargo ELF owner/kind/mode differs")
            self.holds.add(path, elf=True)
            matches.append((path, row))
        need(len(matches) == 1, f"Cargo artifact missing/duplicate: {name}")
        return matches[0]

    def trace(self, binary, args, label, allowed, extra_env=None):
        trace = self.out / f"{label}.exec"
        argv = [str(binary), *map(str, args)]
        env = {**self.env, "PATH": "/nonexistent", **(extra_env or {})}
        self.active_trace = trace
        output = self.command([self.tools["strace"], "-f", "-qq", "-s", "65535", "-e", "trace=execve,execveat,mkdir,mkdirat,rmdir",
                               "-o", trace, *argv], label, seconds=90, env=env, allowed=allowed, trace=trace)
        rows = []
        for line in read(trace).decode().splitlines():
            if "execve" not in line:
                continue
            match = re.search(r'\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$', line)
            need(match and "resumed>" not in line, "failed/opaque/truncated runtime exec")
            rows.append((unique_json(match[1]), unique_json(match[2])))
        need(rows and rows[0] == (str(binary), argv), "actual first exec/argv differs")
        self.holds.add(trace)
        self.holds.verify()
        return output.decode(), rows

    def run(self):
        self.budget(force=True)
        self.sdk_before = self.sdk("before")
        self.providers()
        self.source_before = self.sources("before")
        for name, pin in CORE.items():
            self.holds.add(self.root / name, pin)
        self.historical()
        self.phase = "build"
        self.build_started = True
        self.budget(force=True)
        save(self.out / "growth-build-admission.json", {"source_cap": 2 * GIB, "build_cap": 8 * GIB,
             "proof_cap": 512 * MIB, "current": self.usage,
             "monitor": "500ms sampled allocated/logical bytes; stop/reap on limit, not a filesystem quota"})
        stager_raw = self.cargo(["build", "--bin", "stage-eigen-headers", "--message-format=json"], "eigen-build", features="eigen-tools-native")
        stager, _ = self.select(stager_raw, "stage-eigen-headers", "bin", "rust/crates/dev-tools/src/bin/stage_eigen_headers.rs", False)
        wheel = self.out / "eigen-wheel.stdout"
        wheel_raw = self.command([self.tools["curl"], "--fail", "--location", "--retry", "3", "--max-time", "180",
                 "https://github.com/commaai/dependencies/releases/download/eigen/v3.4.0/eigen-3.4.0-py3-none-linux_x86_64.whl"], "eigen-wheel", seconds=240, limit=2250659)
        need(len(wheel_raw) == 2250658 and sha(wheel_raw) == "0680581b726b6160646d9053cc7416f68c9a38353a26a7f4e4a58222233aa1ba", "Eigen archive pin differs")
        self.holds.add(wheel, sha(wheel_raw))
        self.command([stager, "--wheel", wheel, "--output", self.job / "cache/eigen", "--architecture", "x86_64"], "eigen-stage")
        self.env["PARAMSD_EIGEN_INCLUDE"] = str(self.job / "cache/eigen/install")
        self.eigen_before = snapshot(self.job / "cache/eigen", walk(self.job / "cache/eigen"))
        save(self.out / "eigen-before.json", self.eigen_before)
        daemon_raw = self.cargo(["build", "--bin", "openpilot-paramsd", "--message-format=json"], "daemon-build", "openpilot-paramsd", "native")
        generator_raw = self.cargo(["build", "--bin", "generate-paramsd-car-params", "--message-format=json"], "generator-build")
        tests_raw = self.cargo(["test", "--test", "paramsd_startup_gate", "--test", "paramsd_car_params_cli",
                                "--no-run", "--message-format=json"], "tests-build")
        daemon, daemon_row = self.select(daemon_raw, "openpilot-paramsd", "bin", "rust/crates/paramsd/src/main.rs", False, "paramsd")
        generator, generator_row = self.select(generator_raw, "generate-paramsd-car-params", "bin", "rust/crates/dev-tools/src/bin/generate_paramsd_car_params.rs", False)
        test_generator, _ = self.select(tests_raw, "generate-paramsd-car-params", "bin", "rust/crates/dev-tools/src/bin/generate_paramsd_car_params.rs", False)
        need(generator == test_generator, "test CARGO_BIN_EXE generator path differs")
        gate, gate_row = self.select(tests_raw, "paramsd_startup_gate", "test", "rust/crates/dev-tools/tests/paramsd_startup_gate.rs", True)
        auditor, auditor_row = self.select(tests_raw, "paramsd_car_params_cli", "test", "rust/crates/dev-tools/tests/paramsd_car_params_cli.rs", True)
        need(len({daemon, generator, gate, auditor}) == 4, "four distinct current ELFs required")
        save(self.out / "compiler-artifacts.json", {"daemon": daemon_row, "generator": generator_row, "gate": gate_row, "auditor": auditor_row})
        self.command([SDK / "bin/cargo", "clippy", "--manifest-path", self.root / "rust/Cargo.toml", "--locked",
                      "-p", "openpilot-dev-tools", "--no-default-features", "--features", FEATURES, "--lib", "--bin", "generate-paramsd-car-params",
                      "--test", "paramsd_startup_gate", "--test", "paramsd_car_params_cli", "--", "-D", "warnings"], "clippy", seconds=1200)
        phase = self.out / "atomic"
        phase.mkdir(mode=0o700)
        hook = phase / "startup-phase.so"
        self.command([self.tools["clang"], "-std=c11", "-O2", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror",
                      self.root / "rust/tools/paramsd_startup_phase.c", "-ldl", "-o", hook], "phase-build")
        self.holds.add(hook, elf=True)
        allowed = set(self.holds.files)
        for index, path in enumerate((daemon, generator, gate, auditor, hook, self.tools["strace"])):
            allowed.update(self.dso(path, f"runtime-dso-{index}"))
        allowed.update(self.holds.files)
        save(self.out / "holds-before-runtime.json", self.holds.records())
        self.phase = "runtime-A"
        shm_free = shutil.disk_usage("/dev/shm").free
        save(self.out / "shm-admission.json", {"available_bytes": shm_free, "planned_growth_bytes": 32 * MIB})
        need(shm_free >= 32 * MIB, "isolated msgq tmpfs admission failed")
        car = phase / "CarParams.bin"
        _, execs = self.trace(generator, ["--root", self.root, "--output", car], "car-params", allowed)
        need(len(execs) == 1, "generator must execute exactly once")
        # 기존 maintained auditor가 읽는 trace 이름을 원문 그대로 준비한다.
        shutil.copyfile(self.out / "car-params.exec", phase / "car-params.exec")
        self.holds.add(car)
        self.holds.add(phase / "car-params.exec")
        extra = {"PARAMSD_NATIVE_GATE_DAEMON": str(daemon), "PARAMSD_NATIVE_GATE_LIBRARY": str(hook),
                 "PARAMSD_NATIVE_GATE_CAR_PARAMS": str(car), "PARAMSD_NATIVE_GATE_EVIDENCE": str(phase / "native"),
                 "PARAMSD_CAR_PARAMS_GATE_PROOF": str(phase)}
        for binary, selector, label, filtered in ((gate, FD_TEST, "child-fd", "7"), (auditor, EXEC_TEST, "atomic-exec", "2")):
            args = ["--ignored", "--exact", selector, "--nocapture", "--test-threads=1", "--color=never"]
            output, execs = self.trace(binary, args, label, allowed, extra)
            results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", output)
            need(results == [("1", "0", "0", "0", filtered)]
                 and re.findall(r"^test (\w+) \.\.\. ok$", output, re.M) == [selector], "exact existing ignored test count differs")
            if label == "child-fd":
                need(len(execs) == 2 and execs[1][0] == str(daemon) and len(execs[1][1]) == 3
                     and execs[1][1][:2] == [str(daemon), "--memory-root"]
                     and Path(execs[1][1][2]).is_relative_to(self.tmp), "actual daemon exec/argv differs")
                need(len(self.namespace_creations) == 1, "exactly one actual child-FD namespace mkdir required")
            else:
                need(len(execs) == 1, "maintained auditor unexpectedly executed a child")
        receipt = unique_json(read(phase / "native/result.json", MIB))
        need(receipt["pass"] is True and receipt["daemon_exit"] == 0 and receipt["completed_waits_before_input"] == 1
             and receipt["original_phase_hook_sha256"] == CORE["rust/tools/paramsd_startup_phase.c"]
             and len(set(receipt["child_gate_fds"])) == 2, "native child-FD receipt differs")
        self.runtime = {"exact_ignored_tests": [FD_TEST, EXEC_TEST], "generator_invocations": 1,
                        "daemon_invocations": 1, "runtime_execs": 4, "fresh_regular_tests": 0,
                        "startup_B_executed": False, "whole_numeric_or_runtime_acceptance": False}

    def cleanup_namespaces(self):
        self.learn_namespaces(self.active_trace)
        need(not self.children.scan(), "children must be reaped before namespace cleanup")
        for path, item in self.namespaces.items():
            if not os.path.lexists(path):
                continue
            info = path.lstat()
            held = os.fstat(item["fd"])
            need((info.st_dev, info.st_ino) == (held.st_dev, held.st_ino)
                 and info.st_uid == os.getuid() and stat.S_ISDIR(info.st_mode), "namespace replaced; do not remove")
            need(shutil.rmtree.avoids_symlink_attacks, "FD-safe rmtree required")
            rows = list(walk(path))
            need(len(rows) <= 128 and sum(p.lstat().st_size for p in rows if p.is_file()) <= 32 * MIB,
                 "owned namespace output bound")
            shutil.rmtree(path)
            need(not os.path.lexists(path), "owned namespace cleanup failed")
        need(all(not os.path.lexists(path) for path in self.namespace_creations), "traced namespace remains")
        save(self.out / "namespaces.json", {"created_by_owned_trace": sorted(map(str, self.namespace_creations)),
             "held_directory_identities": {str(path): item["identity"] for path, item in self.namespaces.items()},
             "all_absent": True, "no_other_namespace_removed": True})

    def finish(self, error):
        self.finalizing = True
        self.deadline = time.monotonic() + 180
        failures = []
        for label, operation in (("children", self.children.stop), ("namespaces", self.cleanup_namespaces),
                                 ("held", self.holds.verify),
                                 ("source", lambda: need(self.source_before is not None and self.sources("after") == self.source_before, "source/index seal differs")),
                                 ("sdk", lambda: need(self.sdk_before is not None and self.sdk("after") == self.sdk_before, "SDK seal differs"))):
            try:
                operation()
            except BaseException as problem:
                failures.append(f"{label}: {problem}")
        try:
            need(self.eigen_before is not None, "Eigen baseline unavailable")
            eigen_after = snapshot(self.job / "cache/eigen", walk(self.job / "cache/eigen"))
            save(self.out / "eigen-after.json", eigen_after)
            need(eigen_after == self.eigen_before, "Eigen headers changed")
        except BaseException as problem:
            failures.append(f"eigen: {problem}")
        for index, (root, baseline) in enumerate(self.header_before.items()):
            try:
                after = snapshot(root, walk(root))
                save(self.out / f"system-sdk-{index}-after.json", {"root": str(root), "files": after})
                need(baseline is not None and after == baseline, "system C/C++ SDK/header seal differs")
            except BaseException as problem:
                failures.append(f"system-sdk-{index}: {problem}")
        save(self.out / "commands.json", self.commands)
        save(self.out / "children.json", {"subreaper": True, "tracking": "ancestry/starttime/pidfd; all separate PGIDs included",
             "events": self.children.history, "live": [{k: v for k, v in row.items() if k != "fd"} for row in self.children.scan().values()]})
        save(self.out / "holds-after.json", self.holds.records())
        save(self.out / "final-seals.json", {"pass": not failures, "failures": failures, "source": self.source,
             "all_owned_children_reaped": not self.children.scan(), "phase": self.phase})
        self.budget(force=True)
        if error or failures:
            save(self.out / "failure.json", {"error": error, "final_failures": failures, "phase": self.phase})
            return False
        need(self.usage["proof"] + MIB <= 512 * MIB, "final receipt reserve missing")
        save(self.out / "complete.json", {"schema": "paramsd-A-523-v1", "pass": True, "source": self.source,
             **self.runtime, "original_capture_executed": False, "original_python_executions": 0,
             "car_params9_numpy9600_original1196_regular7_continuous1202_restart8_rerun": False,
             "process_maps_scope": "sampled mappings; no complete/pre-dlopen or whole-FD-roster claim",
             "all_owned_children_reaped": True, "sdk_modified": False, "sources_retired": [],
             "inventory_credit": False, "device_acceptance": False, "growth": self.usage})
        return True


def interrupted(number, _frame):
    raise InterruptedError(f"Paramsd qualification interrupted by signal {number}")


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
                os.close(probe.children.journal)
                for item in probe.namespaces.values():
                    os.close(item["fd"])
    return 0 if success else 1


if __name__ == "__main__":
    raise SystemExit(main())
