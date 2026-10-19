#!/usr/bin/env python3
"""GitHub-hosted exact-source AGNOS delivery. Never run on a workstation."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import time
import zipfile

assert os.environ.get("GITHUB_ACTIONS") == "true", "runtime validation/builds belong in GitHub Actions"
assert os.environ.get("GITHUB_REPOSITORY") == "bin9208/openpilot-rust"
ROOT = Path(os.environ["GITHUB_WORKSPACE"])
WORK = Path(os.environ["RUNNER_TEMP"]) / "agnos-runtime"
WORK.mkdir(exist_ok=True)
CONFIG = ROOT / "rust/ci/runtime"
SHA = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
BASE = "3496c4bbf300e7487d17b2bf25afc170584dfe02"
SDK = WORK / "sdk"
TARGET = WORK / "target"
HOST_TARGET = TARGET / "host-tools"
TOOLS = HOST_TARGET / "release"
RELEASE = WORK / "release"
BUILD = json.loads((CONFIG / "build-args.json").read_text())
NAMES = [BUILD[i + 1] for i, value in enumerate(BUILD) if value == "--bin"]


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def run(args, **kwargs):
    print("+", shlex.join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), cwd=kwargs.pop("cwd", ROOT), check=True, **kwargs)


def room(growth=0):
    free = shutil.disk_usage(WORK).free
    assert free >= 25 * 2**30 + growth, f"disk reserve: free={free}, growth={growth}"


def download(url, name, expected):
    path = WORK / name
    room(2 * 2**30)
    if not path.exists() or digest(path) != expected:
        run(["curl", "--fail", "--location", "--retry", "5", "--retry-all-errors", "--output", path, url])
    assert digest(path) == expected, name
    return path


def unpack(path, destination):
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(path) as archive:
        archive.extractall(destination, filter="data")


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def stage_baseline():
    assert not git("status", "--porcelain")
    changed = git("diff", "--name-only", BASE, SHA).splitlines()
    allowed = ("AGENTS.md", "launch_openpilot.sh", "rust/Cargo.lock", "rust/crates/jetlink/", "rust/crates/runtime-core/", "rust/crates/runtime-launcher/", "rust/crates/dev-tools/src/publish.rs", "rust/ci/runtime/", "rust/upstream/jetlink.json", "docs/naver/dev-md-jetlink-", ".github/workflows/jetlink-native.yml", ".github/workflows/dev-md-runtime.yml", "tools/check_jetlink_usb_policy.py")
    assert all(any(path == prefix or prefix.endswith(("/", "-")) and path.startswith(prefix) for prefix in allowed) for path in changed), changed
    archive = download(f"https://github.com/bin9208/openpilot-rust/releases/download/runtime-{BASE}/openpilot-rust-{BASE}-aarch64.tar.gz", "baseline.tar.gz", "eb67c8c58f8c8da62aa7afdc11d7cfc1262038222b6e4b9ff0a33fe64206297d")
    base = WORK / "baseline"
    room(4 * 2**30)
    unpack(archive, base)
    manifest_file = base / "rust/runtime-manifest.json"
    assert digest(manifest_file) == "041be84b1e7ea8d6611e4256e4ae30814eca1cdef498dffa6d455c0d1962787e"
    manifest = json.loads(manifest_file.read_text())
    replaced = {f"rust/bin/{name}" for name in NAMES} | {f"rust/bin/licenses/runtime-build/{name}.json" for name in ("runtime-staging", "installer-build", "source-build-metadata", "provider-reuse", "elf-summary")}
    reused = []
    for row in manifest["files"]:
        path = row["path"]
        source = base / path
        assert digest(source) == row["sha256"], path
        if path in replaced:
            continue
        dest = ROOT / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        if dest.is_symlink() or dest.exists():
            dest.unlink()
        if source.is_symlink():
            assert source.resolve().is_relative_to(base) and not Path(os.readlink(source)).is_absolute()
            dest.symlink_to(os.readlink(source))
        else:
            shutil.copy2(source, dest)
        reused.append(row)
    assets = [row["path"] for row in reused if not row["path"].startswith("rust/bin/")]
    run(["git", "add", "--renormalize", "--", *assets])
    assert not git("status", "--porcelain"), "baseline source assets differ from this exact source"
    build = ROOT / "rust/bin/licenses/runtime-build"
    metadata = json.loads((base / "rust/bin/licenses/runtime-build/source-build-metadata.json").read_text())
    metadata["openpilot"].update(git_commit=SHA, git_commit_date=git("show", "-s", "--format=%ct %ci", "HEAD"), git_origin=git("remote", "get-url", "origin"), is_dirty=False)
    save(build / "source-build-metadata.json", metadata)
    save(build / "provider-reuse.json", dict(schema=1, baseline_source=BASE, source_commit=SHA, changed_source_paths=changed, runtime_executables_to_rebuild=len(NAMES), artifacts=reused,
         note="Every companion/model/firmware/source asset is verified against the sealed public baseline; allowed changes do not touch its producers. All project Rust executables are rebuilt on the exact clean source. External provider receipts retain their real original identity."))
    shutil.rmtree(base)
    archive.unlink()


def sysroot():
    if (SDK / "VERSION").exists():
        assert (SDK / "VERSION").read_text().strip() == "19.8-carrot-bt1"
        isolate_providers()
        return
    room(10 * 2**30)
    image = download("https://github.com/ajouatom/agnos-builder/releases/download/agnos-19.8-carrot-bt1/system-375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44.img.xz", "system.img.xz", "077d42186def98025ffad9723774c96f45da10c87cc6fedbc58d0cf4fa65ffd4")
    sparse = WORK / "system.sparse"
    with sparse.open("wb") as output:
        run(["xz", "--decompress", "--stdout", image], stdout=output)
    assert digest(sparse) == "a67410a888105ecce7bcc05e17cda77fd10572aa4380e9de242a7c92f3497114"
    raw = WORK / "system.raw"
    run(["simg2img", sparse, raw])
    assert digest(raw) == "375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44"
    SDK.mkdir(parents=True, exist_ok=True)
    # Extract the genuine image tree; debugfs preserves its linker scripts,
    # usr-merge aliases, C/C++ headers and native libraries together.
    with (WORK / "sysroot-extract.log").open("w") as diagnostics:
        run(["debugfs", "-R", f"rdump / {SDK}", raw], stdout=subprocess.DEVNULL, stderr=diagnostics)
    assert (SDK / "VERSION").read_text().strip() == "19.8-carrot-bt1"
    for path in SDK.rglob("*"):
        if path.is_symlink():
            target = os.readlink(path)
            if target.startswith("/"):
                path.unlink()
                path.symlink_to(os.path.relpath(SDK / target.lstrip("/"), path.parent))
    image.unlink(); sparse.unlink(); raw.unlink()
    isolate_providers()


def isolate_providers():
    # The complete image may also contain development libraries. Do not let
    # their generic linker names shadow the explicitly pinned providers used
    # by this runtime (especially FFmpeg struct ABI and static JPEG/libyuv).
    library = SDK / "usr/lib/aarch64-linux-gnu"
    for pattern in ("libavcodec*", "libavformat*", "libavutil*", "libavdevice*", "libavfilter*", "libswscale*", "libswresample*", "libpostproc*", "libopencv*", "libjpeg.a", "libyuv.a", "libonnxruntime*"):
        for path in library.glob(pattern):
            if path.is_file() or path.is_symlink(): path.unlink()


def providers():
    # Only compile-time headers/static libyuv are restored. Runtime companions
    # remain the exact hash-verified AGNOS baseline providers.
    lib = ROOT / "rust/bin/lib"
    cv = WORK / "opencv"
    source_tar = download("https://codeload.github.com/opencv/opencv/tar.gz/fe38fc608f6acb8b68953438a62305d8318f4fcd", "opencv.tar.gz", "6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61")
    unpack(source_tar, cv)
    source = cv / "source"
    if not source.exists():
        (cv / "opencv-fe38fc608f6acb8b68953438a62305d8318f4fcd").rename(source)
        run(["patch", "-p1", "-i", ROOT / "rust/crates/opencv-runtime/native/opencv-defined-access.patch"], cwd=source)
    include = cv / "install/include/opencv4"
    include.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source / "include", include, dirs_exist_ok=True)
    for module in (source / "modules").iterdir():
        if (module / "include").exists():
            shutil.copytree(module / "include", include, dirs_exist_ok=True)
    shutil.copy2(CONFIG / "opencv-headers.cmake", cv / "restore.cmake")
    run(["cmake", "-P", cv / "restore.cmake"])
    (cv / "install/lib").mkdir(parents=True, exist_ok=True)
    for path in lib.glob("libopencv*"):
        dest = cv / "install/lib" / path.name
        if dest.exists() or dest.is_symlink(): dest.unlink()
        dest.symlink_to(path.resolve())
    shutil.copy2(CONFIG / "opencv.sha256", cv / "native-libraries.sha256")
    run(["sha256sum", "--check", "native-libraries.sha256"], cwd=cv, stdout=subprocess.DEVNULL)
    ort_pin = json.loads((ROOT / "rust/upstream/signal-runtime.json").read_text())["onnxruntime"]["providers"]["aarch64"]
    unpack(download(ort_pin["url"], "ort.tgz", ort_pin["archive_sha256"]), WORK / "ort")
    wheel = download("https://files.pythonhosted.org/packages/27/ce/e8b941bcf0b213ca9cb4d8cb69b18398b4dad77923925d80bc3281837461/comma_deps_raylib-6.0.0.1.post103-py3-none-manylinux_2_28_x86_64.whl", "raylib.whl", "942ec942ec01005f05584eede1a6add483043e8e8f764e68cfebb7c21c443b89")
    with zipfile.ZipFile(wheel) as archive:
        for name in archive.namelist():
            if "/include/" in name and not name.endswith("/"):
                dest = WORK / "raylib/include" / name.split("/include/", 1)[1]
                dest.parent.mkdir(parents=True, exist_ok=True); dest.write_bytes(archive.read(name))
    wheel = download("https://github.com/commaai/dependencies/releases/download/libyuv/v1922.0/libyuv-1922.0-py3-none-linux_aarch64.whl", "libyuv.whl", "1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475")
    with zipfile.ZipFile(wheel) as archive:
        name = next(name for name in archive.namelist() if name.endswith("/libyuv.a"))
        dest = WORK / "libyuv/libyuv.a"; dest.parent.mkdir(exist_ok=True); dest.write_bytes(archive.read(name))
    ff = WORK / "ffmpeg"
    unpack(download("https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz", "ffmpeg.tar.xz", "8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968"), ff)
    src = ff / "ffmpeg-6.1.1"
    run([src / "configure", "--enable-cross-compile", "--arch=aarch64", "--target-os=linux", f"--cc={WORK / 'cc'}", "--disable-autodetect", "--disable-everything", "--disable-programs", "--disable-doc", "--disable-x86asm", "--enable-shared", "--disable-static", f"--prefix={ff / 'install'}"], cwd=src)
    run(["make", "install-headers"], cwd=src, stdout=subprocess.DEVNULL)
    (ff / "install/lib").mkdir(parents=True, exist_ok=True)
    for path in lib.iterdir():
        dest = ff / "install/lib" / path.name
        if dest.exists() or dest.is_symlink(): dest.unlink()
        dest.symlink_to(path.resolve())


def environment():
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="2", CARGO_PROFILE_RELEASE_DEBUG="0", LIBCLANG_PATH="/usr/lib/llvm-18/lib")
    for language, compiler in (("cc", "aarch64-linux-gnu-gcc"), ("cxx", "aarch64-linux-gnu-g++")):
        path = WORK / language
        args = [compiler, f"--sysroot={SDK}", f"-B{SDK}/usr/lib/aarch64-linux-gnu/", f"-L{SDK}/usr/lib/aarch64-linux-gnu", f"-Wl,-rpath-link,{SDK}/usr/lib/aarch64-linux-gnu", "-nostdinc", "-isystem", "/usr/lib/gcc-cross/aarch64-linux-gnu/13/include"]
        if language == "cxx":
            args += ["-isystem", "/usr/aarch64-linux-gnu/include/c++/13", "-isystem", "/usr/aarch64-linux-gnu/include/c++/13/aarch64-linux-gnu", "-isystem", "/usr/aarch64-linux-gnu/include/c++/13/backward"]
        args += ["-isystem", str(SDK / "usr/include/aarch64-linux-gnu"), "-isystem", str(SDK / "usr/include")]
        path.write_text("#!/bin/sh\nexec " + shlex.join(args) + ' "$@"\n'); path.chmod(0o755)
    env.update(CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=str(WORK / "cc"), CC_aarch64_unknown_linux_gnu=str(WORK / "cc"), CXX_aarch64_unknown_linux_gnu=str(WORK / "cxx"), AR_aarch64_unknown_linux_gnu="aarch64-linux-gnu-ar",
               BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu=f"--sysroot={SDK} -isystem {SDK}/usr/include/aarch64-linux-gnu -isystem {SDK}/usr/include", PKG_CONFIG_ALLOW_CROSS="1",
               OPENPILOT_OPENCV_ROOT=str(WORK / "opencv"), OPENPILOT_ORT_ROOT=str(WORK / "ort/onnxruntime-linux-aarch64-1.23.2"), STARTUP_UI_RAYLIB_ROOT=str(WORK / "raylib"), ENCODER_LIBYUV_LIB=str(WORK / "libyuv"), FFMPEG_DIR=str(WORK / "ffmpeg/install"))
    return env


def host_tools():
    room(6 * 2**30)
    assert not git("status", "--porcelain")
    env = environment()
    # Cross-build dependencies also compile host units. A separate target tree
    # prevents an upstream build script's cached ARM link paths leaking into
    # packaging tools when that script does not track FFMPEG_DIR changes.
    arm_providers = {"LOCATIOND_EIGEN_INCLUDE", "PARAMSD_EIGEN_INCLUDE", "FFMPEG_DIR", "ENCODER_LIBYUV_LIB", "OPENPILOT_OPENCV_ROOT", "OPENPILOT_ORT_ROOT", "STARTUP_UI_RAYLIB_ROOT"}
    host_env = {key: value for key, value in env.items() if key not in arm_providers}
    host_env["CARGO_TARGET_DIR"] = str(HOST_TARGET)
    run(["cargo", "build", "--locked", "--manifest-path", "rust/Cargo.toml", "--release", "-p", "openpilot-dev-tools", "--bin", "package-runtime-release", "--bin", "seal-runtime-package", "--bin", "runtime-elf-audit", "--bin", "publish-runtime-release", "-p", "openpilot-manager", "--bin", "openpilot-manager"], env=host_env)


def build():
    room(20 * 2**30)
    assert not git("status", "--porcelain")
    env = environment()
    providers()
    eigen = subprocess.check_output([sys.executable, "-c", "import eigen; print(eigen.INCLUDE_DIR)"], text=True).strip()
    env.update(LOCATIOND_EIGEN_INCLUDE=eigen, PARAMSD_EIGEN_INCLUDE=eigen)
    run([*BUILD, "--message-format=json"], env=env, stdout=(WORK / "build.jsonl").open("w"))
    installer_cmd = ["cargo", "build", "--manifest-path", "rust/Cargo.toml", "--locked", "--release", "--target", "aarch64-unknown-linux-musl", "-p", "openpilot-runtime-installer", "--bin", "openpilot-runtime-install"]
    installer_env = dict(env, CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="rust-lld", CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C target-feature=+crt-static")
    run(installer_cmd, env=installer_env)
    installer = TARGET / "aarch64-unknown-linux-musl/release/openpilot-runtime-install"
    save(WORK / "installer-build.json", dict(schema=1, source_commit=SHA, architecture="aarch64", target="aarch64-unknown-linux-musl", installer_sha256=digest(installer), build_command=shlex.join(installer_cmd), validation="Built on GitHub-hosted runner; complete offline QEMU installation and installed AGNOS launcher verification follow before publication."))
    room(6 * 2**30)
    rows = []
    for name in NAMES:
        source = installer if name == "openpilot-runtime-install" else TARGET / "aarch64-unknown-linux-gnu/release" / name
        header = source.open("rb").read(64)
        assert header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\xb7\x00", name
        dest = ROOT / "rust/bin" / name
        shutil.copy2(source, dest); dest.chmod(0o755)
        rows.append(dict(name=name, sha256=digest(dest), bytes=dest.stat().st_size, target="aarch64-unknown-linux-musl" if name == "openpilot-runtime-install" else "aarch64-unknown-linux-gnu"))
    build_dir = ROOT / "rust/bin/licenses/runtime-build"
    save(build_dir / "runtime-staging.json", dict(schema=1, source_commit=SHA, source_tree=git("rev-parse", "HEAD^{tree}"), source_clean=True, architecture="aarch64", agnos_version="19.8-carrot-bt1", runtime_build_command=BUILD, files=rows))
    shutil.copy2(WORK / "installer-build.json", build_dir / "installer-build.json")
    for name in ("openpilot-encoderd", "openpilot-camerad", "openpilot-loggerd", "openpilot-driving-modeld", "openpilot-manager", "openpilot-card", "openpilot-ui", "openpilot-pandad", "openpilot-proclogd-runtime"):
        assert SHA.encode() in (ROOT / "rust/bin" / name).read_bytes(), name
    roots = json.loads((CONFIG / "elf-roots.json").read_text())
    args = [TOOLS / "runtime-elf-audit"]
    for path in roots: args += ["--root", path]
    args += ["--library", "rust/bin/lib", "--library", SDK / "usr/lib/aarch64-linux-gnu", "--library", SDK / "lib", "--output", WORK / "elf.json"]
    run(args)
    audit = json.loads((WORK / "elf.json").read_text()); assert audit["compatible"]
    save(build_dir / "elf-summary.json", dict(compatible=True, objects=len(audit["objects"]), roots=len(audit["roots"]), scope="Actual AGNOS architecture, dependency, symbol/version closure in CI; no device execution."))
    run([TOOLS / "seal-runtime-package", "--root", ROOT, "--manager", TOOLS / "openpilot-manager", "--architecture", "aarch64"])
    run([TOOLS / "package-runtime-release", "--root", ROOT, "--installer", installer, "--installer-build-receipt", WORK / "installer-build.json", "--output", RELEASE])
    run([TOOLS / "package-runtime-release", "--root", ROOT, "--output", RELEASE, "--verify-only"])
    assert not git("status", "--porcelain")
    install(False)
    delivery = WORK / "delivery-tools"; delivery.mkdir(exist_ok=True)
    shutil.copy2(TOOLS / "publish-runtime-release", delivery / "publish-runtime-release")
    save(delivery / "identity.json", dict(source_commit=SHA, publisher_sha256=digest(delivery / "publish-runtime-release"), installer_sha256=digest(installer)))


def install(online):
    room(5 * 2**30)
    checkout = WORK / ("install-online" if online else "install-offline")
    run(["git", "clone", "--no-hardlinks", "--no-checkout", ROOT, checkout])
    run(["git", "-C", checkout, "checkout", "--detach", SHA])
    run(["git", "-C", checkout, "remote", "set-url", "origin", "https://github.com/bin9208/openpilot-rust.git"])
    descriptor = json.loads((RELEASE / "runtime-release.json").read_text())
    assert descriptor["source_commit"] == SHA
    helper = RELEASE / "openpilot-runtime-install-aarch64"; helper.chmod(0o755)
    args = ["qemu-aarch64-static", helper, "--root", checkout]
    if not online: args += ["--archive", RELEASE / descriptor["archive"]["name"], "--descriptor", RELEASE / "runtime-release.json"]
    run(args)
    run(["qemu-aarch64-static", "-L", SDK, "-E", f"LD_LIBRARY_PATH={checkout}/rust/bin/lib:{SDK}/usr/lib/aarch64-linux-gnu:{SDK}/lib", checkout / "rust/bin/openpilot-launch", "--root", checkout, "--verify-package"])
    save(WORK / ("online-install.json" if online else "offline-install.json"), dict(source_commit=SHA, archive_sha256=descriptor["archive"]["sha256"], manifest_sha256=digest(checkout / "rust/runtime-manifest.json"), installer_exit=0, installed_agnos_launcher_exit=0, device_execution=False))
    shutil.rmtree(checkout)


def restore_release():
    # The publication job starts in a fresh source checkout. Restore the sealed
    # payload before the existing publisher compares it with all six assets.
    descriptor = json.loads((RELEASE / "runtime-release.json").read_text())
    assert descriptor["source_commit"] == SHA
    archive = RELEASE / descriptor["archive"]["name"]
    assert archive.name == f"openpilot-rust-{SHA}-aarch64.tar.gz"
    assert digest(archive) == descriptor["archive"]["sha256"]
    source = (ROOT / "rust/crates/dev-tools/src/package.rs").read_text().split("const SOURCE_ASSETS:", 1)[1].split("];", 1)[0]
    assets = set(re.findall(r'"([^"\n]+)"', source))
    assert len(assets) == 19
    room(3 * 2**30)
    with tarfile.open(archive) as payload:
        for member in payload:
            assert member.name.startswith("rust/bin/") or member.name == "rust/runtime-manifest.json" or member.name in assets, member.name
            payload.extract(member, ROOT, filter="data")
    assert digest(ROOT / "rust/runtime-manifest.json") == descriptor["manifest_sha256"]
    run(["git", "add", "--renormalize", "--", *sorted(assets)])
    assert not git("status", "--porcelain")
    (RELEASE / "openpilot-runtime-install-aarch64").chmod(0o755)


def publish():
    assert os.environ["GITHUB_REF"] == "refs/heads/dev-md"
    identity = json.loads((WORK / "delivery-tools/identity.json").read_text())
    assert identity["source_commit"] == SHA
    publisher = WORK / "delivery-tools/publish-runtime-release"; publisher.chmod(0o755)
    assert digest(publisher) == identity["publisher_sha256"]
    assert digest(RELEASE / "openpilot-runtime-install-aarch64") == identity["installer_sha256"]
    restore_release()
    # The existing publisher independently checks every exact-SHA required gate
    # and all sealed asset identities. Wait outside it, once a minute, for CI.
    deadline = time.monotonic() + 3 * 3600
    while True:
        data = json.loads(subprocess.check_output(["gh", "api", f"repos/bin9208/openpilot-rust/actions/runs?head_sha={SHA}&event=push&branch=dev-md&per_page=100"]))
        paths = (".github/workflows/rust.yml", ".github/workflows/fast-checks.yml", ".github/workflows/integration.yml", ".github/workflows/user-docs.yaml")
        gates = {path: next((item for item in data["workflow_runs"] if item["path"] == path), None) for path in paths}
        failed = [path for path, item in gates.items() if item and item["status"] == "completed" and item["conclusion"] != "success"]
        assert not failed, f"required CI failed: {failed}"
        if all(item and item["status"] == "completed" for item in gates.values()): break
        assert time.monotonic() < deadline, "required CI still pending"
        time.sleep(60)
    run([publisher, "--root", ROOT, "--assets", RELEASE, "--branch", "dev-md"])
    install(True)


if __name__ == "__main__":
    os.environ["RUSTUP_TOOLCHAIN"] = "1.94.0"
    if sys.argv[1] == "prepare": stage_baseline(); sysroot(); environment()
    elif sys.argv[1] == "host-tools": host_tools()
    elif sys.argv[1] == "build": build()
    elif sys.argv[1] == "publish": sysroot(); publish()
    else: raise SystemExit("expected prepare, host-tools, build or publish")
