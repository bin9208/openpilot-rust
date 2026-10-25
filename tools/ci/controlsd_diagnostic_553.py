#!/usr/bin/env python3
"""#553 case3/frame0 한정 Actions 진단. expected/gate를 생성·교체하지 않는다.

GDB/실제 BLAS kernel/역사 전체 provider closure는 관측하지 않는다.
로컬은 AST/서식/diff만 검사한다. worker의 원본·native 실행은 Actions 전용이다.
"""
import argparse
import ast
import hashlib
import importlib.util
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

MIB = 1024**2
GIB = 1024**3
COMMON = "tools/ci/controlsd_normal_523.py"
COMMON_SHA = "8a5da6caaf97dc00a0672e271130e0ce0ead8c055bee9883f7e503b9904b85a7"
ARTIFACT = 12577798385
ARTIFACT_RUN = 40137010441
ACTUAL_SHA = "602ea095f7cd89769ffd9233a04a28983ca59d1f"
ARTIFACT_SIZE = 52127695
ARTIFACT_SHA = "9212f3233b3fd4d479334519e0e7667df1672005823eed5ee23e1bc3e03caf4c"
INNER_SHA = "956326c29facb48c10f3dfeba18507e29327c48919f2cf35840edbd26bfe570f"
RAW = {
    "reference/loop/input.json": (101555623, "9441af7c071e49daaebfcbd8153136ef48bf6f248b3a9c995a8224073c3a0a5b"),
    "reference/loop/source.json": (63477957, "448d2519bd0e0b2580d3d6921e3fab13df0ca4a7f7a66e39e72325b2e2f582d3"),
    "native/input.json": (71299517, "04985ed7fe4e273bfaee1d044c3cdef17adb0b2a51835574c6be25eeb6ef9649"),
    "native/native-raw.json": (17334185, "a9cc70c885290d5b7ec91a3a08ef4b53b49f2596176c36db8a40eee0c75bfdaf"),
}
NUMPY_WHEEL = "numpy-2.5.3-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl"
NUMPY_SHA = "b7e18c623bb5c95acb3b3328861272816ba199fb531921c5d6d0b675f1fde9e3"
DSOS = {
    "libscipy_openblas64_-f48b354e.so": "432e1c1739731468ba305771af96773939e43016643e2d630f58cb45b6670e96",
    "libgfortran-83c28eba-468e71e5.so.5.0.0": "a6a368328d02a5d8a149746f13f6268f8cc9192df5c18777b25c3c9b5ef52160",
    "libquadmath-2284e583-a9307bba.so.0.0.0": "6bc3069003caec1f075be3c51ae8355332f081fa9465910c4b5724e7fe7d8ad4",
}
SOURCE_PINS = {
    COMMON: COMMON_SHA,
    "opendbc_repo/opendbc/car/interfaces.py": "c6ac888f2c2a85413f98413e5f603cbf4daf3749ed6e3d9c122328656952ca10",
    "openpilot/selfdrive/controls/lib/latcontrol_torque.py": "67fb6ecafc4d0ce999e2bbf1b729b5f6cf58a5f787c054e770162e7f2f3629cc",
    "openpilot/common/pid.py": "552286210f964e7fe0c2879219c0f958ea9514be11a0da78eb793cd8bbc40c65",
    "opendbc_repo/opendbc/car/torque_data/lat_models/HYUNDAI_PALISADE.json": "ade61eeb60b5b0250fb945f80d5b14e733e6175cd9c2d5597394ece09b7bb42a",
    "rust/tools/controlsd_source.py": "f5a51d38c64197cc1f8786e8c10a3c5c0b54301deb8d958eae46cafcb321b7b0",
    "rust/tools/controlsd_parameters.py": "bf9fa22d71ad808ec4697bc0ce190753c901901441a007dbe38711e7f0bc31b1",
    "rust/tools/locationd_loop_source.py": "d6239cd4e66e66d87aef577bbc49512f88aeea18270a20fe9263979153939de4",
    "rust/tools/locationd_source.py": "bcd257dde90ea11b68d4bf9f28affc139dd6e92f70d6829cd3464291bd62d87c",
    "rust/tools/check_message_state.py": "928e7cf946a1fbf3c6c0bc9135c580e8f108179cfe78349956eb895f6d0c710a",
    "rust/crates/control-policy/src/numerics.rs": "f6c1516cf7990bd05d13f7aeffa7a45ae051517c5273e77dab26b82584585f44",
    "rust/crates/control-policy/src/numpy_exp.rs": "829c0ab3e3ba15857f87e2fa6f4e8207f3a31e4abe75be713e8b3480aa386a47",
    "rust/crates/controlsd/src/torque_neural.rs": "58894ebce7550555b765429d450c9ef2f2ff3510ba66d8a20fef7c4d47586744",
    "rust/crates/controlsd/examples/controlsd_trace.rs": "c0fee266a3c293b428918595af6dffbee8ff3fc050b944eb896fc0bd3d63743b",
}
PACKAGES = {"numpy": "2.5.3", "pycapnp": "2.1.0", "zstandard": "0.25.0",
            "pycryptodome": "3.23.0", "cffi": "2.1.1", "pyzmq": "27.2.0", "pycparser": "3.0"}
SHAPE = [18, 7, 13, 3, 1]
FLAGS = {"actual_kernel_observed": False, "complete_provider_closure": False,
         "actual_blas_call_observed": False,
         "historical_provider_closure_recorded": False, "full_normal40_acceptance": False,
         "retirement_credit": 0, "original_expected_regenerated": False}


def need(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def loads(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, f"duplicate JSON key: {key}")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def hosted():
    expected = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": "bin9208/openpilot-rust",
                "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/codex/feat-523-python-rust",
                "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64"}
    need(all(os.environ.get(k) == v for k, v in expected.items()) and os.uname().machine == "x86_64",
         "#553 requires the explicit trusted GitHub-hosted source/runner")
    need(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "use -I -B -S")


def common(root):
    path = root / COMMON
    def identity(info):
        return tuple(getattr(info, "st_" + field) for field in
                     ("dev", "ino", "mode", "uid", "gid", "size", "nlink", "mtime_ns", "ctime_ns"))
    info = path.lstat()
    need(stat.S_ISREG(info.st_mode) and info.st_size <= 128*1024, "bounded common helper")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        need(identity(os.fstat(fd)) == identity(info), "common helper open identity")
        raw = os.pread(fd, 128*1024+1, 0)
        need(len(raw) == info.st_size and sha(raw) == COMMON_SHA
             and identity(os.fstat(fd)) == identity(info) == identity(path.lstat()), "common helper pin/identity")
    finally:
        os.close(fd)
    spec = importlib.util.spec_from_file_location("_diagnostic553_common", path)
    module = importlib.util.module_from_spec(spec)
    # 검증한 원문만 로드한다. untracked bytecode cache를 대신 실행하지 않는다.
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def bits(value):
    """JSON 원자료의 타입/모든 F64 bits를 보존한다. 수치 tolerance는 사용하지 않는다."""
    if isinstance(value, float):
        return ("f64", struct.pack("<d", value).hex())
    if isinstance(value, list):
        return [bits(v) for v in value]
    if isinstance(value, dict):
        return {k: bits(value[k]) for k in sorted(value)}
    return (type(value).__name__, value)


def typed_scalar(value, np=None):
    """원래 scalar의 폭/타입을 보존한다. 계산에 사용할 값을 변환하지 않는다."""
    runtime = type(value).__module__ + "." + type(value).__qualname__
    if type(value) is float:
        return {"dtype": "f64", "bits": f"{struct.unpack('<Q', struct.pack('<d', value))[0]:016x}",
                "runtime_type": runtime}
    if np is not None and type(value) in (np.float32, np.float64):
        width = value.dtype.itemsize * 8
        return {"dtype": f"f{width}", "bits": f"{int.from_bytes(value.tobytes(), sys.byteorder):0{width//4}x}",
                "runtime_type": runtime}
    need(type(value) is int and -(1 << 63) <= value < 1 << 63, "unknown/unbounded original input scalar")
    return {"dtype": "int", "value": value, "runtime_type": runtime}


def native_scalar_record(stderr):
    prefix = b"CONTROLSD_553_SCALAR "
    rows = []
    for line in stderr.splitlines():
        if b"CONTROLSD_553_SCALAR" in line:
            need(line.startswith(prefix) and len(line)-len(prefix) <= 64*1024,
                 "foreign/oversize native scalar marker")
            rows.append(loads(line[len(prefix):]))
    need(len(rows) == 1, "exactly one native target scalar receipt")
    return rows[0]


def target_reproduced(actual, oneframe, prefix):
    return bits(actual) == bits(oneframe) == bits(prefix)


def validate_target_scalars(original, native, original_state, native_state):
    def floating(row, width=64):
        need(isinstance(row, dict) and set(row) == {"dtype", "bits", "runtime_type"}
             and row["dtype"] == f"f{width}" and type(row["bits"]) is str
             and re.fullmatch(r"[0-9a-f]{"+str(width//4)+r"}", row["bits"])
             and row["runtime_type"] in ({"builtins.float", "numpy.float64", "rust.f64"} if width == 64 else {"numpy.float32"}),
             "typed scalar width/schema/runtime type")
        return row["bits"]
    for row, observer, extra in ((original, "original", {"inputs"}), (native, "native", {"gain_x", "gain_y"})):
        need(isinstance(row, dict) and set(row) == {"schema", "observer", "case", "frame", "fields", "used_kp_scalar_observed"} | extra
             and type(row["schema"]) is int and row["schema"] == 1 and row["observer"] == observer
             and row["case"] == "HYUNDAI_PALISADE-torque" and type(row["frame"]) is int and row["frame"] == 0,
             "exact target scalar case/frame/schema")
    need(set(original["fields"]) == {"error", "error_cast", "used_kp", "p"}
         and set(native["fields"]) == {"error", "p", "speed"}
         and original["used_kp_scalar_observed"] is True and native["used_kp_scalar_observed"] is False,
         "observed versus stored scalar fields")
    for record, observer in ((original, "original"), (native, "native")):
        for row in record["fields"].values():
            floating(row)
            need((row["runtime_type"] == "rust.f64") == (observer == "native"), "foreign scalar runtime origin")
    for row, state in ((original, original_state), (native, native_state)):
        need(type(state["lat_pid"][0]) is float
             and floating(row["fields"]["p"]) == typed_scalar(state["lat_pid"][0])["bits"],
             "typed p does not match its actual output state")
    for key, expected in (("gain_x", "0000000000000000"), ("gain_y", "3ff0000000000000")):
        need(isinstance(native[key], list) and len(native[key]) == 1 and floating(native[key][0]) == expected,
             "target stored gain is not the bound constant one curve")
        need(native[key][0]["runtime_type"] == "rust.f64", "native stored gain runtime origin")
    need(floating(original["fields"]["used_kp"]) == "3ff0000000000000", "target original used gain differs")
    need(isinstance(original["inputs"], list) and len(original["inputs"]) == 4, "four target original NN inputs")
    for i, call in enumerate(original["inputs"]):
        need(set(call) == {"call", "values"} and type(call["call"]) is int and call["call"] == i
             and isinstance(call["values"], list) and 2 <= len(call["values"]) <= 18, "raw original NN input roster")
        for value in call["values"]:
            if value.get("dtype") == "int":
                need(set(value) == {"dtype", "value", "runtime_type"} and value["runtime_type"] == "builtins.int"
                     and type(value["value"]) is int and -(1 << 63) <= value["value"] < 1 << 63, "raw integer input")
            else:
                floating(value, 32 if value.get("dtype") == "f32" else 64)
    need(len(json.dumps({"original": original, "native": native}, allow_nan=False).encode()) <= 64*1024,
         "combined target scalar set exceeds 64KiB")
    return {"p_bits_equal": original["fields"]["p"]["bits"] == native["fields"]["p"]["bits"],
            "error_bits_equal": original["fields"]["error_cast"]["bits"] == native["fields"]["error"]["bits"],
            "criteria": "exact width and bit pattern; original runtime types retained separately",
            "native_used_gain_scalar_observed": False, "native_stored_gain_constant_one": True}


def target_scalar_controls(original, native, original_state, native_state, stderr):
    """실제 양성을 사용한 GHA-only 변조 통제. 원문/expected는 수정하지 않는다."""
    import copy
    def validate(row):
        validate_target_scalars(original, row, original_state, native_state)
    bad = []
    for name, change in (
            ("foreign_case", lambda r: r.update(case="FOREIGN")),
            ("wrong_scalar_width", lambda r: r["fields"]["p"].update(dtype="f32")),
            ("missing_p", lambda r: r["fields"].pop("p")),
            ("p_output_mismatch", lambda r: r["fields"]["p"].update(bits=f"{int(r['fields']['p']['bits'],16)^1:016x}")),
            ("wrong_gain_curve", lambda r: r["gain_y"][0].update(bits="4000000000000000"))):
        row = copy.deepcopy(native)
        change(row)
        bad.append((name, lambda row=row: validate(row)))
    marker = next(line for line in stderr.splitlines() if line.startswith(b"CONTROLSD_553_SCALAR "))
    for name, raw in (("missing_marker", b""), ("duplicate_marker", marker+b"\n"+marker),
                      ("malformed_marker", b"CONTROLSD_553_SCALAR {"),
                      ("foreign_marker_prefix", b"foreign "+marker)):
        bad.append((name, lambda raw=raw: native_scalar_record(raw)))
    for name, reject in bad:
        try:
            reject()
        except (RuntimeError, ValueError):
            pass
        else:
            raise RuntimeError("target scalar mutation accepted: "+name)
    return {"pass": True, "negative_controls": [name for name, _ in bad], "local_execution": False}


def safe_zip(archive, count, expanded):
    rows = archive.infolist()
    need(len(rows) == len({r.filename for r in rows}) == count
         and sum(r.file_size for r in rows) == expanded, "pinned ZIP roster/expanded size")
    for row in rows:
        name = PurePosixPath(row.filename)
        need(not row.is_dir() and not name.is_absolute() and name.as_posix() == row.filename
             and all(p not in ("", ".", "..") for p in row.filename.split("/"))
             and "\\" not in row.filename and not row.flag_bits & 1
             and stat.S_IFMT(row.external_attr >> 16) in (0, stat.S_IFREG)
             and row.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED), "unsafe ZIP member")


class Diagnostic:
    def initialize(self, args):
        self.out = None
        self.holds = self.children = None
        hosted()
        self.commands = []
        self.baselines = {}
        self.phase = "admission"
        self.result = None
        self.finalizing = False
        self.root = Path(args.root)
        self.job = Path(os.environ["CI_JOB_TEMP"])
        self.source = args.source_commit
        need(self.source == os.environ["GITHUB_SHA"] and len(self.source) == 40
             and all(c in "0123456789abcdef" for c in self.source), "current source SHA")
        need(self.root == Path.cwd() == Path(os.environ["DIAG_SOURCE"]) == Path(os.environ["GITHUB_WORKSPACE"]),
             "private source binding")
        need(self.root.parent == Path(os.environ["DIAG_SOURCE_OWNER"])
             and self.root.parent.parent == Path(os.environ["DIAG_ACTIONS_WORKSPACE"]), "private source owner")
        owner = ":".join(os.environ[k] for k in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB"))
        for p in (self.root, self.root.parent, self.job, self.job / "proof"):
            s = p.lstat()
            need(p.is_absolute() and p.resolve(strict=True) == p and stat.S_ISDIR(s.st_mode)
                 and s.st_uid == os.getuid(), f"canonical owned root: {p}")
        need((self.job / "owner").read_text().strip() == owner
             == (self.root.parent / ".diag296-owner").read_text().strip(), "private owner marker")
        need(os.environ["DIAG_SOURCE_IDENTITY"] == f"{self.root.parent.stat().st_dev}:{self.root.parent.stat().st_ino}",
             "source owner inode")
        out = Path(args.evidence)
        need(out.parent == self.job / "proof" and out.name == "controls-diagnostic-553" and not out.exists(),
             "absent dedicated #553 evidence root")
        out.mkdir(mode=0o700)
        self.out = out
        self.c = common(self.root)
        self.holds = self.c.Holds()
        self.children = self.c.Children(out / "children-events.jsonl")
        self.holds.add(self.root / COMMON, COMMON_SHA)
        self.holds.add(Path(__file__).resolve())
        self.target = self.root / "rust/target"
        self.sdk = Path(os.environ["CI_PREPARED_RUST_ROOT"])
        need(Path(os.environ["CARGO_TARGET_DIR"]) == self.target
             and self.sdk == self.job / "rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu"
             and self.sdk.resolve(strict=True) == self.sdk, "prepared SDK/private target")
        self.python = Path(args.python)
        need(self.python.is_absolute() and str(self.python) == os.environ["ORIGINAL_PYTHON"], "explicit original Python")
        self.holds.add(self.python, elf=True)
        self.python_prefix = Path(os.environ["ORIGINAL_PYTHON_ROOT"])
        need(self.python_prefix.is_absolute() and self.python_prefix.resolve(strict=True) == self.python_prefix
             and self.python.resolve().parent.parent == self.python_prefix, "canonical actual original Python root")
        need(not any(a.is_relative_to(b) or b.is_relative_to(a)
                     for a, b in ((self.python_prefix, self.job), (self.python_prefix, self.root.parent))),
             "original Python root must be separate from owned source/job")
        self.python_lib = self.python_prefix / "lib"
        self.deps = self.job / "originaldeps"
        self.packages = self.job / "packages"
        self.tmp = self.job / "diagnostic553-tmp"
        self.bin = self.job / "diagnostic553-bin"
        self.numerics = self.job / "cache/diagnostic553-numerics"
        for p in (self.deps, self.packages, self.tmp, self.bin, self.numerics):
            p.mkdir(mode=0o700)
        self.env = {k: os.environ[k] for k in ("GITHUB_ACTIONS", "GITHUB_REPOSITORY", "GITHUB_EVENT_NAME",
                    "GITHUB_REF", "GITHUB_SHA", "RUNNER_ENVIRONMENT", "RUNNER_OS", "RUNNER_ARCH")}
        self.env.update(HOME=str(self.job / "home"), TMPDIR=str(self.tmp), XDG_CACHE_HOME=str(self.job / "cache"),
                        PATH=f"{self.sdk / 'bin'}:/usr/bin:/bin", LC_ALL="C", TZ="UTC", PYTHONDONTWRITEBYTECODE="1",
                        CARGO_HOME=str(self.job / "cargo-home"), RUSTUP_HOME=str(self.job / "rustup-home"),
                        CARGO_TARGET_DIR=str(self.target), RUSTC=str(self.sdk / "bin/rustc"),
                        RUSTDOC=str(self.sdk / "bin/rustdoc"), RUSTUP_TOOLCHAIN="1.94.0", CARGO_BUILD_JOBS="2",
                        CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0",
                        CARGO_TERM_COLOR="never")
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        need(hard == resource.RLIM_INFINITY or hard >= 4096, "at least 4096 FDs")
        resource.setrlimit(resource.RLIMIT_NOFILE, (max(soft, 4096), hard))
        self.deadline = time.monotonic() + 52 * 60
        self.last_budget = 0
        self.tools = {}
        for name in ("git", "gh", "curl", "cc", "gcc", "c++", "clang", "clang++", "pkg-config", "capnp",
                     "as", "ar", "ranlib", "ld", "bash", "sh"):
            p = shutil.which(name, path="/usr/bin:/bin")
            need(p, f"required actual provider: {name}")
            self.tools[name] = self.holds.add(Path(p), elf=name != "sh")
        for name, pin in self.c.SDK_PINS.items():
            self.holds.add(self.sdk / "bin" / name, pin, elf=True)

    def save(self, name, value):
        self.c.save(self.out / name, value)

    def budget(self, force=False):
        if not force and time.monotonic() - self.last_budget < 0.5:
            return
        caps = {"source": 2*GIB, "build": 8*GIB, "sdk": 3*GIB, "proof": GIB,
                "originaldeps": GIB, "originalpython": GIB}
        for attempt in range(3):
            totals = dict.fromkeys(caps, 0)
            devices, seen, transient = {}, set(), []
            def missing(p):
                areas = (self.target, self.deps, self.packages, self.tmp, self.job / "cargo-home", self.root / ".git")
                need(any(p != area and p.is_relative_to(area) for area in areas), f"unknown missing path: {p}")
                transient.append(str(p))
            for root, default in ((self.root.parent, "source"), (self.job, "build"),
                                  (self.python_prefix, "originalpython")):
                for p in self.c.walk(root, mutable=True, missing=missing):
                    try:
                        info = p.lstat()
                    except FileNotFoundError:
                        missing(p)
                        continue
                    need(len(seen) < 180000, "growth entry bound")
                    area = default
                    for prefix, category in ((self.target, "build"), (self.job / "rustup-home", "sdk"),
                            (self.job / "rust-bootstrap", "sdk"), (self.job / "cargo-home/bin", "sdk"),
                            (self.job / "proof", "proof"), (self.deps, "originaldeps"), (self.packages, "originaldeps")):
                        if p.is_relative_to(prefix):
                            area = category
                    if area == "proof":
                        need(stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode), "special/alias proof output")
                        if stat.S_ISREG(info.st_mode):
                            need(info.st_nlink == 1 and info.st_size <= 512*MIB, "proof hardlink/file cap")
                    key = info.st_dev, info.st_ino
                    if key not in seen:
                        seen.add(key)
                        totals[area] += max(info.st_blocks*512, info.st_size if stat.S_ISREG(info.st_mode) else 0)
                    devices.setdefault(info.st_dev, p.parent)
            need(all(totals[k] <= caps[k] for k in caps), f"operation growth cap: {totals}")
            if hasattr(self, "clippy_base"):
                need(totals["build"] <= self.clippy_base + 512*MIB, "Clippy append growth cap")
            if transient:
                need(attempt < 2, "mutable growth sampling did not stabilize")
                continue
            # helper는 16.5GiB이며, 진입 전 완료한 APT2GiB와 합쳐 최초 18.5GiB 계약이다.
            roots = {"source": self.root.parent, "build": self.job, "sdk": self.sdk,
                     "proof": self.out, "originaldeps": self.job, "originalpython": self.python_prefix}
            for device, path in devices.items():
                remaining = GIB//2 + sum(caps[k]-totals[k] for k, p in roots.items() if p.stat().st_dev == device)
                need(shutil.disk_usage(path).free >= remaining, f"physical filesystem admission: {path}")
            self.last_budget = time.monotonic()
            self.usage = totals
            return

    def command(self, argv, label, seconds=600, env=None, limit=32*MIB, stdin=None):
        if not self.finalizing:
            self.budget(force=True)
        need(not self.children.scan(), "owned child before command")
        process, error = None, None
        started = time.monotonic()
        def limits():
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
            resource.setrlimit(resource.RLIMIT_FSIZE, (512*MIB, 512*MIB))
        with (self.out / f"{label}.stdout").open("xb") as stdout, (self.out / f"{label}.stderr").open("xb") as stderr:
            try:
                process = subprocess.Popen(list(map(str, argv)), cwd=self.root, env=env or self.env,
                    stdin=stdin if stdin is not None else subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                    start_new_session=True, preexec_fn=limits)
                self.children.process = process
                while True:
                    rows = self.children.scan()
                    for pid, identity in rows.items():
                        try:
                            need(self.c.proc_stat(pid)["start"] == identity["start"], "sampled provider PID reused")
                            for line in Path(f"/proc/{pid}/maps").read_text().splitlines():
                                fields = line.split(None, 5)
                                if len(fields) != 6 or "x" not in fields[1] or not fields[5].startswith("/"):
                                    continue
                                path = Path(fields[5].removesuffix(" (deleted)"))
                                # build/temp namespace는 immutable provider로 restamp하지 않는다.
                                if path.is_relative_to(self.target) or path.is_relative_to(self.tmp):
                                    continue
                                need(not fields[5].endswith(" (deleted)"), "deleted current provider mapping")
                                self.holds.add(path, elf=True)
                        except (FileNotFoundError, ProcessLookupError):
                            # 종료와 maps 조회 사이 경합만 허용한다. complete-maps 주장은 하지 않는다.
                            try:
                                current = self.c.proc_stat(pid)
                            except (FileNotFoundError, ProcessLookupError):
                                continue
                            need(current["state"] in ("Z", "X") and current["start"] == identity["start"],
                                 "live process provider disappeared")
                    self.children.reap()
                    need(time.monotonic() <= min(started + seconds, self.deadline), f"#553 command timeout: {label}")
                    need(stdout.tell() <= limit and stderr.tell() <= limit, f"#553 log cap: {label}")
                    if not self.finalizing:
                        self.budget()
                    if process.returncode is not None:
                        remaining = self.children.scan()
                        if remaining:
                            observed = []
                            for pid, row in remaining.items():
                                entry = {k: v for k, v in row.items() if k != "fd"}
                                try:
                                    entry["executable"] = os.readlink(f"/proc/{pid}/exe")
                                except OSError as problem:
                                    entry["executable_read_error"] = type(problem).__name__
                                observed.append(entry)
                            self.save("live-children-after-command.json", {"label": label, "children": observed})
                        need(not remaining, "live child after command")
                        need(process.returncode == 0, f"{label}: exit {process.returncode}")
                        break
                    time.sleep(0.02)
            except BaseException as problem:
                error = str(problem)
                raise
            finally:
                try:
                    self.children.stop()
                finally:
                    self.commands.append({"label": label, "argv": list(map(str, argv)), "error": error,
                        "returncode": process.returncode if process else None,
                        "elapsed_seconds": time.monotonic()-started,
                        "all_owned_children_reaped": not self.children.scan(),
                        "provider_scope": "sampled executable file FDs sealed from first observation; mutable build/temp excluded; not complete closure"})
                    self.children.process = None
        if not self.finalizing:
            self.budget(force=True)
        return self.c.read(self.out / f"{label}.stdout", limit)

    def snapshot(self, root):
        return self.c.snapshot(root, self.c.walk(root))

    def sources(self, label):
        def git(*args):
            return self.command([self.tools["git"], "--no-optional-locks", "-C", self.root, *args], f"{label}-git-{args[0]}")
        need(git("rev-parse", "HEAD").decode().strip() == self.source, "current HEAD")
        git("diff", "--exit-code", "HEAD", "--")
        index = git("ls-files", "--stage", "-z")
        files = []
        for entry in index.split(b"\0"):
            if not entry:
                continue
            meta, name = entry.split(b"\t", 1)
            mode, _, stage = meta.split()
            p = PurePosixPath(os.fsdecode(name))
            need(stage == b"0" and mode != b"160000" and not p.is_absolute() and ".." not in p.parts, "index shape")
            files.append(self.root / str(p))
        value = {"source": self.source, "index_sha256": sha(index), "files": self.c.snapshot(self.root, files)}
        self.save(f"source-{label}.json", value)
        return value

    def import_actual(self):
        self.phase = "authenticated-602-data"
        # 인증 다운로드에만 공식 telemetry opt-out을 고정한다. 수치 실행 환경은 그대로다.
        auth = {**self.env, "GH_TOKEN": os.environ["GH_TOKEN"], "GH_TELEMETRY": "false"}
        self.command([self.tools["gh"], "--version"], "artifact-gh-version", env=auth, seconds=30)
        raw = self.command([self.tools["gh"], "api", f"repos/bin9208/openpilot-rust/actions/artifacts/{ARTIFACT}"],
                           "artifact-metadata", env=auth, seconds=180)
        metadata = loads(raw)
        need(metadata["id"] == ARTIFACT and metadata["size_in_bytes"] == ARTIFACT_SIZE
             and metadata["workflow_run"]["id"] == ARTIFACT_RUN
             and metadata["workflow_run"]["head_sha"] == ACTUAL_SHA
             and metadata["digest"] == "sha256:" + ARTIFACT_SHA and metadata["expired"] is False,
             "602 artifact metadata binding")
        self.command([self.tools["gh"], "api", f"repos/bin9208/openpilot-rust/actions/artifacts/{ARTIFACT}/zip"],
                     "602-artifact", env=auth, seconds=240, limit=ARTIFACT_SIZE+1)
        artifact = self.out / "602-artifact.stdout"
        need(artifact.stat().st_size == ARTIFACT_SIZE, "602 archive size")
        self.holds.add(artifact, ARTIFACT_SHA)
        imported = {}
        with zipfile.ZipFile(artifact) as archive:
            safe_zip(archive, 208, 382908540)
            for relative, (size, pin) in {**RAW,
                    "reference/artifact.zip": (9895310, INNER_SHA),
                    "numerics-wheel.stdout": (16717410, NUMPY_SHA)}.items():
                member = archive.getinfo("controls-normal40/" + relative)
                need(member.file_size == size, "selected raw size")
                target = self.packages / NUMPY_WHEEL if relative == "numerics-wheel.stdout" else self.out / relative.replace("/", "--")
                total = 0
                digest = hashlib.sha256()
                with archive.open(member) as source, target.open("xb") as output:
                    while chunk := source.read(MIB):
                        total += len(chunk)
                        need(total <= size, "selected raw inflation bound")
                        digest.update(chunk)
                        output.write(chunk)
                need(total == size and digest.hexdigest() == pin, "selected raw CRC/SHA")
                self.holds.add(target, pin)
                imported[relative] = target
        # 같은 고정 inner ZIP의 selected raw2와 바깥 copy를 직접 대응시킨다.
        with zipfile.ZipFile(imported["reference/artifact.zip"]) as archive:
            safe_zip(archive, 120, 259913092)
            for name in ("loop/input.json", "loop/source.json"):
                item = archive.getinfo(name)
                value = hashlib.sha256()
                count = 0
                with archive.open(item) as stream:
                    while chunk := stream.read(MIB):
                        count += len(chunk)
                        need(count <= RAW["reference/" + name][0], "inner raw bound")
                        value.update(chunk)
                need((count, value.hexdigest()) == RAW["reference/" + name], "inner/outer raw binding")
        old = loads(self.c.read(imported["reference/loop/input.json"], 128*MIB))["cases"]
        need(len(old) == 40 and old[3]["name"] == "HYUNDAI_PALISADE-torque" and len(old[3]["frames"]) == 200,
             "frozen case3 identity")
        case = old[3]
        del old
        actual_input = loads(self.c.read(imported["native/input.json"], 128*MIB))["cases"]
        need(len(actual_input) == 40 and actual_input[3] == case, "602/frozen exact case3 input")
        prefix = actual_input[:4]
        need([(c["name"], len(c["frames"])) for c in prefix] == [
            ("HONDA_CIVIC-pid", 200), ("TOYOTA_RAV4-torque", 200),
            ("VOLKSWAGEN_ID4_MK1-angle", 200), ("HYUNDAI_PALISADE-torque", 200)],
             "602 native prefix case/frame roster")
        del actual_input
        self.expected_original = loads(self.c.read(imported["reference/loop/source.json"], 128*MIB))[3]["rows"][0]["state"]
        self.expected_native = loads(self.c.read(imported["native/native-raw.json"], 32*MIB))[3]["rows"][0]["state"]
        need(self.expected_original["lat_pid"][0] == -0.034913528710603714
             and self.expected_native["lat_pid"][0] == -0.03491372987627983, "602 first failure boundary")
        case = {**case, "frames": case["frames"][:1]}
        self.request = self.out / "case3-frame0-input.json"
        self.c.write_new(self.request, (json.dumps({"cases": [case]}, allow_nan=False)+"\n").encode())
        self.holds.add(self.request)
        # 앞600프레임은 원문 native 입력 그대로다. 원본 prefix로 대체하지 않는다.
        prefix[3] = {**prefix[3], "frames": prefix[3]["frames"][:1]}
        need(prefix[3] == case and sum(len(c["frames"]) for c in prefix) == 601,
             "602 native prefix600 plus exact target frame0")
        self.prefix_request = self.out / "native-prefix600-target1-input.json"
        self.c.write_new(self.prefix_request, (json.dumps({"cases": prefix}, allow_nan=False)+"\n").encode())
        self.holds.add(self.prefix_request)
        self.prefix_roster = [{"name": c["name"], "frames": len(c["frames"])} for c in prefix]
        self.save("input-admission.json", {"artifact": ARTIFACT, "run": ARTIFACT_RUN, "source": ACTUAL_SHA,
            "archive_sha256": ARTIFACT_SHA, "inner_archive_sha256": INNER_SHA, "selected_members_crc_checked": 6,
            "all_members_crc_checked": False, "source_case_index": 3, "source_case_frames": 200,
            "selected_frame_index": 0, "selected_cases": 1, "selected_frames": 1,
            "input_sha256": self.c.regular(self.request)["sha256"], "expected_original_state": self.expected_original,
            "expected_602_native_state": self.expected_native,
            "prefix_input_source_member": "controls-normal40/native/input.json",
            "prefix_input_source_sha256": RAW["native/input.json"][1],
            "prefix_input_sha256": self.c.regular(self.prefix_request)["sha256"],
            "prefix_cases": self.prefix_roster, "prefix_context_frames": 600,
            "prefix_target_case_index": 3, "prefix_target_frame_index": 0, **FLAGS})

    def prepare_python(self):
        self.phase = "private-original-provider"
        pyenv = {**self.env, "LD_LIBRARY_PATH": str(self.python_lib)}
        version = loads(self.command([self.python, "-I", "-B", "-S", "-c",
            "import json,sys,sysconfig; assert sys.version_info[:3]==(3,12,15); print(json.dumps({'version':sys.version,'stdlib':sysconfig.get_path('stdlib'),'prefix':sys.prefix}))"],
            "python-version", env=pyenv))
        need(self.python_prefix == Path(version["prefix"]), "actual Python prefix binding")
        lock = tomllib.loads(self.c.read(self.root / "uv.lock", 4*MIB).decode())
        self.holds.add(self.root / "uv.lock")
        wheels = []
        for name, version in PACKAGES.items():
            rows = [r for r in lock["package"] if r["name"] == name and r["version"] == version]
            need(len(rows) == 1, f"locked package: {name}")
            candidates = []
            for row in rows[0]["wheels"]:
                file = row["url"].rsplit("/", 1)[-1]
                if "py3-none-any" in file or ("x86_64" in file and "manylinux" in file
                        and ("cp312" in file or (name == "pycryptodome" and "abi3" in file))):
                    candidates.append({**row, "file": file, "name": name, "version": version})
            need(len(candidates) == 1, f"exact cp312/abi3 wheel: {name}")
            row = candidates[0]
            need(row["url"].startswith("https://files.pythonhosted.org/packages/")
                 and row["size"] <= 32*MIB and row["hash"].startswith("sha256:"), "locked wheel source/bound")
            target = self.packages / row["file"]
            if name != "numpy":
                self.command([self.tools["curl"], "--fail", "--silent", "--show-error", "--location",
                              "--proto", "=https", "--proto-redir", "=https", "--max-time", "180", row["url"]],
                             "wheel-" + name, seconds=190, limit=row["size"]+1)
                self.c.copy_new(self.out / f"wheel-{name}.stdout", target, 32*MIB)
            need(target.stat().st_size == row["size"], "wheel literal size")
            self.holds.add(target, row["hash"].removeprefix("sha256:"))
            if name == "numpy":
                need(row["file"] == NUMPY_WHEEL and row["hash"] == "sha256:"+NUMPY_SHA, "NumPy original pin")
            wheels.append(row)
        self.command([self.python, "-I", "-B", "-m", "venv", "--without-pip", "--copies", self.deps], "private-venv", env=pyenv)
        requirements = self.packages / "requirements.txt"
        self.c.write_new(requirements, ("\n".join(f"{(self.packages / r['file']).as_uri()} --hash={r['hash']}" for r in wheels)+"\n").encode())
        self.holds.add(requirements)
        self.command([self.python, "-I", "-B", "-m", "pip", "install", "--prefix", self.deps, "--ignore-installed", "--no-index", "--no-deps",
                      "--no-compile", "--no-cache-dir", "--disable-pip-version-check", "--require-hashes", "-r", requirements],
                     "private-wheel-install", env=pyenv, seconds=240)
        self.site = self.deps / "lib/python3.12/site-packages"
        for name, pin in DSOS.items():
            self.holds.add(self.site / "numpy.libs" / name, pin, elf=True)
        # 기존 Numerics loader가 받는 manifest/schema와 DSO3만 준비한다.
        for name, pin in DSOS.items():
            self.c.copy_new(self.site / "numpy.libs" / name, self.numerics / name, 64*MIB)
            self.holds.add(self.numerics / name, pin, elf=True)
        manifest = {"format": 1, "numpy": "2.5.3", "abi": "scipy_dgesdd_64_",
                    "library": "libscipy_openblas64_-f48b354e.so",
                    "files": [{"name": name, "sha256": pin} for name, pin in DSOS.items()]}
        self.c.save(self.numerics / "manifest.json", manifest)
        self.holds.add(self.numerics / "manifest.json")
        self.save("original-package-pins.json", {"packages": wheels, "python": str(self.python),
            "python_library_path": str(self.python_lib), "historical_other_package_closure_recorded": False,
            "scope": "current lock wheel versions; historical pycparser/global closure is not restamped", **FLAGS})

    def build_trace(self, tapped):
        label = "tapped-build" if tapped else "plain-build"
        features = ["diagnostic-553", "native"] if tapped else ["native"]
        target = self.target
        env = self.env
        if getattr(self, "same_job_normal_mode", False):
            need(tapped, "same-job must reuse its held actual plain ELF")
            # 원래 Cargo executable 경로는 이미 hold됐다. 다른 target에서만 observer를 빌드한다.
            target = self.target / "same-job-observer"
            need(not target.exists() and not target.is_symlink(), "fresh nested observer Cargo target")
            env = {**self.env, "CARGO_TARGET_DIR": str(target)}
        raw = self.command([self.sdk / "bin/cargo", "build", "--manifest-path", self.root / "rust/Cargo.toml",
            "--locked", "-p", "openpilot-controlsd", "--no-default-features", "--features", ",".join(features),
            "--example", "controlsd_trace", "--message-format=json", *self.c.SHA2_PROFILE_ARGS], label, seconds=1200, env=env)
        rows = [loads(line) for line in raw.splitlines()]
        need(rows and rows[-1] == {"reason": "build-finished", "success": True}
             and sum(r.get("reason") == "build-finished" for r in rows) == 1, "Cargo success JSONL")
        found = [r for r in rows if r.get("reason") == "compiler-artifact"
                 and r["target"]["name"] == "controlsd_trace" and r["target"]["kind"] == ["example"]]
        need(len(found) == 1, "single eager controlsd_trace Cargo artifact")
        row = found[0]
        manifest = self.root / "rust/crates/controlsd/Cargo.toml"
        need(row["manifest_path"] == str(manifest)
             and row["package_id"] == f"path+file://{manifest.parent}#openpilot-controlsd@0.1.0"
             and row["target"]["src_path"] == str(self.root / "rust/crates/controlsd/examples/controlsd_trace.rs")
             and sorted(row["features"]) == features and row["profile"]["test"] is False
             and row["profile"]["opt_level"] == "0", "exact Cargo source/features/profile")
        path = Path(row["executable"])
        need(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(target), "private Cargo ELF")
        copied = self.bin / ("controlsd-trace-tapped" if tapped else "controlsd-trace-plain")
        before = self.c.regular(path, 512*MIB)
        self.c.copy_new(path, copied, 512*MIB)
        copied.chmod(0o500)
        self.holds.add(copied, before["sha256"], elf=True)
        self.save(label+"-selection.json", {"artifact": row, "cargo_jsonl_sha256": sha(raw),
            "selected_before_copy": before, "private_copy": str(copied),
            "copy_sha256": before["sha256"], "copied_before_next_build": True})
        return copied

    def observe_target_failure(self, plain_trace, original_trace, native_env):
        """재현 admission 뒤에만 호출한다. 기존 Replay deadline을 새로 부여하지 않는다."""
        need(getattr(self, "same_job_normal_mode", False), "target observation exact same-job mode")
        self.budget(force=True)
        self.phase = "same-job-tapped-build"
        tapped = self.build_trace(True)
        self.holds.verify()
        config = self.out / "worker-config.json"
        self.phase = "same-job-original-tapped"
        self.command([self.python, "-I", "-B", "-S", self.root / "tools/ci/controlsd_diagnostic_553.py",
                      "--worker", "original-tapped", "--config", config], "original-tapped", seconds=700,
                     env={**self.env, "PATH": "/nonexistent", "LD_LIBRARY_PATH": str(self.python_lib)})
        for suffix in (".json", "-seals.json", "-providers-before.json", "-providers-after.json", "-numpy-dispatch.json",
                       "-layers.json", "-ast.json", "-pid-ast.json", "-scalars.json", "-getters.json"):
            self.holds.add(self.out / ("original-tapped"+suffix))
        seals = loads(self.c.read(self.out / "original-tapped-seals.json"))
        need(seals["pass"] is True and seals["errors"] == [] and seals["worker_error"] is None,
             "target original tapped provider seals")
        output = self.out / "same-native-tapped.json"
        self.phase = "same-job-native-tapped"
        with self.request.open("rb") as stream:
            need(self.c.ident(os.fstat(stream.fileno())) == self.holds.records()[str(self.request)]["identity"],
                 "target tapped held input FD")
            self.command([tapped, output], "same-native-tapped", seconds=600, env=native_env, stdin=stream)
        self.holds.add(output)
        original = loads(self.c.read(self.out / "original-tapped.json", 16*MIB))
        native = loads(self.c.read(output, 16*MIB))
        original_same, native_same = bits(original) == bits(original_trace), bits(native) == bits(plain_trace)
        self.save("target-observer-effects.json", {"original_tapped_equals_plain": original_same,
            "native_tapped_equals_same_held_plain_replay": native_same,
            "same_native_elf": False, "actual_fullnormal_process_intermediates_observed": False, **FLAGS})
        self.holds.add(self.out / "target-observer-effects.json")
        need(original_same and native_same, "target one-frame observation changed output bits")
        stderr = self.c.read(self.out / "same-native-tapped.stderr", 32*MIB)
        native_layers, getters = [], []
        for line in stderr.splitlines():
            if line.startswith(b"CONTROLSD_553_LAYER "):
                native_layers.append(loads(line.removeprefix(b"CONTROLSD_553_LAYER ")))
            if line.startswith(b"CONTROLSD_553_GETTERS "):
                raw = line.removeprefix(b"CONTROLSD_553_GETTERS ")
                need(len(raw) <= 64*1024, "target getter receipt bound")
                getters.append(loads(raw))
        need(len(getters) == 1, "target native getter receipt once")
        comparison = compare_layers(loads(self.c.read(self.out / "original-tapped-layers.json", 32*MIB)), native_layers)
        getter_comparison = compare_getters(loads(self.c.read(self.out / "original-tapped-getters.json", 64*1024)),
                                            getters[0], self.source, self.site, Path(native_env["CONTROLS_NUMERICS"]), comparison)
        left = loads(self.c.read(self.out / "original-tapped-scalars.json", 64*1024))
        right = native_scalar_record(stderr)
        original_state, native_state = original[0]["rows"][0]["state"], native[0]["rows"][0]["state"]
        scalar_comparison = validate_target_scalars(left, right, original_state, native_state)
        need(len(left["inputs"]) == comparison["original_calls"] == comparison["native_calls"] == 4,
             "target raw input and layer call binding")
        controls = target_scalar_controls(left, right, original_state, native_state, stderr)
        self.save("same-native-tapped-scalars.json", right)
        self.holds.add(self.out / "same-native-tapped-scalars.json")
        self.save("target-scalar-controls.json", controls)
        self.holds.add(self.out / "target-scalar-controls.json")
        result = {"schema": 1, "source": self.source, "diagnostic_only": True,
            "case": "HYUNDAI_PALISADE-torque", "frame": 0,
            "observation_valid": True, "scalar_layer_observed": True,
            "actual_failure_reproduced_before_observation": True,
            "original_tapped_equals_plain": original_same, "native_tapped_equals_plain": native_same,
            "actual_fullnormal_process_intermediates_observed": False,
            "original_scalars": left, "native_scalars": right, "scalar_comparison": scalar_comparison,
            "layer_comparison": comparison, "same_instance_getters": getter_comparison,
            "negative_controls": controls, "fullnormal_failure_preserved": True, **FLAGS}
        self.save("target-scalar-layer-comparison.json", result)
        self.holds.add(self.out / "target-scalar-layer-comparison.json")
        return result

    def clippy(self):
        self.phase = "strict-current-clippy"
        self.budget(force=True)
        before = self.usage["build"]
        self.clippy_base = before
        try:
            for package, target, features in (("openpilot-control-policy", ["--lib"], "diagnostic-553,numerics"),
                    ("openpilot-controlsd", ["--example", "controlsd_trace"], "diagnostic-553,native")):
                self.command([self.sdk / "bin/cargo", "clippy", "--manifest-path", self.root / "rust/Cargo.toml",
                    "--locked", "-p", package, "--no-default-features", "--features", features,
                    *target, *self.c.SHA2_PROFILE_ARGS, "--", "-D", "warnings"],
                    "clippy-"+package, seconds=1200)
        finally:
            del self.clippy_base
        self.budget(force=True)
        need(self.usage["build"]-before <= 512*MIB, "Clippy append exceeds 512 MiB")

    def getter_units(self):
        label = "same-instance-getter-unit2"
        self.phase = label
        selectors = [
            "numerics::diagnostic_553::tests::getter_pointer_requires_unique_mapping_and_exact_provider_identity",
            "numerics::diagnostic_553::tests::getter_strings_require_nonempty_bounded_terminator",
        ]
        # libtest의 복수 exact 필터만 실행한다. 0선택/부분 선택/추가 실행/ignored는 수용하지 않는다.
        raw = self.command([self.sdk / "bin/cargo", "test", "--manifest-path", self.root / "rust/Cargo.toml",
            "--locked", "-p", "openpilot-control-policy", "--no-default-features", "--features", "diagnostic-553",
            "--lib", *self.c.SHA2_PROFILE_ARGS, "--", "--exact", *selectors,
            "--nocapture", "--test-threads=1", "--color=never", "--format=pretty"], label, seconds=600)
        output = raw.decode()
        roster = re.findall(r"^test (\S+) \.\.\. ([^\n]+)$", output, re.M)
        results = re.findall(r"^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; "
                             r"(\d+) measured; (\d+) filtered out;", output, re.M)
        need(output.splitlines().count("running 2 tests") == 1
             and roster == [(name, "ok") for name in selectors]
             and len(results) == 1 and results[0][:4] == ("2", "0", "0", "0"),
             "exact same-instance getter unit2 roster/summary")
        self.save(label+".json", {"schema": 1, "source": self.source, "selectors": selectors,
            "passed": 2, "failed": 0, "ignored": 0, "measured": 0, "filtered_out": int(results[0][4]),
            "stdout_sha256": sha(raw), "command_label": label, "diagnostic_only": True, **FLAGS})
        for suffix in (".stdout", ".stderr", ".json"):
            self.holds.add(self.out / (label+suffix))

    def loader_environment_comparison(self, plain, native_env, native_plain):
        directory = self.out / "native-loader-ab"
        directory.mkdir(mode=0o700)
        output = directory / "native-result.json"
        plain_seal = self.holds.records()[str(plain)]
        input_seal = self.holds.records()[str(self.request)]
        plain_record = {k: plain_seal[k] for k in ("identity", "sha256")}
        input_record = {k: input_seal[k] for k in ("identity", "sha256")}
        with_ld = dict(native_env)
        need(with_ld.get("LD_LIBRARY_PATH") == str(self.numerics), "existing private native loader environment")
        without_ld = {k: v for k, v in with_ld.items() if k != "LD_LIBRARY_PATH"}
        need(set(with_ld) - set(without_ld) == {"LD_LIBRARY_PATH"}
             and all(with_ld[k] == v for k, v in without_ld.items()), "loader A/B single environment difference")
        rows, traces = [], {}
        directory_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
        directory_identity = self.c.ident(os.fstat(directory_fd))[:5]

        def directory_guard():
            need(directory.resolve(strict=True) == directory
                 and self.c.ident(directory.lstat())[:5] == directory_identity
                 == self.c.ident(os.fstat(directory_fd))[:5]
                 and stat.S_ISDIR(directory_identity[2]) and directory_identity[2] & 0o777 == 0o700
                 and directory_identity[3] == os.getuid(), "same private loader A/B directory FD")

        try:
            for variant, env in (("with-ld", with_ld), ("without-ld", without_ld)):
                label = "native-loader-" + variant
                self.phase = label
                directory_guard()
                need(not os.path.lexists(output), "loader A/B fixed output must start absent")
                need(time.monotonic() < self.deadline, "#553 global deadline before loader A/B")
                self.holds.verify()
                need(self.c.regular(plain) == plain_record and self.c.regular(self.request) == input_record,
                     "same held plain ELF and one-frame input before loader A/B")
                command_error = None
                with self.request.open("rb") as source:
                    need(self.c.ident(os.fstat(source.fileno())) == input_seal["identity"], "loader A/B stdin FD")
                    try:
                        self.command([plain, output], label, env=env, stdin=source, seconds=600)
                    except RuntimeError as problem:
                        record = self.commands[-1] if self.commands else {}
                        code = record.get("returncode")
                        # 보통의 child exit만 별도 관측한다. timeout/guard/회수 오류는 계속 전파한다.
                        if not (record.get("label") == label and code is not None and code > 0
                                and record.get("error") == str(problem) == f"{label}: exit {code}"
                                and record.get("all_owned_children_reaped") is True):
                            raise
                        command_error = str(problem)
                    need(self.c.ident(os.fstat(source.fileno())) == input_seal["identity"], "loader A/B stdin FD changed")
                self.budget(force=True)
                self.holds.verify()
                need(self.c.regular(plain) == plain_record and self.c.regular(self.request) == input_record,
                     "same held plain ELF and one-frame input after loader A/B")
                directory_guard()
                record = self.commands[-1]
                need(record["label"] == label and record["argv"] == [str(plain), str(output)],
                     "same actual loader A/B argv")
                row = {"variant": variant, "command_label": label, "argv": [str(plain), str(output)],
                       "stdout": str(self.out / (label + ".stdout")), "stderr": str(self.out / (label + ".stderr")),
                       "ld_library_path_present": "LD_LIBRARY_PATH" in env,
                       "ld_library_path": env.get("LD_LIBRARY_PATH"), "returncode": record["returncode"],
                       "command_error": command_error, "elapsed_seconds": record["elapsed_seconds"],
                       "all_owned_children_reaped": record["all_owned_children_reaped"],
                       "output": None, "output_parse_error": None, "state_bits": None}
                trace = None
                if os.path.lexists(output):
                    before = self.c.regular(output, 16*MIB)
                    need(before["identity"][3] == os.getuid() and before["identity"][6] == 1
                         and before["identity"][2] & 0o077 == 0, "private single-link loader output")
                    fd = os.open(output, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
                    try:
                        need(self.c.ident(os.fstat(fd)) == before["identity"], "loader output FD identity")
                        raw = self.c.read(output, 16*MIB)
                        need(sha(raw) == before["sha256"] and self.c.regular(output, 16*MIB) == before,
                             "loader output changed before rename")
                        preserved = directory / (variant + ".json")
                        need(not os.path.lexists(preserved), "loader output proof must be absent")
                        os.rename(output.name, preserved.name, src_dir_fd=directory_fd, dst_dir_fd=directory_fd)
                        directory_guard()
                        after = self.c.regular(preserved, 16*MIB)
                        need(after["identity"][:8] == before["identity"][:8]
                             and after["sha256"] == before["sha256"]
                             and self.c.ident(os.fstat(fd)) == after["identity"],
                             "loader output rename changed more than ctime")
                        self.holds.add(preserved, before["sha256"])
                        row["output"] = {"path": str(preserved), "before_rename": before, "after_rename": after}
                    finally:
                        os.close(fd)
                    try:
                        trace = loads(raw)
                        need(isinstance(trace, list) and len(trace) == 1 and isinstance(trace[0], dict)
                             and set(trace[0]) == {"name", "printed", "rows"}
                             and trace[0]["name"] == "HYUNDAI_PALISADE-torque"
                             and isinstance(trace[0]["rows"], list) and len(trace[0]["rows"]) == 1
                             and isinstance(trace[0]["rows"][0], dict)
                             and isinstance(trace[0]["rows"][0].get("state"), dict), "loader A/B exact one-frame output")
                        row["state_bits"] = bits(trace[0]["rows"][0]["state"])
                        row["matches_existing_native_plain_bits"] = bits(trace) == bits(native_plain)
                        row["reproduces_602_state_bits"] = row["state_bits"] == bits(self.expected_native)
                        row["reproduces_frozen_state_bits"] = row["state_bits"] == bits(self.expected_original)
                    except (ValueError, RuntimeError) as problem:
                        trace = None
                        row["output_parse_error"] = str(problem)
                else:
                    row["output_parse_error"] = "command produced no output file"
                need(not os.path.lexists(output), "loader A/B fixed output not released")
                traces[variant] = trace
                rows.append(row)
                self.save(label + "-receipt.json", {"schema": 1, "diagnostic_only": True, **row, **FLAGS})
                self.holds.add(self.out / (label + "-receipt.json"))
            directory_guard()
        finally:
            os.close(directory_fd)
        valid = all(value is not None for value in traces.values()) and len(traces) == 2
        result = {"schema": 1, "diagnostic_only": True, "source": self.source,
            "same_vm": True, "same_argv": [str(plain), str(output)], "same_cwd": str(self.root),
            "same_held_plain_elf": {"path": str(plain), **plain_seal},
            "same_held_oneframe_input": {"path": str(self.request), **input_seal},
            "common_environment_without_ld_library_path": without_ld,
            "environment_difference_keys": ["LD_LIBRARY_PATH"], "runs": rows,
            "commands_succeeded": all(row["returncode"] == 0 for row in rows), "valid_outputs": valid,
            "output_bits_equal": bits(traces["with-ld"]) == bits(traces["without-ld"]) if valid else None,
            "state_bits_equal": rows[0]["state_bits"] == rows[1]["state_bits"] if valid else None,
            "baseline_plain_output_path_differs": True, "replaces_existing_reproduction_gate": False, **FLAGS}
        self.save("native-loader-environment-comparison.json", result)
        self.holds.add(self.out / "native-loader-environment-comparison.json")
        return result

    def run(self):
        self.budget(force=True)
        for path, pin in SOURCE_PINS.items():
            self.holds.add(self.root / path, pin)
        self.baselines["source"] = self.sources("before")
        self.baselines["sdk"] = self.c.snapshot(self.sdk, self.c.walk(self.sdk), sdk=True)
        self.save("sdk-before.json", self.baselines["sdk"])
        self.import_actual()
        self.prepare_python()
        self.phase = "eager-current-native-builds"
        plain = self.build_trace(False)
        tapped = self.build_trace(True)
        self.clippy()
        self.getter_units()
        for label, root in (("originaldeps", self.deps), ("packages", self.packages), ("numerics", self.numerics),
                            ("python-provider", self.python_prefix)):
            self.baselines[label] = self.snapshot(root)
            self.save(label+"-before.json", self.baselines[label])
        config = self.out / "worker-config.json"
        self.c.save(config, {"root": str(self.root), "source": self.source, "site": str(self.site),
                            "input": str(self.request), "evidence": str(self.out)})
        self.holds.add(config)
        self.save("held-before-runtime.json", self.holds.records())
        traces = {}
        native_env = {**self.env, "PATH": "/nonexistent", "LD_LIBRARY_PATH": str(self.numerics),
                      "CONTROLS_NUMERICS": str(self.numerics),
                      "CONTROLS_ASSETS": str(self.root / "opendbc_repo/opendbc/car/torque_data")}
        for mode in ("original-plain", "original-tapped", "native-plain", "native-tapped"):
            self.phase = mode
            output = self.out / (mode+".json")
            if mode.startswith("original"):
                env = {**self.env, "PATH": "/nonexistent", "LD_LIBRARY_PATH": str(self.python_lib)}
                self.command([self.python, "-I", "-B", "-S", self.root / "tools/ci/controlsd_diagnostic_553.py",
                              "--worker", mode, "--config", config], mode, env=env, seconds=700)
                for suffix in ("-providers-before.json", "-providers-after.json", "-seals.json", "-numpy-dispatch.json"):
                    self.holds.add(self.out / (mode+suffix))
                seals = loads(self.c.read(self.out / (mode+"-seals.json")))
                need(seals["pass"] is True and seals["errors"] == [] and seals["worker_error"] is None,
                     "original current-provider seals")
                if mode == "original-tapped":
                    self.holds.add(self.out / "original-tapped-layers.json")
                    self.holds.add(self.out / "original-tapped-ast.json")
                    self.holds.add(self.out / "original-tapped-getters.json")
            else:
                with self.request.open("rb") as source:
                    self.command([tapped if mode.endswith("tapped") else plain, output], mode,
                                 env=native_env, stdin=source, seconds=600)
            self.holds.add(output)
            traces[mode] = loads(self.c.read(output, 16*MIB))
        original_same = bits(traces["original-plain"]) == bits(traces["original-tapped"])
        native_same = bits(traces["native-plain"]) == bits(traces["native-tapped"])
        original_reproduced = bits(traces["original-plain"][0]["rows"][0]["state"]) == bits(self.expected_original)
        native_reproduced = bits(traces["native-plain"][0]["rows"][0]["state"]) == bits(self.expected_native)
        original = loads(self.c.read(self.out / "original-tapped-layers.json", 32*MIB))
        native, getter_records = [], []
        for line in self.c.read(self.out / "native-tapped.stderr", 32*MIB).splitlines():
            if line.startswith(b"CONTROLSD_553_LAYER "):
                native.append(loads(line.removeprefix(b"CONTROLSD_553_LAYER ")))
            if line.startswith(b"CONTROLSD_553_GETTERS "):
                raw = line.removeprefix(b"CONTROLSD_553_GETTERS ")
                need(len(raw) <= 64*1024, "native getter receipt bound")
                getter_records.append(loads(raw))
        comparison = compare_layers(original, native)
        need(len(getter_records) == 1, "single native same-instance getter receipt")
        self.save("native-tapped-getters.json", getter_records[0])
        self.holds.add(self.out / "native-tapped-getters.json")
        getters = compare_getters(loads(self.c.read(self.out / "original-tapped-getters.json", 64*1024)),
                                  getter_records[0], self.source, self.site, self.numerics, comparison)
        self.save("same-instance-getters.json", getters)
        self.holds.add(self.out / "same-instance-getters.json")
        self.result = {"schema": 1, "scope": "case3/frame0 controlled original/native diagnostic only",
            "current_source": self.source, "actual_failure_source": ACTUAL_SHA,
            "plain_original_reproduces_frozen_state_bits": original_reproduced,
            "plain_native_reproduces_602_state_bits": native_reproduced,
            "original_observation_preserves_result_bits": original_same,
            "native_observation_preserves_result_bits": native_same,
            "observation_valid": original_same and native_same,
            "reduced_replay_reproduced": original_reproduced and native_reproduced,
            "layer_comparison": comparison, "same_instance_getters": getters, **FLAGS}
        # 별도 프로세스지만 같은 VM/held plain ELF/환경으로 이력 효과만 대조한다.
        # prefix는 tap하지 않는다. 기존16호출/단일 Palisade topology 상한은 그대로다.
        self.phase = "native-prefix-plain"
        prefix_output = self.out / "native-prefix-plain.json"
        plain_seal = self.holds.records()[str(plain)]
        need(self.c.regular(plain) == {k: plain_seal[k] for k in ("identity", "sha256")},
             "same held plain ELF before prefix replay")
        with self.prefix_request.open("rb") as source:
            self.command([plain, prefix_output], "native-prefix-plain", env=native_env, stdin=source, seconds=600)
        need(self.c.regular(plain) == {k: plain_seal[k] for k in ("identity", "sha256")},
             "same held plain ELF after prefix replay")
        self.holds.add(prefix_output)
        prefix_trace = loads(self.c.read(prefix_output, 16*MIB))
        need(isinstance(prefix_trace, list) and len(prefix_trace) == 4
             and all(isinstance(c, dict) and set(c) == {"name", "printed", "rows"}
                     and isinstance(c["rows"], list) for c in prefix_trace)
             and [{"name": c["name"], "frames": len(c["rows"])} for c in prefix_trace] == self.prefix_roster,
             "native prefix replay exact four cases/601 rows")
        one_state = traces["native-plain"][0]["rows"][0]["state"]
        prefix_state = prefix_trace[3]["rows"][0]["state"]
        prefix_comparison = {"schema": 1, "diagnostic_only": True, "current_source": self.source,
            "actual_failure_source": ACTUAL_SHA, "cases": 4, "frames": 601,
            "context_frames": 600, "target_case_index": 3, "target_frame_index": 0,
            "same_vm": True, "same_held_plain_elf": {"path": str(plain), **plain_seal},
            "same_native_environment": True, "prefix_tapped": False,
            "prefix_input_sha256": self.c.regular(self.prefix_request)["sha256"],
            "state_bit_encoding": "Float64 little-endian hex; all JSON types preserved",
            "oneframe_state_bits": bits(one_state), "prefix_state_bits": bits(prefix_state),
            "expected_602_state_bits": bits(self.expected_native), "frozen_state_bits": bits(self.expected_original),
            "prefix_equals_oneframe_state_bits": bits(prefix_state) == bits(one_state),
            "prefix_reproduces_602_state_bits": bits(prefix_state) == bits(self.expected_native),
            "prefix_reproduces_frozen_state_bits": bits(prefix_state) == bits(self.expected_original), **FLAGS}
        self.save("native-prefix-comparison.json", prefix_comparison)
        self.holds.add(self.out / "native-prefix-comparison.json")
        self.result["prefix_plain_comparison"] = prefix_comparison
        self.save("diagnostic-before-loader-ab.json", self.result)
        self.holds.add(self.out / "diagnostic-before-loader-ab.json")
        loader = self.loader_environment_comparison(plain, native_env, traces["native-plain"])
        self.result["loader_environment_comparison"] = loader
        self.save("diagnostic.json", self.result)
        need(original_same and native_same, "observation changed output bits; diagnostic is invalid")
        need(original_reproduced and native_reproduced, "one-frame replay did not reproduce historical/602 boundaries")
        need(loader["commands_succeeded"] and loader["valid_outputs"], "loader A/B command or output failed")

    def finish(self, error):
        if self.out is None:
            return False
        self.finalizing = True
        self.deadline = time.monotonic()+240
        failures = []
        missing = {"source", "sdk", "originaldeps", "packages", "numerics", "python-provider"} - self.baselines.keys()
        if missing:
            failures.append({"scope": "baselines", "error": "not established: " + ",".join(sorted(missing))})
        def attempt(label, action):
            try:
                action()
            except BaseException:
                failures.append({"scope": label, "error": traceback.format_exc()})
        if self.children is not None:
            attempt("owned-children", self.children.stop)
        if self.holds is not None:
            def held():
                self.holds.verify()
                self.save("held-after.json", self.holds.records())
            attempt("held", held)
        for label, root in (("source", self.root), ("sdk", getattr(self, "sdk", None)),
                ("originaldeps", getattr(self, "deps", None)), ("packages", getattr(self, "packages", None)),
                ("numerics", getattr(self, "numerics", None)), ("python-provider", getattr(self, "python_prefix", None))):
            if label not in self.baselines:
                continue
            def after(label=label, root=root):
                current = self.sources("after") if label == "source" else (
                    self.c.snapshot(root, self.c.walk(root), sdk=True) if label == "sdk" else self.snapshot(root))
                if label != "source":
                    self.save(label+"-after.json", current)
                need(current == self.baselines[label], f"{label} before/after changed")
            attempt(label, after)
        if self.holds is not None:
            attempt("held-after-finalizers", self.holds.verify)
        attempt("final-growth-budget", lambda: self.budget(force=True))
        live = {"not-established": True}
        def child_scan():
            nonlocal live
            live = self.children.scan()
        if self.children is not None:
            attempt("owned-children-final-scan", child_scan)
        if live:
            failures.append({"scope": "owned-children", "error": "live descendants after finalizers"})
        self.save("children.json", {"subreaper": self.children is not None, "live": list(live)})
        self.save("final-seals.json", {"source": self.source, "pass": not failures,
            "failures": failures, "all_owned_children_reaped": not live})
        success = error is None and not failures and self.result is not None
        self.save("outcome.json", {"schema": 1, "diagnostic_pass": success, "phase": self.phase,
            "error": error, "finalizer_errors": failures, "commands": self.commands,
            "immutable_seals_pass": not failures, "diagnostic": self.result, **FLAGS})
        if success:
            if getattr(self, "same_job_normal_mode", False):
                # 전용 controller의 관측 완료다. normal 실패/602 재현을 성공으로 바꾸지 않는다.
                self.save("complete.json", {"schema": 1, "source": self.source,
                    "diagnostic_only": True, "pass": True, "same_job_normal_mode": True,
                    "normal_status_promoted": False, **FLAGS})
                return success
            self.save("complete.json", {"schema": 1, "source": self.source, "diagnostic_only": True, "pass": True,
                "cases": 1, "frames": 1, "plain_original_matches_historical": True, "plain_native_matches_602": True,
                "tapped_original_equals_plain": True, "tapped_native_equals_plain": True, "kernel_observed": False,
                "historical_original_provider_closure_complete": False, "normal40_acceptance": False,
                "full_runtime_acceptance": False, "retirement_credit": 0})
        return success


class OriginalObservation:
    """원래 ndarray/scalar를 그대로 반환하며 계산에 기록 복사본을 돌려주지 않는다."""
    def __init__(self, np, target_scalars=False):
        self.np = np
        self.calls = []
        self.current = None
        self.layer = None
        self.target_scalars = target_scalars
        self.inputs = []
        self.pid_records = []
        self.pid_pending = None
        self.selected_pid = None

    def push(self, event):
        need(self.current is not None and len(self.current["events"]) < 512, "original observer event bound")
        self.current["events"].append(event)

    def begin(self, model, values):
        need(self.current is None and len(self.calls) < 16 and len(values) <= 18, "original observer call bound")
        need(model.input_size == 18 and [W.shape[1] for W, _, _ in model.layers] == SHAPE[1:], "bound Flux shape")
        self.layer = None
        self.current = {"format": 553, "diagnostic_only": True, "call": len(self.calls),
            "input_f64_bits": [int.from_bytes(struct.pack("<d", float(v)), "little") for v in values], "events": []}
        if self.target_scalars:
            self.inputs.append({"call": len(self.calls), "values": [typed_scalar(v, self.np) for v in values]})
        self.array("mean", model.input_mean)
        self.array("std", model.input_std)
        self.push({"stage": "friction_before", "value": bool(model.friction_override)})

    def array(self, stage, value):
        need(type(value) is self.np.ndarray and value.dtype == self.np.float32 and value.size <= 256,
             "original observer requires bounded actual F32 ndarray")
        self.push({"stage": stage, "layer": self.layer,
            "bits": value.view(self.np.uint32).ravel(order="F").tolist(), "elements": int(value.size),
            "shape": list(value.shape), "strides": list(value.strides),
            "c_contiguous": bool(value.flags.c_contiguous), "f_contiguous": bool(value.flags.f_contiguous),
            "address": int(value.ctypes.data)})
        return value

    def begin_layer(self, W, b, x):
        self.layer = 0 if self.layer is None else self.layer + 1
        need(self.layer < 4, "original layer bound")
        self.array("dot_input", x)
        self.array("weights_fortran_flat", W)
        self.array("bias", b)

    def finish(self, value):
        need(type(value) is float and self.current is not None and self.layer == 3, "original call completion")
        self.current["output_f64_bits"] = int.from_bytes(struct.pack("<d", value), "little")
        self.calls.append(self.current)
        self.current = None
        return value


    def pid_scalar(self, stage, owner, value):
        need(self.target_scalars and stage in ("error", "error_cast", "used_kp", "p"), "bound PID scalar stage")
        if stage == "error":
            need(self.pid_pending is None and len(self.pid_records) < 8, "one-frame PID observation bound")
            self.pid_pending = {"owner": owner, "fields": {}}
        need(self.pid_pending is not None and self.pid_pending["owner"] is owner
             and stage not in self.pid_pending["fields"], "duplicate/foreign PID scalar")
        self.pid_pending["fields"][stage] = typed_scalar(value, self.np)
        if stage == "p":
            need(set(self.pid_pending["fields"]) == {"error", "error_cast", "used_kp", "p"}, "complete PID operands")
            self.pid_records.append(self.pid_pending)
            self.pid_pending = None
        return value

    def select_pid(self, owner):
        rows = [r for r in self.pid_records if r["owner"] is owner]
        need(self.selected_pid is None and self.pid_pending is None and len(rows) == 1,
             "exact actual lateral PID identity at snapshot")
        self.selected_pid = rows[0]["fields"]
        self.pid_records.clear()


def instrument_pid(tree):
    """PID p 식의 각 operand/result를 한 번 읽고 AST 원복 동등성을 검증한다."""
    import copy
    before = ast.dump(tree, include_attributes=False)
    expected = ast.dump(ast.parse("self.p = float(error) * self.k_p").body[0], include_attributes=False)
    changed = 0
    def wrap(stage, value):
        return ast.Call(func=ast.Attribute(value=ast.Name(id="_diag553", ctx=ast.Load()),
                        attr="pid_scalar", ctx=ast.Load()),
                        args=[ast.Constant(stage), ast.Name(id="self", ctx=ast.Load()), value], keywords=[])
    for cls in tree.body:
        if isinstance(cls, ast.ClassDef) and cls.name == "PIDController":
            for method in cls.body:
                if isinstance(method, ast.FunctionDef) and method.name == "update":
                    for node in method.body:
                        if isinstance(node, ast.Assign) and ast.dump(node, include_attributes=False) == expected:
                            error = wrap("error", node.value.left.args[0])
                            cast = ast.Call(func=node.value.left.func, args=[error], keywords=[])
                            node.value = wrap("p", ast.BinOp(left=wrap("error_cast", cast), op=ast.Mult(),
                                                              right=wrap("used_kp", node.value.right)))
                            changed += 1
    need(changed == 1, "exact original PID p expression once")
    class Remove(ast.NodeTransformer):
        def visit_Call(self, node):
            if (isinstance(node.func, ast.Attribute) and isinstance(node.func.value, ast.Name)
                    and node.func.value.id == "_diag553" and node.func.attr == "pid_scalar"):
                need(len(node.args) == 3 and not node.keywords, "identity PID tap shape")
                return self.visit(node.args[-1])
            return self.generic_visit(node)
    tree = ast.fix_missing_locations(tree)
    restored = ast.dump(Remove().visit(copy.deepcopy(tree)), include_attributes=False)
    need(restored == before, "removing PID taps did not restore exact original AST")
    return tree, {"original_ast_sha256": sha(before.encode()), "restored_ast_sha256": sha(restored.encode()),
                  "restored_equals_original": True,
                  "transformed_ast_sha256": sha(ast.dump(tree, include_attributes=False).encode())}


def instrument_flux(tree):
    """정확한 원본 AST에 identity taps만 추가하고 제거 시 전체 AST 동일성을 확인한다."""
    import copy
    original = ast.dump(tree, include_attributes=False)
    def call(method, *args):
        return ast.Call(func=ast.Attribute(value=ast.Name(id="_diag553", ctx=ast.Load()), attr=method, ctx=ast.Load()),
                        args=list(args), keywords=[])
    def wrap(stage, value):
        return call("array", ast.Constant(stage), value)
    class AddTaps(ast.NodeTransformer):
        method = None
        def visit_ClassDef(self, node):
            return self.generic_visit(node) if node.name == "FluxModel" else node
        def visit_FunctionDef(self, node):
            previous, self.method = self.method, node.name
            node = self.generic_visit(node)
            if node.name == "evaluate":
                node.body.insert(0, ast.Expr(call("begin", ast.Name(id="self", ctx=ast.Load()),
                                                 ast.Name(id="input_array", ctx=ast.Load()))))
            self.method = previous
            return node
        def visit_Call(self, node):
            node = self.generic_visit(node)
            if isinstance(node.func, ast.Attribute):
                if self.method == "evaluate" and node.func.attr == "array":
                    return wrap("cast", node)
                if self.method == "forward" and node.func.attr == "dot":
                    return wrap("dot", node)
                if self.method == "sigmoid" and node.func.attr == "exp":
                    return wrap("exp_output", node)
            return node
        def visit_BinOp(self, node):
            node = self.generic_visit(node)
            stage = None
            if self.method == "evaluate":
                right = node.right
                model_field = (isinstance(right, ast.Attribute) and isinstance(right.value, ast.Name)
                               and right.value.id == "self")
                if (isinstance(node.op, ast.Sub) and isinstance(node.left, ast.Name)
                        and node.left.id == "input_array" and model_field and right.attr == "input_mean"):
                    stage = "subtract"
                elif isinstance(node.op, ast.Div) and model_field and right.attr == "input_std":
                    stage = "normalize"
            elif self.method == "forward" and isinstance(node.op, ast.Add):
                stage = "bias_output"
            elif self.method == "sigmoid":
                stage = "denominator" if isinstance(node.op, ast.Add) else "sigmoid" if isinstance(node.op, ast.Div) else None
            return wrap(stage, node) if stage else node
        def visit_UnaryOp(self, node):
            node = self.generic_visit(node)
            return wrap("exp_input", node) if self.method == "sigmoid" and isinstance(node.op, ast.USub) else node
        def visit_For(self, node):
            node = self.generic_visit(node)
            if self.method == "forward":
                need(len(node.body) == 1 and isinstance(node.body[0], ast.Assign), "original forward loop shape")
                node.body.insert(0, ast.Expr(call("begin_layer", *(ast.Name(id=s, ctx=ast.Load()) for s in ("W", "b", "x")))))
                node.body.append(ast.Expr(wrap("layer_output", ast.Name(id="x", ctx=ast.Load()))))
            return node
        def visit_Return(self, node):
            node = self.generic_visit(node)
            if self.method == "evaluate":
                node.value = call("finish", node.value)
            return node
    class RemoveTaps(ast.NodeTransformer):
        def visit_Expr(self, node):
            if isinstance(node.value, ast.Call) and isinstance(node.value.func, ast.Attribute) and isinstance(node.value.func.value, ast.Name) and node.value.func.value.id == "_diag553":
                return None
            return self.generic_visit(node)
        def visit_Call(self, node):
            if isinstance(node.func, ast.Attribute) and isinstance(node.func.value, ast.Name) and node.func.value.id == "_diag553":
                need(node.func.attr in ("array", "finish"), "unknown identity tap")
                return self.visit(node.args[-1])
            return self.generic_visit(node)
    transformed = ast.fix_missing_locations(AddTaps().visit(tree))
    restored = ast.dump(RemoveTaps().visit(copy.deepcopy(transformed)), include_attributes=False)
    need(restored == original, "removing observations did not restore exact original AST")
    return transformed, {"original_ast_sha256": sha(original.encode()),
        "restored_ast_sha256": sha(restored.encode()), "restored_equals_original": True,
        "transformed_ast_sha256": sha(ast.dump(transformed, include_attributes=False).encode())}


def original_worker(args):
    hosted()
    need(sys.version_info[:3] == (3, 12, 15), "actual original interpreter must be CPython 3.12.15")
    root = Path.cwd()
    c = common(root)
    config_path = Path(args.config)
    config = loads(c.read(config_path, MIB))
    root, out, site = (Path(config[k]) for k in ("root", "evidence", "site"))
    need(root == Path.cwd() and config["source"] == os.environ["GITHUB_SHA"] and out.is_relative_to(Path(os.environ["HOME"]).parent / "proof"),
         "original worker binding")
    holds = c.Holds()
    holds.add(config_path)
    holds.add(Path(config["input"]))
    error = None
    prefix = args.worker
    # worker 환경은 원래 env-i 범위를 유지한다. 새 flag 대신 기존 owned job/source marker로 한정한다.
    owner_path = Path(os.environ["HOME"]).parent / "owner"
    source_owner = root.parent / ".diag296-owner"
    holds.add(owner_path)
    holds.add(source_owner)
    owner = c.read(owner_path, 256).decode("ascii").strip()
    need(re.fullmatch(r"[1-9][0-9]*:[1-9][0-9]*:[a-z0-9-]+", owner)
         and c.read(source_owner, 256).decode("ascii").strip() == owner,
         "original worker existing owned job/source binding")
    target_scalars = args.worker == "original-tapped" and owner.endswith(":controls-same-job-553")
    before = None
    def provider_files():
        files = {str(Path(sys.executable).resolve())}
        for module in list(sys.modules.values()):
            for key in ("__file__", "__cached__"):
                name = getattr(module, key, None)
                if name and Path(name).is_file():
                    files.add(str(Path(name).resolve()))
        mapped = set()
        for line in Path("/proc/self/maps").read_text().splitlines():
            row = line.split(None, 5)
            if len(row) == 6 and "x" in row[1] and row[5].startswith("/"):
                need(not row[5].endswith(" (deleted)"), "deleted provider mapping")
                p = Path(row[5]).resolve(strict=True)
                major, minor = (int(x, 16) for x in row[3].split(":"))
                need((p.stat().st_dev, p.stat().st_ino) == (os.makedev(major, minor), int(row[4])), "provider mapping identity")
                files.add(str(p))
                mapped.add(str(p))
        for name in sorted(files):
            holds.add(Path(name))
        libpython = [p for p in mapped if Path(p).name.startswith("libpython3.12")]
        need(len(libpython) == 1 and Path(libpython[0]).parent == Path(sys.base_prefix) / "lib", "actual libpython path")
        return {"files": sorted(files), "executable_mappings": sorted(mapped), "libpython": libpython,
                "python_version": sys.version, "base_prefix": sys.base_prefix, **FLAGS}
    try:
        for name, pin in SOURCE_PINS.items():
            holds.add(root / name, pin)
            if name.endswith(".py"):
                need(not Path(importlib.util.cache_from_source(str(root / name))).exists(), "critical source bytecode cache forbidden")
        sys.path[:0] = [str(root / "rust/tools"), str(root), str(root / "opendbc_repo"), str(site)]
        import numpy as np
        need(np.__version__ == "2.5.3" and Path(np.__file__).resolve().is_relative_to(site), "private pinned NumPy")
        for name, pin in DSOS.items():
            holds.add(site / "numpy.libs" / name, pin, elf=True)
        observation = OriginalObservation(np, target_scalars)
        ast_record = None
        pid_ast_record = None
        if args.worker == "original-tapped":
            from importlib import abc, machinery
            interfaces = root / "opendbc_repo/opendbc/car/interfaces.py"
            pid_source = root / "openpilot/common/pid.py"
            class Loader(machinery.SourceFileLoader):
                def get_code(self, fullname):
                    nonlocal ast_record, pid_ast_record
                    path = pid_source if fullname == "openpilot.common.pid" else interfaces
                    raw = c.read(path, 4*MIB)
                    need(sha(raw) == SOURCE_PINS[str(path.relative_to(root))], "instrumented original source pin")
                    tree = ast.parse(raw, filename=str(path))
                    if path == pid_source:
                        tree, pid_ast_record = instrument_pid(tree)
                        changed = raw.replace(b"self.p = float(error) * self.k_p", b"self.p = float(error) + self.k_p")
                        need(changed != raw, "PID operator negative source anchor")
                        try:
                            instrument_pid(ast.parse(changed))
                        except RuntimeError:
                            pid_ast_record["changed_p_operator_rejected"] = True
                        else:
                            raise RuntimeError("changed original PID p operator accepted")
                    else:
                        tree, ast_record = instrument_flux(tree)
                    return compile(tree, str(path), "exec")
                def exec_module(self, module):
                    module.__dict__["_diag553"] = observation
                    super().exec_module(module)
            class Finder(abc.MetaPathFinder):
                def find_spec(self, fullname, path=None, target=None):
                    if fullname == "opendbc.car.interfaces":
                        return importlib.util.spec_from_file_location(fullname, interfaces, loader=Loader(fullname, str(interfaces)))
                    if target_scalars and fullname == "openpilot.common.pid":
                        return importlib.util.spec_from_file_location(fullname, pid_source, loader=Loader(fullname, str(pid_source)))
                    return None
            need("opendbc.car.interfaces" not in sys.modules, "original interfaces imported before observer")
            if target_scalars:
                need("openpilot.common.pid" not in sys.modules, "original PID imported before observer")
            sys.meta_path.insert(0, Finder())
        import controlsd_source as adapter
        original_load = adapter.load
        def load(*values):
            nonlocal before
            result = original_load(*values)
            need(before is None, "single original adapter load")
            before = provider_files()
            c.save(out / (prefix+"-providers-before.json"), {**before, "held": holds.records()})
            return result
        adapter.load = load
        if target_scalars:
            original_snapshot = adapter.snapshot
            def snapshot(controls):
                observation.select_pid(controls.LaC.pid)
                return original_snapshot(controls)
            adapter.snapshot = snapshot
        request = loads(c.read(Path(config["input"]), MIB))
        need(len(request["cases"]) == 1 and len(request["cases"][0]["frames"]) == 1, "one-frame worker scope")
        if target_scalars:
            need(request["cases"][0]["name"] == "HYUNDAI_PALISADE-torque", "exact target scalar request")
        result = [adapter.trace(request["cases"][0])]
        need(before is not None, "actual original adapter was not reached")
        c.write_new(out / (prefix+".json"), (json.dumps(result, allow_nan=False)+"\n").encode())
        if args.worker == "original-tapped":
            need(ast_record is not None and observation.current is None and observation.calls, "complete original tap records")
            c.save(out / "original-tapped-layers.json", observation.calls)
            c.save(out / "original-tapped-ast.json", ast_record)
            if target_scalars:
                need(pid_ast_record is not None and observation.selected_pid is not None,
                     "actual target PID AST and single selected record")
                scalar = {"schema": 1, "observer": "original", "case": request["cases"][0]["name"], "frame": 0,
                    "fields": observation.selected_pid, "inputs": observation.inputs, "used_kp_scalar_observed": True}
                need(len(json.dumps(scalar, allow_nan=False).encode()) <= 64*1024, "original scalar 64KiB cap")
                c.save(out / "original-tapped-scalars.json", scalar)
                c.save(out / "original-tapped-pid-ast.json", pid_ast_record)
            need(len(observation.calls) == 4, "four completed calls before original getters")
            original_getters(c, holds, before, provider_files, site, out, config["source"])
        dispatch = np.lib.introspect.opt_func_info(func_name="^(exp|subtract|add|divide)$", signature="float32")
        c.save(out / (prefix+"-numpy-dispatch.json"), {"ufunc_dispatch": dispatch,
            "actual_blas_kernel_observed": False, "environment": {k: os.environ.get(k) for k in
            ("OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "OPENBLAS_CORETYPE", "NPY_DISABLE_CPU_FEATURES")}})
    except BaseException:
        error = traceback.format_exc()
        raise
    finally:
        errors = []
        try:
            after = provider_files()
            holds.verify()
            c.save(out / (prefix+"-providers-after.json"), {**after, "held": holds.records(),
                "first_observed_after": sorted(set(after["files"])-set(before["files"])) if before else after["files"]})
        except BaseException:
            errors.append(traceback.format_exc())
        try:
            c.save(out / (prefix+"-seals.json"), {"pass": not errors, "errors": errors, "worker_error": error, **FLAGS})
        finally:
            holds.close()
        need(not errors, "original current-provider final seals failed")


def original_getters(c, holds, before, providers, site, out, source):
    """계산/결과 저장 후 이미 로드된 extension/provider reference만 빌린다."""
    record = {"schema": 553, "diagnostic_only": True, "observer": "original-tapped",
              "phase": "after-output-flush", "source": source, "pid": os.getpid(), "pass": False, **FLAGS}
    handles, close, failure = [], None, None
    def mappings():
        with open("/proc/self/maps", "rb") as stream:
            raw = stream.read(2*MIB+1)
        need(len(raw) <= 2*MIB and raw.endswith(b"\n"), "getter maps bound/EOF")
        rows = []
        for line in raw.decode().splitlines():
            fields = line.split(None, 5)
            need(len(fields) >= 5 and len(fields[1]) == 4, "getter maps fields")
            start, end = (int(v, 16) for v in fields[0].split("-"))
            major, minor = (int(v, 16) for v in fields[3].split(":"))
            need(start < end, "getter maps range")
            rows.append({"start": start, "end": end, "permissions": fields[1],
                         "offset": int(fields[2], 16), "major": major, "minor": minor,
                         "inode": int(fields[4]), "path": fields[5] if len(fields) == 6 else ""})
        return rows
    def executable(rows):
        return [r for r in rows if "x" in r["permissions"] and r["path"].startswith("/")]
    def readable(rows, address, size):
        selected = [r for r in rows if r["start"] <= address < r["end"]]
        need(address and size > 0 and len(selected) == 1 and "r" in selected[0]["permissions"]
             and size <= selected[0]["end"]-address, "getter bounded readable pointer")
        return selected[0]
    def tasks():
        result = []
        for path in Path("/proc/self/task").iterdir():
            need(path.name.isascii() and path.name.isdecimal() and len(result) < 1024, "getter task roster")
            row = c.proc_stat(int(path.name))
            result.append([row["pid"], row["start"]])
        return sorted(result)
    try:
        ctypes = sys.modules.get("ctypes")
        extension = sys.modules.get("numpy._core._multiarray_umath")
        need(ctypes is not None and "_ctypes" in sys.modules and extension is not None,
             "preexisting ctypes/NumPy extension required; no import fallback")
        api = ctypes.__dict__.get("pythonapi")
        need(api is not None and api._handle, "preexisting pythonapi handle required")
        pre = providers()
        need(set(pre["executable_mappings"]) == set(before["executable_mappings"]),
             "provider changed before post-compute getters")
        rows = mappings()
        before_exec, before_tasks = executable(rows), tasks()
        extension_path = Path(extension.__file__).resolve(strict=True)
        provider = site / "numpy.libs/libscipy_openblas64_-f48b354e.so"
        need(provider.resolve(strict=True) == provider and str(provider) in before["executable_mappings"]
             and str(extension_path) in before["executable_mappings"], "preexisting exact numerical mappings")
        held = holds.records()[str(provider)]
        need(held["sha256"] == DSOS[provider.name], "same held original numerical pin")
        device, inode = held["identity"][:2]
        def belongs(address, execute=False):
            need(type(address) is int and address > 0, "NULL/invalid getter pointer")
            selected = [r for r in rows if r["start"] <= address < r["end"]]
            need(address and len(selected) == 1, "getter pointer unmapped/ambiguous")
            row = selected[0]
            need(row["path"] == str(provider) and row["inode"] == inode
                 and os.makedev(row["major"], row["minor"]) == device
                 and ("x" if execute else "r") in row["permissions"], "getter pointer/provider FD mismatch")
            return row
        def loader(name, restype, argtypes):
            fn = api[name]  # preexisting handle의 fresh wrapper; CDLL/dlopen wrapper 생성0.
            address = ctypes.cast(fn, ctypes.c_void_p).value
            selected = [r for r in before_exec if r["start"] <= address < r["end"]]
            need(len(selected) == 1 and selected[0]["path"] in before["executable_mappings"],
                 "loader function must already be mapped and held")
            fn.restype, fn.argtypes = restype, argtypes
            return fn
        pointer = ctypes.c_void_p
        integer = ctypes.c_int
        dlopen = loader("dlopen", pointer, [ctypes.c_char_p, integer])
        dlsym = loader("dlsym", pointer, [pointer, ctypes.c_char_p])
        dlinfo = loader("dlinfo", integer, [pointer, integer, pointer])
        dladdr = loader("dladdr", integer, [pointer, pointer])
        close = loader("dlclose", integer, [pointer])
        class DlInfo(ctypes.Structure):
            _fields_ = [("name", pointer), ("base", pointer), ("symbol", pointer), ("address", pointer)]
        class LinkMap(ctypes.Structure):
            _fields_ = [("base", ctypes.c_ulong), ("name", pointer), ("dynamic", pointer),
                        ("next", pointer), ("previous", pointer)]
        def existing(path):
            # _ctypes.dlopen would OR RTLD_NOW; call the existing loader function directly.
            mode = os.RTLD_NOLOAD | os.RTLD_LAZY
            need(not mode & os.RTLD_NOW, "NOLOAD|LAZY only")
            handle = dlopen(os.fsencode(path), mode)
            need(handle, "RTLD_NOLOAD miss; no load fallback")
            handles.append(handle)
            namespace, link = ctypes.c_long(), pointer()
            need(dlinfo(handle, 1, ctypes.byref(namespace)) == 0 and namespace.value == 0
                 and dlinfo(handle, 2, ctypes.byref(link)) == 0, "same base namespace/link-map")
            readable(mappings(), link.value, ctypes.sizeof(LinkMap))
            base = ctypes.cast(link, ctypes.POINTER(LinkMap)).contents.base
            return handle, {"namespace": namespace.value, "link_map": link.value, "load_base": base}
        extension_handle, extension_info = existing(extension_path)
        provider_handle, provider_info = existing(provider)
        base = provider_info["load_base"]
        need(base and any(r["path"] == str(provider) and r["start"] == base and r["offset"] == 0 for r in rows),
             "provider base must preexist")
        def symbol(handle, name):
            address = dlsym(handle, name.encode()+b"\0")
            need(address, "required same-instance symbol missing")
            belongs(address, True)
            info = DlInfo()
            need(dladdr(address, ctypes.byref(info)) and info.base == base, "different provider instance")
            return address
        symbols = {}
        for name in ("scipy_cblas_sgemv64_", "scipy_cblas_sdot64_"):
            address = symbol(extension_handle, name)
            need(address == symbol(provider_handle, name), "extension/provider lookup binding differs")
            symbols[name] = address
        for name in ("scipy_openblas_get_config64_", "scipy_openblas_get_corename64_", "scipy_openblas_get_num_threads64_"):
            symbols[name] = symbol(provider_handle, name)
        def string(name):
            address = ctypes.PYFUNCTYPE(pointer)(symbols[name])()
            need(type(address) is int and address > 0, "NULL getter string")
            row = readable(rows, address, 1)
            info = DlInfo()
            need(dladdr(address, ctypes.byref(info)) and info.base == base, "getter string belongs to another image")
            # static config의 BSS는 inode0 anonymous map일 수 있다. same dladdr base를 필수로 한다.
            if row["inode"] or row["path"]:
                belongs(address)
            raw = ctypes.string_at(address, min(4096, row["end"]-address))
            end = raw.find(b"\0")
            need(end > 0, "getter string NULL/empty/unterminated")
            return list(raw[:end])
        config_bytes = string("scipy_openblas_get_config64_")
        core_bytes = string("scipy_openblas_get_corename64_")
        thread_count = ctypes.PYFUNCTYPE(integer)(symbols["scipy_openblas_get_num_threads64_"])()
        need(thread_count > 0, "invalid getter thread count")
        after = mappings()
        need(before_exec == executable(after), "getter executable mappings changed")
        holds.verify()
        record["observation"] = {"library_handle_source": "RTLD_NOLOAD|RTLD_LAZY", "new_library_opened": False,
            "provider": {"path": str(provider), **held, **provider_info}, "extension": extension_info,
            "symbol_addresses": symbols, "config_bytes": config_bytes, "core_bytes": core_bytes, "threads": thread_count,
            "executable_mappings_before": before_exec, "executable_mappings_after": executable(after),
            "tasks_before": before_tasks, "tasks_after": tasks(), "provider_fd_held_through_observation": True,
            "executable_mapping_roster_equal": True, "thread_lifecycle_complete": False,
            "process_start_ticks": c.proc_stat(os.getpid())["start"],
            "string_pointer_scope": "same dladdr image base; readable file mapping or anonymous BSS",
            "cpu_environment": {k: os.environ.get(k) for k in
                ("OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "OPENBLAS_CORETYPE", "NPY_DISABLE_CPU_FEATURES")},
            "lookup_is_not_actual_blas_entry_observation": True}
        record["pass"] = True
    except BaseException:
        failure = traceback.format_exc()
    finally:
        try:
            close_errors = []
            for handle in reversed(handles):
                try:
                    need(close is not None and close(handle) == 0, "NOLOAD reference close failed")
                except BaseException:
                    close_errors.append(traceback.format_exc())
            need(not close_errors, "\n".join(close_errors))
            if handles:
                need(before_exec == executable(mappings()), "mapping changed after NOLOAD reference close")
            holds.verify()
        except BaseException:
            failure = (failure or "") + traceback.format_exc()
        if failure:
            record["pass"], record["error"] = False, failure
        raw = (json.dumps(record, allow_nan=False)+"\n").encode()
        if len(raw) > 64*1024:
            failure = "getter receipt exceeds 64 KiB"
            record = {k: v for k, v in record.items() if k != "observation"}
            record.update({"pass": False, "error": failure})
            raw = (json.dumps(record, allow_nan=False)+"\n").encode()
        need(len(raw) <= 64*1024, "getter failure receipt bound")
        c.write_new(out / "original-tapped-getters.json", raw)
    need(failure is None and record["pass"], "original same-instance getter observation failed")


def compare_getters(original, native, source, site, numerics, layers):
    need(layers["original_calls"] == layers["native_calls"] == 4, "four completed calls per getter process")
    for record, observer, path in ((original, "original-tapped", site / "numpy.libs"),
                                   (native, "native-tapped", numerics)):
        need(record.get("schema") == 553 and record.get("diagnostic_only") is True
             and record.get("observer") == observer and record.get("phase") == "after-output-flush"
             and record.get("source") == source and record.get("pass") is True
             and type(record.get("pid")) is int and record["pid"] > 0, "getter receipt source/phase/health")
        need(all(type(record.get(k)) is type(v) and record[k] == v for k, v in FLAGS.items()), "getter scope flags")
        value = record["observation"]
        need(value.get("new_library_opened") is False and value.get("provider_fd_held_through_observation") is True
             and value.get("executable_mapping_roster_equal") is True
             and value["executable_mappings_before"] == value["executable_mappings_after"]
             and value["provider"]["path"] == str(path / "libscipy_openblas64_-f48b354e.so")
             and value["provider"]["sha256"] == DSOS["libscipy_openblas64_-f48b354e.so"], "same-instance provider receipt")
        start = str(value["process_start_ticks"])
        need(start.isascii() and start.isdecimal() and all(any(tid == record["pid"] and str(ticks) == start
             for tid, ticks in value[key]) for key in ("tasks_before", "tasks_after")), "getter process starttime binding")
        for key in ("config_bytes", "core_bytes"):
            raw = value[key]
            need(isinstance(raw, list) and 0 < len(raw) < 4096
                 and all(type(b) is int and 0 < b <= 255 for b in raw), "bounded raw getter string")
        need(type(value["threads"]) is int and value["threads"] > 0, "C int thread getter value")
    left, right = original["observation"], native["observation"]
    return {"schema": 553, "diagnostic_only": True, "source": source,
            "observation_valid": True, "completed_calls_per_process": 4,
            "same_reported_config": left["config_bytes"] == right["config_bytes"],
            "same_reported_core": left["core_bytes"] == right["core_bytes"],
            "same_reported_threads": left["threads"] == right["threads"],
            "different_pids_not_same_cross_process_instance": True,
            "getters_do_not_observe_past_blas_calls": True,
            "original_receipt": "original-tapped-getters.json", "native_receipt": "native-tapped-getters.json", **FLAGS}


def observation_index(calls, original):
    need(isinstance(calls, list) and 0 < len(calls) <= 16, "observed call count")
    result, metadata = {}, []
    vector_stages = {"mean", "std", "cast", "dot_input", "weights_fortran_flat", "bias", "dot"}
    scalar_stages = {"subtract", "normalize", "bias_output", "exp_input", "exp_output", "denominator", "sigmoid", "layer_output"}
    for call_id, call in enumerate(calls):
        need(set(call) == {"format", "diagnostic_only", "call", "input_f64_bits", "events", "output_f64_bits"}
             and call["format"] == 553 and call["diagnostic_only"] is True
             and type(call["call"]) is int and call["call"] == call_id,
             "observation envelope")
        def put(layer, stage, i, value, width=32):
            need(type(value) is int and 0 <= value < 1 << width, "raw bit width/type")
            key = (call_id, layer, stage, i)
            need(key not in result, "duplicate observed stage/element")
            result[key] = value
        inputs = call["input_f64_bits"]
        need(isinstance(inputs, list) and 2 <= len(inputs) <= 18, "observed input dimensions")
        for i, value in enumerate(inputs):
            put(-1, "input_f64", i, value, 64)
        put(4, "output_f64", 0, call["output_f64_bits"], 64)
        need(isinstance(call["events"], list) and len(call["events"]) <= 512, "observed event bound")
        for event in call["events"]:
            stage = event["stage"]
            if stage == "friction_before":
                need(set(event) == {"stage", "value"} and type(event["value"]) is bool, "friction record")
                put(-1, stage, 0, int(event["value"]))
                continue
            layer = -1 if event["layer"] is None else event["layer"]
            need(type(layer) is int and -1 <= layer < 4, "layer index")
            need(stage in vector_stages | scalar_stages, "unknown observation stage")
            if original or stage in vector_stages:
                fields = {"stage", "layer", "bits", "elements", "address"}
                fields |= {"shape", "strides", "c_contiguous", "f_contiguous"} if original else {"stride_bytes"}
                need(set(event) == fields and type(event["address"]) is int
                     and 0 < event["address"] < 1 << 64 and event["address"] % 4 == 0, "vector metadata schema")
                need(type(event["elements"]) is int and 0 < event["elements"] <= 256
                     and isinstance(event["bits"], list) and len(event["bits"]) == event["elements"], "vector record")
                if original:
                    n = 18 if layer == -1 else SHAPE[layer]
                    m = 18 if layer == -1 else SHAPE[layer+1]
                    shape = [18] if stage == "cast" else [n, m] if stage == "weights_fortran_flat" else [1, n] if stage == "dot_input" else [1, m]
                    need(event["shape"] == shape and isinstance(event["strides"], list)
                         and len(event["strides"]) == len(shape) and all(type(s) is int and s > 0 and s % 4 == 0 for s in event["strides"])
                         and type(event["c_contiguous"]) is bool and type(event["f_contiguous"]) is bool,
                         "original actual shape/strides/flags")
                    count = 1
                    for width in shape:
                        count *= width
                    need(count == event["elements"], "original shape/element count")
                    if stage == "weights_fortran_flat":
                        need(event["f_contiguous"] is True, "original transposed W must be F-contiguous")
                else:
                    need(event["stride_bytes"] == 4, "native vector stride")
                for i, value in enumerate(event["bits"]):
                    put(layer, stage, i, value)
                metadata.append({"call": call_id, **{k: v for k, v in event.items() if k != "bits"}})
            else:
                need(set(event) == {"stage", "layer", "index", "bits"} and type(event["index"]) is int
                     and 0 <= event["index"] < 18, "scalar record")
                put(layer, stage, event["index"], event["bits"])
        expected = {(call_id, -1, "input_f64", i) for i in range(len(inputs))}
        expected.add((call_id, 4, "output_f64", 0))
        expected.add((call_id, -1, "friction_before", 0))
        for stage in ("mean", "std", "cast", "subtract", "normalize"):
            expected.update((call_id, -1, stage, i) for i in range(18))
        for layer, (n, m) in enumerate(zip(SHAPE, SHAPE[1:])):
            for stage, count in (("dot_input", n), ("weights_fortran_flat", n*m), ("bias", m), ("dot", m),
                                 ("bias_output", m), ("layer_output", m)):
                expected.update((call_id, layer, stage, i) for i in range(count))
            if layer < 2:
                for stage in ("exp_input", "exp_output", "denominator", "sigmoid"):
                    expected.update((call_id, layer, stage, i) for i in range(m))
        need({k for k in result if k[0] == call_id} == expected, "incomplete/extra observation stage roster")
    return result, metadata


def compare_layers(original, native):
    left, left_meta = observation_index(original, True)
    right, right_meta = observation_index(native, False)
    stages = ["input_f64", "mean", "std", "friction_before", "cast", "subtract", "normalize", "dot_input",
              "weights_fortran_flat", "bias", "dot", "bias_output", "exp_input", "exp_output", "denominator",
              "sigmoid", "layer_output", "output_f64"]
    ordered = sorted(left.keys() | right.keys(), key=lambda k: (k[0], k[1], stages.index(k[2]), k[3]))
    differences = [k for k in ordered if k not in left or k not in right or left[k] != right[k]]
    first = differences[0] if differences else None
    return {"original_calls": len(original), "native_calls": len(native), "compared_bit_fields": len(ordered),
        "different_bit_fields": len(differences),
        "first_divergence": {"call": first[0], "layer": first[1], "stage": first[2], "element": first[3],
            "original_bits": left.get(first), "native_bits": right.get(first),
            "boundary": "missing_call_or_input_element" if first not in left or first not in right else "bit_value"} if first else None,
        "comparison_criteria": "exact per-call/per-layer F32/F64 bits; explicit Fortran-flat tensor ordering; layouts retained separately",
        "original_layouts": left_meta, "native_layouts": right_meta, **FLAGS}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root")
    parser.add_argument("--python")
    parser.add_argument("--evidence")
    parser.add_argument("--source-commit")
    parser.add_argument("--worker", choices=("original-plain", "original-tapped"), help=argparse.SUPPRESS)
    parser.add_argument("--config", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker:
        need(args.config, "worker config required")
        original_worker(args)
        return 0
    need(all((args.root, args.python, args.evidence, args.source_commit)), "explicit controller interface required")
    signal.signal(signal.SIGTERM, lambda number, frame: (_ for _ in ()).throw(InterruptedError(f"#553 signal {number}")))
    signal.signal(signal.SIGINT, lambda number, frame: (_ for _ in ()).throw(InterruptedError(f"#553 signal {number}")))
    probe = Diagnostic()
    probe.out = None
    probe.holds = probe.children = None
    error = None
    passed = False
    try:
        probe.initialize(args)
        probe.run()
    except BaseException:
        error = traceback.format_exc()
        print(error, file=sys.stderr)
    finally:
        try:
            passed = probe.finish(error)
        except BaseException:
            print(traceback.format_exc(), file=sys.stderr)
        finally:
            if probe.holds is not None:
                probe.holds.close()
            if probe.children is not None:
                os.close(probe.children.journal)
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
