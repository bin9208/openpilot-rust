#!/usr/bin/env python3
"""#523 Actions 전용 original109 데이터 재사용. 원본 Python은 실행하지 않는다.

로컬 기능 실행 UNRUN. regular5는 역사적 결과로만 남기고 현재 native CLI 1회와
기존 exact ignored auditor 1개를 실행한다. SDK 설치/원본 재캡처/공유 cache 삭제 없음.
"""

import hashlib
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
import traceback
import zipfile


MIB = 1024**2
REPOSITORY = "bin9208/openpilot-rust"
SDK = Path("/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu")
ORIGINAL_SOURCE = "c5e37a69b34d66129ad0ce7c4d79c58a1921227e"
REVIEWED_SOURCE = "150bd42771ee45046f0c51343bedeef62646c516"
AUDITOR = "fresh_original_full_schema_and_float_bits_match_every_generated_packet"
MANIFEST_SHA = "f8fbe6bf1b4923c7a2b15f651afc5359c4bf5eb856a1db6e19f9077a782a0abe"
DATA = {
    "requests.jsonl": (310125, "eb2e7d93755b76757b6d80f78dcddfba91cf9c88e1c0175b4f0683c7c1e006a9"),
    "original.jsonl": (71583219, "82f4c68cdfc6f34b01ebbb8e555c92cc53abe4a2043f3bf0497ba0585784a9a3"),
    "original-projection.jsonl": (154773853, "e95b1ae6c1eabb9a38a1896d6fa2dd8b5ef954f6b1a24765f234ad848bcb8b39"),
}
MEMBERS = set("""SOURCE_COMMIT dbc-stage.stderr dbc-stage.stdout dbcs.sha256 disk-before.txt
growth-build.json growth-proof.json growth-source.json growth-tools.json memory-before.txt
original-capture-complete.json original-dbcs-after.txt original-manifest.json
original-projection.jsonl original-source-after.txt original-source-index-after.nul
original.jsonl original.stderr original.stdout ownership.json python-packages.txt python.txt
requests.jsonl rustc.txt rustup-installer.sha256 source-contract.json source-index-before.nul
source-paths.nul source.sha256""".split())
ORIGINAL_PINS = {
    "rust/tools/controlsd_fixture.py": "df3fe9adc8b9ed195927700de5235fd88df58c0c80a617a5e043c5d5283fbb45",
    "rust/tools/controlsd_scenarios.py": "d83dfce64a13d033143fccbeb03bd4a270dd3837f759ad58a3bdca50fe472368",
    "rust/tools/paramsd_fixture.py": "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521",
}
CORE = {
    "src/controlsd_fixture_generate.rs": "bd43c2fe8db4243888395d035145ab574bd1a2f52f72aaeee6ac26220684c8ef",
    "src/controlsd_fixture_generate/audit.rs": "9c455a724c5f4fcee7616197fbb057e2eda737c4810be0f758cf0a881538f58c",
    "src/controlsd_fixture_generate/parameters.rs": "eecf9b4061858547b6081db26c2b614018e9e71b67b64084045cb2aefbb7827d",
    "src/controlsd_fixture_generate/recipes.rs": "ef36f1cdee47cc3ecf30f50f311b633812a3f9799ae652bcd15fcc14d6976385",
    "src/controlsd_fixture_generate/wire.rs": "554144bdc2f57e61ea0569130c36783ff3bb14c20a608ab61584207f99e39569",
    "src/bin/generate_controlsd_fixtures.rs": "4a4f00b454090cb367c9a3023799e263a21129f9a71581fe696ab840465a096f",
    "tests/controlsd_fixture_generate.rs": "a322a9b20ebfe9bd62abea5ce6511657a7f613b07258bf7baf0758146d1a9658",
}
AUDIT_PREFIX_SHA = "6fc8f02115d71b5a0138f2b0cf8c78ebad4d943576b124e2aa7515414741686e"
STREAM_TESTS = {
    "controlsd_fixture_generate::audit::tests::cases_cursor_splits_nested_escaped_and_nonfinite_values",
    "controlsd_fixture_generate::audit::tests::streamed_cases_preserve_projection_float_bits_and_packet_counts",
    "controlsd_fixture_generate::audit::tests::streamed_cases_reject_incomplete_rosters_syntax_and_late_differences",
}
SDK_PINS = {
    "rustc": "103b60e1b1339968c1d74202ea1d45686037e82c4ea3e0569de24b18a1e6836a",
    "cargo": "77f14b761b02b47e6747473f556b3bc9f98f7e4525b7c3b8d74898ff816e4636",
    "rustfmt": "d1c6d8c0e2fae4fbb686d421415d2f471e409ade6babb3e491d9ac5d9c70c602",
    "clippy-driver": "2ff45d07196eec4f57a631103a877631d3579e86db2770bcfaa3b9d92a0b9d15",
}
REGULAR_TESTS = {
    "frame_preserves_actual_service_order_and_pose_builder_bytes",
    "change_shallowly_replaces_nested_objects_and_keeps_other_packets_exact",
    "event_is_full_schema_but_service_api_is_bounded",
    "psa_parameters_use_real_synthetic_car_and_apply_torque_changes",
    "cli_rejects_existing_output_and_invalid_arguments",
}


def need(value, message):
    if not value:
        raise RuntimeError(message)


def identity(info):
    return [getattr(info, "st_" + name) for name in
            ("dev", "ino", "mode", "uid", "gid", "size", "nlink", "mtime_ns", "ctime_ns")]


def digest_fd(fd):
    value = hashlib.sha256()
    offset = 0
    while chunk := os.pread(fd, MIB, offset):
        value.update(chunk)
        offset += len(chunk)
    return value.hexdigest()


def regular(path, limit=512 * MIB):
    before = path.lstat()
    need(stat.S_ISREG(before.st_mode) and before.st_size <= limit, f"regular file/bound: {path}")
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        need(identity(os.fstat(fd)) == identity(before), f"file replaced: {path}")
        digest = digest_fd(fd)
        need(identity(os.fstat(fd)) == identity(before) == identity(path.lstat()), f"file changed: {path}")
        return {"identity": identity(before), "sha256": digest}
    finally:
        os.close(fd)


def read(path, limit):
    seal = regular(path, limit)
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    need(len(data) <= limit and hashlib.sha256(data).hexdigest() == seal["sha256"], f"read changed: {path}")
    return data


def write(path, data):
    need(len(data) <= 512 * MIB, f"proof file bound: {path}")
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data)


def save(path, value):
    write(path, (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode())


def interrupted(number, _frame):
    # 외부 timeout/Actions 취소도 현재 자식 process group을 정리하는 경로로 보낸다.
    raise InterruptedError(f"qualification interrupted by signal {number}")


def runtime_file_limit():
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * MIB, 512 * MIB))


def walk_error(error):
    raise error


def paths_under(root):
    for base, dirs, files in os.walk(root, followlinks=False, onerror=walk_error):
        need(len(Path(base).relative_to(root).parts) <= 64, f"directory depth: {base}")
        for name in sorted(dirs + files):
            yield Path(base) / name
        dirs[:] = sorted(name for name in dirs if not (Path(base) / name).is_symlink())


def snapshot(root, paths):
    result = {}
    total = 0
    for path in paths:
        need(len(result) < 65000 and path.is_relative_to(root), "seal roster bound/root")
        info = path.lstat()
        row = {"identity": identity(info)}
        if stat.S_ISREG(info.st_mode):
            total += info.st_size
            need(total <= 8 * 1024**3, "seal input bytes exceed 8 GiB")
            row.update(regular(path, 2 * 1024**3))
        elif stat.S_ISLNK(info.st_mode):
            row["symlink"] = os.readlink(path)
        else:
            need(stat.S_ISDIR(info.st_mode), f"special source/provider: {path}")
        result[str(path.relative_to(root))] = row
    return result


class Holds:
    def __init__(self):
        self.files = {}

    def add(self, alias, *, owned=False, elf=False, expected=None):
        alias = Path(alias)
        path = alias.resolve(strict=True)
        if path not in self.files:
            need(len(self.files) < 4096, "held file count exceeds 4096")
            seal = regular(path, 2 * 1024**3)
            fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
            need(identity(os.fstat(fd)) == seal["identity"], f"held open changed: {path}")
            self.files[path] = {"fd": fd, "seal": seal, "aliases": set()}
        held = self.files[path]
        held["aliases"].add(alias)
        info = os.fstat(held["fd"])
        need(not owned or info.st_uid == os.getuid(), f"foreign owned file: {path}")
        if elf:
            header = os.pread(held["fd"], 64, 0)
            need(len(header) == 64 and header[:7] == b"\x7fELF\x02\x01\x01"
                 and int.from_bytes(header[18:20], "little") == 62, f"x86_64 ELF64: {path}")
        need(expected is None or held["seal"]["sha256"] == expected, f"pin differs: {alias}")
        return path

    def records(self):
        return {str(path): {**item["seal"], "aliases": sorted(map(str, item["aliases"]))}
                for path, item in sorted(self.files.items())}

    def verify(self):
        for path, item in self.files.items():
            need(identity(os.fstat(item["fd"])) == item["seal"]["identity"], f"held identity changed: {path}")
            need(regular(path, 2 * 1024**3) == item["seal"], f"held bytes/path changed: {path}")
            need(digest_fd(item["fd"]) == item["seal"]["sha256"], f"held descriptor changed: {path}")
            for alias in item["aliases"]:
                need(alias.resolve(strict=True) == path, f"provider alias changed: {alias}")

    def close(self):
        for item in self.files.values():
            os.close(item["fd"])


class Probe:
    def __init__(self):
        expected = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": REPOSITORY,
                    "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/codex/feat-523-python-rust",
                    "RUNNER_NAME": "bin-actions", "RUNNER_ENVIRONMENT": "self-hosted",
                    "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64"}
        need(all(os.environ.get(key) == value for key, value in expected.items()), "registered Actions identity required")
        need(os.uname().machine == "x86_64", "x86_64 host required")
        self.expected = os.environ["GITHUB_SHA"]
        need(re.fullmatch(r"[0-9a-f]{40}", self.expected), "exact source SHA required")
        self.root = Path(os.environ["DIAG_SOURCE"])
        self.job = Path(os.environ["CI_JOB_TEMP"])
        self.out = self.job / "proof/controls109"
        self.target = self.root / "rust/target"
        self.owner = ":".join(os.environ[key] for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB"))
        for path in (self.root, self.job, self.out.parent):
            info = path.lstat()
            need(path.is_absolute() and path.resolve(strict=True) == path and stat.S_ISDIR(info.st_mode)
                 and info.st_uid == os.getuid(), f"owned canonical directory: {path}")
        need(read(self.job / "owner", 512).decode().strip() == self.owner, "job owner differs")
        need(read(self.root.parent / ".diag296-owner", 512).decode().strip() == self.owner, "source owner differs")
        need(Path(os.environ["CARGO_TARGET_DIR"]) == self.target, "private target differs")
        self.out.mkdir(mode=0o700)
        self.holds = Holds()
        self.commands = []
        self.phase = "admission"
        self.source_before = self.sdk_before = None
        self.runtime = {}
        self.tools = {}
        self.command_number = 0
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        need(hard == resource.RLIM_INFINITY or hard >= 4352, "held proof needs at least 4352 descriptors")
        resource.setrlimit(resource.RLIMIT_NOFILE, (max(soft, 4352), hard))
        self.env = os.environ.copy()
        for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET", "RUSTC_WRAPPER",
                    "RUSTC_WORKSPACE_WRAPPER", "LD_PRELOAD", "LD_LIBRARY_PATH", "PYTHONPATH", "PYTHONHOME",
                    "CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS"):
            self.env.pop(key, None)
        self.env.update(RUSTC=str(SDK / "bin/rustc"), RUSTDOC=str(SDK / "bin/rustdoc"),
                        CARGO_TARGET_DIR=str(self.target), CARGO_BUILD_JOBS="2", CARGO_INCREMENTAL="0",
                        CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_TERM_COLOR="never",
                        LC_ALL="C", CONTROLSD_FIXTURE_PROOF=str(self.out))

    def budget(self, *, extra_bytes=0, extra_files=0):
        total = count = 0
        for path in paths_under(self.job / "proof"):
            info = path.lstat()
            if stat.S_ISDIR(info.st_mode):
                continue
            need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_nlink == 1,
                 f"unsafe proof output: {path}")
            total += info.st_size
            count += 1
            need(info.st_size <= 512 * MIB and total <= 1024 * MIB and count <= 1024, "raw proof bound exceeded")
        need(total + extra_bytes <= 1024 * MIB and count + extra_files <= 1024, "pending proof output bound")

    def growth(self, label, path, mib):
        while not path.exists():
            path = path.parent
        free = shutil.disk_usage(path).free
        save(self.out / f"growth-{label}.json", {"path": str(path), "available_bytes": free,
                                              "planned_growth_bytes": mib * MIB})
        need(free >= mib * MIB, f"{label}: {free} available < {mib * MIB} planned bytes")

    def resources(self, label):
        usages = {}
        for name, path in (("source", self.root.parent), ("registry", Path(self.env["CARGO_HOME"])),
                           ("target", self.target), ("proof", self.job / "proof")):
            allocated = {}
            if path.exists():
                for item in (path, *paths_under(path)):
                    info = item.lstat()
                    allocated[(info.st_dev, info.st_ino)] = info.st_blocks * 512
            usages[name] = {"path": str(path), "allocated_bytes": sum(allocated.values()),
                            "free_bytes": shutil.disk_usage(path if path.exists() else path.parent).free}
        cgroup = {}
        for name in ("memory.current", "memory.peak", "memory.events", "pids.current"):
            path = Path("/sys/fs/cgroup") / name
            if path.is_file():
                cgroup[name] = path.read_text()[:65536]
        save(self.out / f"resources-{label}.json", {"paths": usages, "cgroup": cgroup,
             "maxrss_self_kib": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
             "maxrss_children_kib": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
             "limit": "snapshots; shared cgroup counters are not exclusively attributable to this probe"})

    @staticmethod
    def group_alive(pid):
        try:
            os.killpg(pid, 0)
            return True
        except ProcessLookupError:
            return False

    def maps(self, group, allowed, observed):
        for proc in Path("/proc").iterdir():
            if not proc.name.isdigit():
                continue
            try:
                if proc.stat().st_uid != os.getuid() or os.getpgid(int(proc.name)) != group:
                    continue
                for line in (proc / "maps").read_text().splitlines():
                    fields = line.split(None, 5)
                    if len(fields) != 6 or "x" not in fields[1] or not fields[5].startswith("/"):
                        continue
                    need(not fields[5].endswith(" (deleted)"), "deleted executable mapping")
                    path = Path(fields[5]).resolve(strict=True)
                    need(path in allowed, f"unbound runtime executable mapping: {path}")
                    major, minor = (int(part, 16) for part in fields[3].split(":"))
                    info = path.stat()
                    need((info.st_dev, info.st_ino) == (os.makedev(major, minor), int(fields[4])),
                         f"runtime mapping inode differs: {path}")
                    observed.add(str(path))
            except (FileNotFoundError, ProcessLookupError):
                continue

    def command(self, argv, label, *, seconds=60, data=None, env=None, allowed=None, limit=32 * MIB):
        self.command_number += 1
        stdout = self.out / f"{label}.stdout"
        stderr = self.out / f"{label}.stderr"
        argv = list(map(str, argv))
        observed = set()
        started = time.monotonic()
        status = None
        problem = None
        with stdout.open("xb") as out, stderr.open("xb") as err:
            process = subprocess.Popen(argv, cwd=self.root, env=env or self.env, start_new_session=True,
                                       stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                       stdout=out, stderr=err,
                                       preexec_fn=runtime_file_limit if allowed is not None else None)
            try:
                if data is not None:
                    process.stdin.write(data)
                    process.stdin.close()
                while process.poll() is None:
                    need(time.monotonic() - started <= seconds, f"process deadline: {label}")
                    need(out.tell() <= limit and err.tell() <= limit, f"command output bound: {label}")
                    self.budget()
                    if allowed is not None:
                        self.maps(process.pid, allowed, observed)
                    time.sleep(0.05)
                status = process.wait()
                need(not self.group_alive(process.pid), f"live descendant after command: {label}")
                need(status == 0, f"command {label} exited {status}; see raw stderr")
            except BaseException as error:
                problem = str(error)
                if self.group_alive(process.pid):
                    os.killpg(process.pid, signal.SIGTERM)
                    deadline = time.monotonic() + 10
                    while self.group_alive(process.pid) and time.monotonic() < deadline:
                        process.poll()
                        time.sleep(0.05)
                    if self.group_alive(process.pid):
                        os.killpg(process.pid, signal.SIGKILL)
                status = process.wait()
                raise
            finally:
                self.commands.append({"label": label, "argv": argv, "returncode": status,
                    "elapsed_seconds": time.monotonic() - started, "problem": problem,
                    "observed_executable_mappings": sorted(observed),
                    "mapping_scope": "sampled executable file mappings; not pre-dlopen or complete data mappings"})
        need(stdout.stat().st_size <= limit and stderr.stat().st_size <= limit, f"command output bound: {label}")
        self.budget()
        return read(stdout, limit)

    def git(self, args, label, **kwargs):
        return self.command([self.tools["git"], "-C", self.root, *args], label, **kwargs)

    def sources(self, label):
        need(self.git(["rev-parse", "HEAD"], f"head-{label}").decode().strip() == self.expected, "source HEAD differs")
        self.git(["diff", "--exit-code", "HEAD", "--"], f"diff-{label}")
        raw = self.git(["ls-files", "--stage", "-z"], f"index-{label}")
        paths = []
        for record in raw.split(b"\0"):
            if not record:
                continue
            metadata, name = record.split(b"\t", 1)
            mode, _, stage = metadata.decode().split()
            need(stage == "0" and mode != "160000", "unmerged/submodule source is outside complete seal")
            relative = PurePosixPath(name.decode())
            need(not relative.is_absolute() and ".." not in relative.parts, "unsafe index path")
            paths.append(self.root / str(relative))
        value = {"index_sha256": hashlib.sha256(raw).hexdigest(), "files": snapshot(self.root, paths)}
        save(self.out / f"source-{label}.json", value)
        return value

    def sdk(self, label):
        need(SDK.resolve(strict=True) == SDK and SDK.stat().st_uid == os.getuid(), "prepared SDK root differs")
        value = snapshot(SDK, [SDK, *paths_under(SDK)])
        save(self.out / f"sdk-{label}.json", value)
        return value

    def providers(self):
        for name in ("git", "gh", "capnp", "cc", "c++", "ar", "as", "ld", "pkg-config", "ldd", "strace", "timeout"):
            path = shutil.which(name, path=self.env.get("PATH"))
            need(path, f"prepared provider missing: {name}; installation/fallback prohibited")
            self.tools[name] = self.holds.add(Path(path))
        self.holds.add(Path(sys.executable), elf=True)
        for module in tuple(sys.modules.values()):
            name = getattr(module, "__file__", None)
            if name and Path(name).is_file():
                self.holds.add(Path(name))
        for line in Path("/proc/self/maps").read_text().splitlines():
            fields = line.split(None, 5)
            if len(fields) == 6 and fields[5].startswith("/"):
                need(not fields[5].endswith(" (deleted)"), "deleted Python provider mapping")
                self.holds.add(Path(fields[5]))
        for name, expected in SDK_PINS.items():
            self.holds.add(SDK / "bin" / name, owned=True, expected=expected)
        for name in ("cargo-clippy", "rustdoc"):
            self.holds.add(SDK / "bin" / name, owned=True, elf=True)
        version = self.command([SDK / "bin/rustc", "-vV"], "rustc-version").decode()
        need("release: 1.94.0\n" in version and "host: x86_64-unknown-linux-gnu\n" in version,
             "prepared compiler version/host differs")
        self.command([self.tools["capnp"], "--version"], "capnp-version")
        timeout = self.command([self.tools["timeout"], "--version"], "timeout-version").decode()
        need("GNU coreutils" in timeout, "GNU timeout required")
        # GCC의 실제 보조 provider를 경로 query로 먼저 고정한다. 설치/다른 compiler fallback은 없다.
        for driver, programs in (("cc", ("cc1", "collect2")), ("c++", ("cc1plus",))):
            for program in programs:
                value = self.command([self.tools[driver], "-print-prog-name=" + program],
                                     f"provider-{program}", limit=65536).decode().strip()
                path = Path(value) if value.startswith("/") else Path(shutil.which(value, path=self.env.get("PATH")) or value)
                need(path.is_absolute(), f"unresolved current C compiler provider: {program}")
                self.holds.add(path, elf=True)
        programs = sorted(set(self.tools.values()) | {SDK / "bin/rustc", SDK / "bin/cargo", SDK / "bin/clippy-driver"}
                          | {path for path in self.holds.files if path.name in ("cc1", "cc1plus", "collect2")})
        for index, path in enumerate(programs):
            with path.open("rb") as stream:
                is_elf = stream.read(4) == b"\x7fELF"
            if is_elf:
                self.dso(path, f"build-provider-{index}-ldd")
        save(self.out / "diagnostic-python.json", {"executable": sys.executable, "version": sys.version,
             "original_python_executions": 0, "binding_starts_after_interpreter_and_stdlib_imports": True})

    def fetch(self, artifact, run, head, size, digest, name):
        meta = json.loads(self.command([self.tools["gh"], "api", f"repos/{REPOSITORY}/actions/artifacts/{artifact}"],
                                      f"{name}-artifact-api", limit=MIB))
        result = json.loads(self.command([self.tools["gh"], "api", f"repos/{REPOSITORY}/actions/runs/{run}"],
                                        f"{name}-run-api", limit=MIB))
        need(meta["id"] == artifact and meta["size_in_bytes"] == size and meta["expired"] is False
             and meta["digest"] == "sha256:" + digest and meta["workflow_run"]["id"] == run
             and meta["workflow_run"]["head_sha"] == head, "historical artifact metadata differs")
        need(result["id"] == run and result["head_sha"] == head and result["status"] == "completed"
             and result["conclusion"] == "failure" and result["repository"]["full_name"] == REPOSITORY,
             "historical run identity/outcome differs")
        path = self.out / f"{name}-download.stdout"
        self.command([self.tools["gh"], "api", f"repos/{REPOSITORY}/actions/artifacts/{artifact}/zip"],
                     f"{name}-download", limit=size + 1)
        seal = regular(path, size + 1)
        need(path.stat().st_size == size and seal["sha256"] == digest, "historical ZIP bytes differ")
        self.holds.add(path, owned=True, expected=digest)
        return path

    @staticmethod
    def zip_roster(archive, count, total=None, *, no_executable=False):
        records = archive.infolist()
        need(len(records) == len({item.filename for item in records}) == count, "ZIP member count/duplicate")
        expanded = sum(item.file_size for item in records)
        need(expanded < 256 * MIB and (total is None or expanded == total), "ZIP expanded byte bound")
        for item in records:
            name = item.filename
            path = PurePosixPath(name)
            mode = item.external_attr >> 16
            need(name and not path.is_absolute() and path.as_posix() == name and "\\" not in name
                 and all(part not in ("", ".", "..") for part in name.split("/"))
                 and not item.is_dir() and not stat.S_ISLNK(mode)
                 and (not no_executable or not mode & 0o111),
                 "unsafe/non-inert ZIP member")
        return records

    def original(self):
        path = self.fetch(12462355309, 39859011541, "4c35e0ebab5802027653518608260b7850c92a6f", 8211608,
                          "8a3a8f5997825fdc637db941348a24087a0763382ee4471653257e22ccb4e558", "original")
        history = self.out / "historical-original"
        history.mkdir(mode=0o700)
        with zipfile.ZipFile(path) as archive:
            records = self.zip_roster(archive, 29, no_executable=True)
            need({item.filename for item in records} == MEMBERS and archive.testzip() is None, "original ZIP roster/CRC")
            for item in records:
                destination = self.out if item.filename in DATA or item.filename == "original-manifest.json" else history
                fd = os.open(destination / item.filename, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o400)
                with archive.open(item) as source, os.fdopen(fd, "wb") as target:
                    shutil.copyfileobj(source, target, MIB)
        manifest_path = self.out / "original-manifest.json"
        need(regular(manifest_path)["sha256"] == MANIFEST_SHA, "original manifest pin")
        manifest = json.loads(read(manifest_path, 8 * MIB))
        checkpoint = json.loads(read(history / "original-capture-complete.json", 8 * MIB))
        need(manifest["source_commit"] == checkpoint["source_commit"] == ORIGINAL_SOURCE
             and read(history / "SOURCE_COMMIT", 41).decode().strip() == ORIGINAL_SOURCE, "original source attribution")
        need(type(manifest["requests"]) is int and manifest["requests"] == 109
             and manifest["constructor_cases"] == 30 and manifest["constructor_frames"] == 8605
             and manifest["native_output_used_as_expected"] is False
             and manifest["scope"] == "original input constructors only; no controller/daemon/model execution",
             "original corpus scope/count")
        for key in ("original_capture_completed", "source_index_unchanged", "source_and_dbc_seals_rechecked"):
            need(checkpoint[key] is True, f"capture checkpoint: {key}")
        need(checkpoint["native_fixture_comparison_completed"] is False
             and checkpoint["original_manifest"]["sha256"] == MANIFEST_SHA, "original capture is not native acceptance")
        need(read(history / "source-index-before.nul", 16 * MIB) ==
             read(history / "original-source-index-after.nul", 16 * MIB), "original index changed")
        for name in ("original-source-after.txt", "original-dbcs-after.txt"):
            lines = read(history / name, 16 * MIB).decode().splitlines()
            need(lines and all(line.endswith(": OK") for line in lines), "original source/DBC seal failed")
        need(set(manifest["files"]) == set(DATA), "original file roster")
        for name, (size, digest) in DATA.items():
            value = regular(self.out / name)
            need(value["sha256"] == digest and value["identity"][5] == size, f"original data pin: {name}")
            with (self.out / name).open("rb") as stream:
                lines = sum(chunk.count(b"\n") for chunk in iter(lambda: stream.read(MIB), b""))
            need(lines == 109 and manifest["files"][name] == {"sha256": digest, "bytes": size}
                 and checkpoint["files"][name] == {"sha256": digest, "bytes": size, "lines": 109}, "109 data records/seals")
        need(manifest["sources"] == ORIGINAL_PINS == json.loads(read(history / "source-contract.json", MIB)),
             "original source contract")
        old_root = PurePosixPath(json.loads(read(history / "ownership.json", MIB))["source"])
        need(old_root.is_absolute() and len(manifest["loaded_project_modules"]) == 177, "original mapped source roster")
        mapped = {}
        for old_path, digest in manifest["loaded_project_modules"].items():
            relative = PurePosixPath(old_path).relative_to(old_root)
            name = str(relative)
            need(relative.parts and ".." not in relative.parts and "\n" not in name and name not in mapped,
                 "unsafe/duplicate historical source remap")
            current = self.root / name
            need(current.resolve(strict=True) == current and current.is_relative_to(self.root), "mapped source alias")
            self.holds.add(current, owned=True, expected=digest)
            mapped[name] = digest
        queries = "".join(f"{self.expected}:{name}\n" for name in mapped).encode()
        blobs = self.git(["cat-file", "--batch"], "mapped-current-git-blobs", data=queries)
        offset = 0
        for name, digest in mapped.items():
            end = blobs.index(b"\n", offset)
            header = blobs[offset:end].split()
            need(len(header) == 3 and header[1] == b"blob", f"missing current Git source: {name}")
            length = int(header[2]); offset = end + 1
            need(hashlib.sha256(blobs[offset:offset + length]).hexdigest() == digest
                 and blobs[offset + length:offset + length + 1] == b"\n", f"current Git blob differs: {name}")
            offset += length + 1
        need(offset == len(blobs), "extra Git source batch output")
        counts = {"providers": 38, "preload_extension_providers": 12, "loaded_project_modules": 177}
        need(checkpoint["provider_counts"] == counts, "historical provider counts")
        need(all(len(manifest[key]) == count for key, count in counts.items())
             and checkpoint["provider_binding_limit"] == manifest["provider_binding_limit"], "historical provider binding")
        ops = {}
        for line in read(self.out / "requests.jsonl", MIB).splitlines():
            value = json.loads(line)
            ops[value["op"]] = ops.get(value["op"], 0) + 1
        need(ops == {"frame": 49, "change": 34, "parameters": 21, "case": 1, "event": 3, "cases": 1}, "original operation roster")
        for name in MEMBERS:
            file = self.out / name if name in DATA or name == "original-manifest.json" else history / name
            self.holds.add(file, owned=True)
        save(self.out / "original-reused.json", {"original_run": 39859011541, "artifact": 12462355309,
             "original_source": ORIGINAL_SOURCE, "manifest_sha256": MANIFEST_SHA, "requests": 109,
             "constructor_cases": 30, "constructor_frames": 8605, "current_mapped_sources": mapped,
             "historical_provider_counts": counts, "historical_provider_binding_limit": manifest["provider_binding_limit"],
             "historical_provider_paths_dereferenced": False, "original_capture_rerun": False})

    def regular_history(self):
        path = self.fetch(12472101435, 39876347504, "c3d7d46a91ca69cd86792bc50dcc51a651377449", 9259218,
                          "d81c515c092450c8694e4ab5382577429bfa2ad59344ff4b0417e02d65bc62dc", "regular5")
        job = json.loads(self.command([self.tools["gh"], "api", f"repos/{REPOSITORY}/actions/jobs/120162770498"],
                                     "regular5-job-api", limit=MIB))
        need(job["id"] == 120162770498 and job["run_id"] == 39876347504 and job["runner_name"] == "bin-actions"
             and job["runner_id"] == 22 and job["head_sha"] == "c3d7d46a91ca69cd86792bc50dcc51a651377449"
             and job["status"] == "completed" and job["conclusion"] == "failure", "historical regular5 actual job")
        history = self.out / "historical-regular-tests"
        history.mkdir(mode=0o700)
        with zipfile.ZipFile(path) as archive:
            self.zip_roster(archive, 61, 237113522)
            for name, bound in (("SOURCE_COMMIT", 41), ("focused-tests.log", 32768)):
                need(0 < archive.getinfo(name).file_size <= bound, "selected historical member bound")
                write(history / name, archive.read(name))
        need(read(history / "SOURCE_COMMIT", 41).decode().strip() == "55084e65ab94b71ba884ec11e2ce91eb8127bd71",
             "historical five-pass source")
        raw = read(history / "focused-tests.log", 32768)
        need(hashlib.sha256(raw).hexdigest() == "9b04713619801591cd20da6afccf567bbcce7d1ab57a8bf7ff5e3cc47e5e3ad1", "historical test log pin")
        text = raw.decode()
        tests = re.findall(r"^test (\w+) \.\.\. ok$", text, re.M)
        need(len(tests) == 5 and set(tests) == REGULAR_TESTS, "historical five test names")
        need(re.findall(r"^test (\w+) \.\.\. ignored,.*$", text, re.M) == [AUDITOR]
             and re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", text)
             == [("5", "0", "1", "0", "0")], "historical five-pass result")
        self.holds.add(history / "SOURCE_COMMIT", owned=True)
        self.holds.add(history / "focused-tests.log", owned=True)
        audit = read(self.root / "rust/crates/dev-tools/src/controlsd_fixture_generate/audit.rs", MIB)
        need(hashlib.sha256(audit.split(b"// Frame only", 1)[0].rstrip(b"\n")).hexdigest() == AUDIT_PREFIX_SHA,
             "historical normalization/comparison/wire contracts changed")
        save(self.out / "regular5-reused.json", {"run": 39876347504, "job": 120162770498, "artifact": 12472101435,
             "historical_source": "55084e65ab94b71ba884ec11e2ce91eb8127bd71", "reviewed_source": REVIEWED_SOURCE,
             "current_core7_equal_reviewed_source": False, "current_core7": CORE,
             "current_core6_equal_reviewed_source": True, "auditor_stable_prefix_sha256": AUDIT_PREFIX_SHA,
             "current_auditor_change": "cases-only bounded streaming with existing comparison functions",
             "current_cargo_lib_and_dependency_bindings": "fresh source seal and current build/Clippy; not the old fixture150 tree",
             "historical_passed": 5, "historical_ignored_not_passed": 1, "fresh_regular_test_invocations": 0,
             "fresh_source_regular5_acceptance_claimed": False, "archived_code_or_elf_executed": False})

    def cargo(self, args, label, *, seconds=1500):
        return self.command([SDK / "bin/cargo", *args, "--manifest-path", "rust/Cargo.toml", "--locked",
                             "-p", "openpilot-dev-tools", "--no-default-features", "--features", "controlsd-fixture-native"],
                            label, seconds=seconds)

    def select(self, raw, name, kind, source, tested):
        records = [json.loads(line) for line in raw.splitlines()]
        need(records and all(isinstance(row, dict) and isinstance(row.get("reason"), str) for row in records), "Cargo JSONL records")
        need(records[-1] == {"reason": "build-finished", "success": True}, "Cargo build-finished missing/failed")
        matches = []
        for row in records:
            if row["reason"] != "compiler-artifact":
                continue
            target = row["target"]
            if target["name"] != name or target["kind"] != [kind] or row["profile"]["test"] is not tested:
                continue
            need(Path(row["manifest_path"]) == self.root / "rust/crates/dev-tools/Cargo.toml"
                 and Path(target["src_path"]) == self.root / source and row["executable"] is not None,
                 "Cargo artifact source/package/executable differs")
            need("controlsd-fixture-native" in row["features"], "Cargo feature absent")
            path = Path(row["executable"])
            need(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(self.target),
                 "Cargo executable outside current private target")
            matches.append((path, row))
        need(len(matches) == 1, f"Cargo artifact missing/duplicate: {name}/{kind}/{tested}")
        return matches[0]

    def dso(self, path, label):
        output = self.command([self.tools["ldd"], path], label, limit=MIB).decode()
        providers = []
        need("not found" not in output, "unresolved current DSO")
        for line in output.splitlines():
            match = re.fullmatch(r"\s*(?:\S+\s+=>\s+)?(/\S+)\s+\(0x[0-9a-fA-F]+\)\s*", line)
            if match:
                provider = Path(match[1]).resolve(strict=True)
                need(any(provider.is_relative_to(root) for root in (Path("/usr/lib"), Path("/lib").resolve(), SDK, self.target)),
                     "current DSO outside system/SDK/current target")
                providers.append(self.holds.add(Path(match[1]), elf=True))
            else:
                need(re.fullmatch(r"\s*linux-vdso\.so\.\d+\s+\(0x[0-9a-fA-F]+\)\s*", line), "unrecognized ldd output")
        need(providers, "empty current DSO closure")
        return set(providers)

    def trace(self, binary, args, label, allowed, *, require_mapped=True):
        trace = self.out / f"{label}.exec"
        argv = [str(binary), *map(str, args)]
        env = {key: value for key, value in self.env.items() if key not in ("GH_TOKEN", "GITHUB_TOKEN", "ACTIONS_RUNTIME_TOKEN")}
        env["PATH"] = "/nonexistent"
        output = self.command([self.tools["timeout"], "--signal=TERM", "--kill-after=10s", "1200s",
                               self.tools["strace"], "-f", "-qq", "-s", "65535", "-e", "trace=execve,execveat",
                               "-o", trace, *argv], label, seconds=1220, env=env, allowed=allowed, limit=32 * MIB)
        execs = []
        for line in read(trace, 16 * MIB).decode().splitlines():
            if "execve" not in line:
                continue
            match = re.search(r'\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$', line)
            need(match and "resumed>" not in line, f"unexpected/truncated/failed exec attempt: {label}")
            execs.append((json.loads(match[1]), json.loads(match[2])))
        need(execs == [(str(binary), argv)], f"unexpected runtime exec roster: {label}")
        observed = self.commands[-1]["observed_executable_mappings"]
        need(not require_mapped or str(binary) in observed, f"actual runtime ELF mapping not observed: {label}")
        self.holds.verify()
        return output

    def run(self):
        self.phase = "proof-growth"
        self.growth("proof", self.job, 2048)
        self.sdk_before = self.sdk("before")
        self.providers()
        self.source_before = self.sources("before")
        for name, digest in CORE.items():
            self.holds.add(self.root / "rust/crates/dev-tools" / name, owned=True, expected=digest)
        for name, digest in ORIGINAL_PINS.items():
            self.holds.add(self.root / name, owned=True, expected=digest)
        self.phase = "original-data-reuse"
        self.original()
        self.regular_history()
        self.env.pop("GH_TOKEN", None)
        self.env.pop("GITHUB_TOKEN", None)
        self.resources("prepared")
        self.phase = "native-build"
        self.growth("build", self.root, 8192)
        if self.root.stat().st_dev != self.job.stat().st_dev:
            self.growth("build-registry-filesystem", self.job, 8192)
        flags = ["--bin", "generate-controlsd-fixtures", "--message-format=json"]
        cli_raw = self.cargo(["build", *flags], "native-build")
        self.resources("built")
        self.growth("auditor-precompile", self.root, 512)
        test_raw = self.cargo(["test", "--test", "controlsd_fixture_generate", "--no-run", "--message-format=json"],
                              "auditor-build")
        self.growth("stream-controls-precompile", self.root, 512)
        controls_raw = self.cargo(["test", "--lib", "--no-run", "--message-format=json"], "stream-controls-build")
        self.holds.add(self.out / "stream-controls-build.stdout", owned=True)
        self.holds.add(self.out / "native-build.stdout", owned=True)
        self.holds.add(self.out / "auditor-build.stdout", owned=True)
        self.phase = "strict-clippy"
        self.growth("clippy", self.root, 512)
        self.command([SDK / "bin/cargo", "clippy", "--manifest-path", "rust/Cargo.toml", "--locked",
                      "-p", "openpilot-dev-tools", "--no-default-features", "--features", "controlsd-fixture-native",
                      "--lib", "--bin", "generate-controlsd-fixtures", "--test", "controlsd_fixture_generate",
                      "--", "-D", "warnings"], "clippy", seconds=1500)
        self.resources("clippy")
        cli_source = "rust/crates/dev-tools/src/bin/generate_controlsd_fixtures.rs"
        test_source = "rust/crates/dev-tools/tests/controlsd_fixture_generate.rs"
        cli, cli_record = self.select(test_raw, "generate-controlsd-fixtures", "bin", cli_source, False)
        built_cli, _ = self.select(cli_raw, "generate-controlsd-fixtures", "bin", cli_source, False)
        auditor, test_record = self.select(test_raw, "controlsd_fixture_generate", "test", test_source, True)
        controls, controls_record = self.select(controls_raw, "openpilot_dev_tools", "lib",
                                               "rust/crates/dev-tools/src/lib.rs", True)
        self.holds.add(controls, owned=True, elf=True)
        need(cli == built_cli, "CLI differs across current Cargo invocations")
        self.holds.add(cli, owned=True, elf=True)
        self.holds.add(auditor, owned=True, elf=True)
        save(self.out / "current-artifacts.json", {"source": self.expected, "cli": cli_record, "auditor": test_record,
                                                  "stream_controls": controls_record})
        allowed = set(self.holds.files)
        for path, label in ((cli, "cli-ldd"), (auditor, "auditor-ldd"), (controls, "stream-controls-ldd"),
                            (self.tools["strace"], "strace-ldd"), (self.tools["timeout"], "timeout-ldd")):
            allowed.update(self.dso(path, label))
        allowed.update(self.holds.files)
        save(self.out / "holds-before-runtime.json", self.holds.records())
        self.phase = "stream-controls"
        # 빠른 합성 통제는 sampling에서 ELF를 놓칠 수 있다. 실제 exec와 모든 관측 mapping은 검사한다.
        # native109/전체 auditor에는 원래 actual mapping 관측 필수 조건을 그대로 둔다.
        output = self.trace(controls, ["controlsd_fixture_generate::audit::tests::", "--nocapture", "--test-threads=1"],
                            "stream-controls", allowed, require_mapped=False).decode()
        names = re.findall(r"^test ([\w:]+) \.\.\. ok$", output, re.M)
        results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", output)
        need(len(names) == 3 and set(names) == STREAM_TESTS and len(results) == 1
             and results[0][:4] == ("3", "0", "0", "0"), "exact new streaming control result")
        self.phase = "native109"
        self.trace(cli, ["--root", self.root, "--input", self.out / "requests.jsonl",
                        "--output", self.out / "native.jsonl"], "native109", allowed)
        self.holds.add(self.out / "native.jsonl", owned=True)
        self.resources("native109")
        self.phase = "full-schema-audit"
        self.growth("audit", self.root, 512)
        output = self.trace(auditor, ["--ignored", "--exact", AUDITOR, "--nocapture"], "original-native", allowed).decode()
        results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", output)
        need(results == [("1", "0", "0", "0", "5")]
             and re.findall(r"^test (\w+) \.\.\. ok$", output, re.M) == [AUDITOR], "exact ignored audit result")
        comparison = json.loads(read(self.out / "comparison.json", MIB))
        need(comparison["pass"] is True and comparison["requests"] == 109
             and comparison["schema_and_float_bits"] == "exact; independently checked original pycapnp projection"
             and comparison["sources_retired"] is False and comparison["runtime_acceptance"] is False,
             "comparison receipt contract")
        self.resources("audited")
        self.runtime = {"comparison": comparison, "native_cli_invocations": 1, "exact_ignored_tests": 1, "stream_controls": 3,
                        "fresh_regular_test_invocations": 0, "strict_clippy": True,
                        "runtime_mapping_observation": "sampled executable mappings, with current ELF/ldd DSO FD holds; not pre-dlopen"}

    def finish(self, error):
        failures = []
        for label, operation in (("held", self.holds.verify),
                                 ("source", lambda: need(self.sources("after") == self.source_before, "source full seal differs")),
                                 ("sdk", lambda: need(self.sdk("after") == self.sdk_before, "SDK full seal differs"))):
            if (label == "source" and self.source_before is None) or (label == "sdk" and self.sdk_before is None):
                failures.append(f"{label}: baseline unavailable")
                continue
            try:
                operation()
            except Exception as problem:
                failures.append(f"{label}: {problem}")
        save(self.out / "commands.json", self.commands)
        save(self.out / "holds-after.json", self.holds.records())
        save(self.out / "final-seals.json", {"pass": not failures, "failures": failures,
                                             "source": self.expected, "phase": self.phase})
        if error is not None or failures:
            save(self.out / "failure.json", {"phase": self.phase, "error": error, "final_seal_failures": failures,
                 "original_capture_rerun": False, "fresh_regular_test_invocations": 0,
                 "historical_provider_paths_dereferenced": False, "runtime_acceptance": False})
            return False
        self.budget()
        receipt = {"schema": "openpilot.controlsd-fixture-523.v1", "pass": True,
             "source": self.expected, "original_source": ORIGINAL_SOURCE, "original_artifact": 12462355309,
             "requests": 109, "constructor_cases": 30, "constructor_frames": 8605, **self.runtime,
             "original_capture_rerun": False, "original_python_executions": 0,
             "native_python_exec_attempts": 0, "historical_provider_paths_dereferenced": False,
             "full_schema_and_float_bits": "exact", "wire_byte_identity_is_diagnostic_only": True,
             "runtime_acceptance": False, "normal40_rejection27_health5_accepted": False,
             "retired_sources": [], "inventory_credit": False, "device_acceptance": False}
        self.budget()
        seals = {str(path.relative_to(self.out)): regular(path) for path in paths_under(self.out)
                 if path.is_file()}
        save(self.out / "proof-files.json", {"files": seals, "excludes": ["proof-files.json", "complete.json"]})
        self.budget()
        receipt["proof_files_sha256"] = regular(self.out / "proof-files.json")["sha256"]
        data = (json.dumps(receipt, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()
        self.budget(extra_bytes=len(data), extra_files=1)
        write(self.out / "complete.json", data)
        return True


def main():
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    probe = None
    error = None
    try:
        need(len(sys.argv) == 1, "no arguments; explicit Actions environment only")
        probe = Probe()
        probe.run()
    except Exception:
        error = traceback.format_exc()
        print(error, file=sys.stderr)
    finally:
        if probe is not None:
            try:
                success = probe.finish(error)
            except Exception:
                print(traceback.format_exc(), file=sys.stderr)
                success = False
            finally:
                probe.holds.close()
        else:
            success = False
    return 0 if success else 1


if __name__ == "__main__":
    raise SystemExit(main())
