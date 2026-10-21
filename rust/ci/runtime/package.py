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
assert os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted", "runtime delivery requires a GitHub-hosted runner"
ROOT = Path(os.environ["GITHUB_WORKSPACE"])
WORK = Path(os.environ["RUNNER_TEMP"]) / "agnos-runtime"
WORK.mkdir(exist_ok=True)
CONFIG = ROOT / "rust/ci/runtime"
SHA = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
assert SHA == os.environ["GITHUB_SHA"], "checkout must match the event source"
BASE = "3496c4bbf300e7487d17b2bf25afc170584dfe02"
INHERITED = "a2b06522a24f4a3533192daf99ac48f119782bda"
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


def source_path_allowed(path, source_entry="", inherited_entry=""):
    allowed = (
        "AGENTS.md", "launch_openpilot.sh", "rust/Cargo.lock",
        "rust/crates/jetlink/", "rust/crates/runtime-core/", "rust/crates/runtime-launcher/",
        "rust/crates/xiaoge/", "rust/crates/dev-tools/src/publish.rs",
        "rust/ci/runtime/package.py", "rust/ci/runtime/build-args.json",
        "rust/ci/runtime/elf-roots.json", "rust/ci/runtime/opencv.sha256",
        "rust/ci/runtime/opencv-headers.cmake", "rust/ci/runtime/test_package_policy.py",
        # Issue #453: exact GPU producer/API files only. Their native provider
        # is rebuilt below; these exceptions never authorize CPU binary reuse.
        "rust/crates/dev-tools/src/opencv_build.rs",
        "rust/crates/opencv-runtime/build.rs", "rust/crates/opencv-runtime/PROVENANCE.md",
        "rust/crates/opencv-runtime/src/bridge.rs", "rust/crates/opencv-runtime/src/lib.rs",
        "rust/crates/opencv-runtime/src/native/mod.rs", "rust/crates/opencv-runtime/src/native/net.rs",
        "rust/crates/opencv-runtime/native/bridge.h", "rust/crates/opencv-runtime/native/net.cc",
        "rust/crates/opencv-runtime/native/dnn_context.cc", "rust/crates/opencv-runtime/native/dnn_context.h",
        "rust/crates/opencv-runtime/native/dnn_context_device.h",
        "rust/crates/opencv-runtime/native/opencl_priority.h",
        "rust/crates/opencv-runtime/src/native/net/tests.rs",
        "rust/crates/opencv-runtime/tests/native.rs", "rust/crates/opencv-runtime/tests/contracts.rs",
        "rust/crates/opencv-runtime/tests/dnn_target.rs",
        "rust/crates/opencv-runtime/tests/fixtures/opencl_probe.c",
        "rust/crates/opencv-runtime/tests/fixtures/opencl_priority.cc",
        "rust/crates/opencv-runtime/tests/fixtures/dnn_context_device.cc",
        "docs/naver/dev-md-xiaoge-gpu.md", "openpilot/common/params_keys.h",
        "openpilot/selfdrive/carrot/xiaoge/v_asm_web.html",
        "rust/upstream/jetlink.json", "docs/naver/dev-md-jetlink-",
        "docs/naver/dev-md-lfs-checkout-validation.md", "docs/naver/dev-md-onnx-load-validation.md",
        "docs/user/en/lane-change.md", "docs/user/ko/lane-change.md",
        "docs/user/en/settings.md", "docs/user/ko/settings.md", ".lfsconfig",
        ".github/workflows/jetlink-native.yml", ".github/workflows/dev-md-runtime.yml",
        ".github/workflows/fast-checks.yml", "tools/check_jetlink_usb_policy.py",
        ".github/workflows/rust.yml",
        "tools/ci/tests/test_lfs_endpoint.py",
    )
    if any(path == prefix or prefix.endswith(("/", "-")) and path.startswith(prefix) for prefix in allowed):
        return True
    # Historical runner documentation is permitted only with its original mode,
    # type and blob. It does not authorize new CI or provider-source changes.
    return path == "docs/naver/self-hosted-runner-404.md" and bool(source_entry) and source_entry == inherited_entry


def check_source_policy():
    assert not git("status", "--porcelain")
    # Check both sides of moves so an allowed destination cannot hide removal
    # of a native provider's source.
    changed = git("diff", "--no-renames", "--name-only", BASE, SHA).splitlines()
    inherited_path = "docs/naver/self-hosted-runner-404.md"
    source_entry = git("ls-tree", SHA, "--", inherited_path)
    inherited_entry = git("ls-tree", INHERITED, "--", inherited_path)
    rejected = [path for path in changed if not source_path_allowed(path, source_entry, inherited_entry)]
    save(WORK / "source-policy.json", dict(schema=1, source_commit=SHA, baseline_source=BASE,
         inherited_source=INHERITED, inherited_path=inherited_path, source_entry=source_entry,
         inherited_entry=inherited_entry, changed_source_paths=changed, rejected_source_paths=rejected))
    assert not rejected, f"baseline provider reuse rejects source changes: {rejected}"
    return changed


def rebuild_opencv_artifact(path):
    return path.startswith("rust/bin/lib/libopencv_") or path.startswith("rust/bin/licenses/opencv/")


def stage_baseline():
    changed = check_source_policy()
    archive = download(f"https://github.com/bin9208/openpilot-rust/releases/download/runtime-{BASE}/openpilot-rust-{BASE}-aarch64.tar.gz", "baseline.tar.gz", "eb67c8c58f8c8da62aa7afdc11d7cfc1262038222b6e4b9ff0a33fe64206297d")
    base = WORK / "baseline"
    room(4 * 2**30)
    unpack(archive, base)
    manifest_file = base / "rust/runtime-manifest.json"
    assert digest(manifest_file) == "041be84b1e7ea8d6611e4256e4ae30814eca1cdef498dffa6d455c0d1962787e"
    manifest = json.loads(manifest_file.read_text())
    save(WORK / "opencv-baseline.json", {row["path"]: row["sha256"] for row in manifest["files"]
                                       if rebuild_opencv_artifact(row["path"])})
    replaced = {f"rust/bin/{name}" for name in NAMES} | {f"rust/bin/licenses/runtime-build/{name}.json" for name in ("runtime-staging", "installer-build", "source-build-metadata", "provider-reuse", "elf-summary")}
    reused = []
    for row in manifest["files"]:
        path = row["path"]
        source = base / path
        assert digest(source) == row["sha256"], path
        if path in replaced or rebuild_opencv_artifact(path):
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
         rebuilt_providers=["opencv"],
         note="Non-OpenCV companions/models/firmware/source assets retain verified baseline identity. OpenCV is excluded and freshly rebuilt with OpenCL from pinned hardened source against genuine AGNOS; all project Rust executables are rebuilt on the exact clean source. Other external receipts retain original identity."))
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
    # Restore headers for unchanged providers; OpenCV is always freshly built.
    lib = ROOT / "rust/bin/lib"
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
    # CMake/pkg-config sees only this target prefix, never host FFmpeg.
    pc = ff / "install/lib/pkgconfig"; pc.mkdir(exist_ok=True)
    for name, version in (("avcodec", "60.31.102"), ("avformat", "60.16.100"),
                          ("avutil", "58.29.100"), ("swscale", "7.5.100")):
        (pc / f"lib{name}.pc").write_text(
            f"prefix={ff / 'install'}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n"
            f"Name: lib{name}\nDescription: sealed AGNOS baseline FFmpeg {name}\nVersion: {version}\n"
            f"Libs: -L${{libdir}} -l{name}\nCflags: -I${{includedir}}\n")
    build_opencv()


def require_fresh_opencv(provider, receipt, baseline):
    assert receipt["status"] == "BUILT" and receipt["architecture"] == "aarch64"
    assert receipt["opencl"] is True and receipt["fresh_build"] is True
    assert receipt["source_commit"] == SHA and receipt["source_unchanged"] is True
    assert receipt["source_revision"] == "fe38fc608f6acb8b68953438a62305d8318f4fcd"
    assert receipt["source_archive_sha256"] == "6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61"
    assert receipt["hardening_patch_sha256"] == "7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec"
    libraries = {f"install/lib/libopencv_{name}.so.4.13.0" for name in
                 ("core", "dnn", "imgproc", "imgcodecs", "videoio")}
    aliases = {path.replace(".so.4.13.0", f".{suffix}") for path in libraries for suffix in ("so", "so.413")}
    headers = {f"install/include/opencv4/opencv2/{name}.hpp" for name in ("core", "dnn", "imgproc", "imgcodecs", "videoio")}
    assert set(receipt["libraries"]) == libraries
    sealed = receipt["sealed"]
    required = libraries | aliases | headers | {"source.json", "source-files.json", "aarch64.cmake", "opencv-defined-access.patch",
                           "opencl-capability.json", "configure.log", "configure.command.json", "configure.exit.json",
                           "build/CMakeCache.txt", "build/cvconfig.h", "build.command.json", "build.exit.json",
                           "install.command.json", "install.exit.json", "licenses/OpenCL-Headers-LICENSE.txt"}
    assert required <= sealed.keys(), "fresh OpenCL build evidence is incomplete"
    for relative, expected in sealed.items():
        path = Path(relative)
        assert not path.is_absolute() and ".." not in path.parts
        assert (provider / path).resolve().is_relative_to(provider.resolve())
        assert digest(provider / path) == expected, relative
    assert receipt["libraries"] == {path: sealed[path] for path in libraries}
    listed = dict(line.split("  ", 1)[::-1] for line in (provider / "native-libraries.sha256").read_text().splitlines())
    assert listed == sealed, "build receipt and sealed native manifest differ"
    capability = json.loads((provider / "opencl-capability.json").read_text())
    assert capability["enabled"] is True and capability["cmake_opencl"] == "YES"
    assert capability["linkage"] == "dynamic" and capability["target_icd_packaged"] is False
    assert "OPENCL_LIBRARY:STRING=\n" in (provider / "build/CMakeCache.txt").read_text()
    assert any(line.split("OpenCL:", 1)[-1].split()[:1] == ["YES"]
               for line in (provider / "configure.log").read_text().splitlines() if "OpenCL:" in line)
    assert any(line.split() in (["#define", "HAVE_OPENCL"], ["#define", "HAVE_OPENCL", "1"])
               for line in (provider / "build/cvconfig.h").read_text().splitlines())
    command = json.loads((provider / "configure.command.json").read_text())
    assert "-DWITH_OPENCL=ON" in command and "-DOPENCL_LIBRARY:STRING=" in command
    assert "-DENABLE_CCACHE=OFF" in command
    assert not any(line.split()[:2] == ["#define", "HAVE_OPENCL_STATIC"]
                   for line in (provider / "build/cvconfig.h").read_text().splitlines())
    for phase in ("configure", "build", "install"):
        assert json.loads((provider / f"{phase}.exit.json").read_text())["code"] == 0
    identity = json.loads((provider / "source.json").read_text())
    assert identity["native_source_files_sha256"] == sealed["source-files.json"] == receipt["source_files_sha256"]
    assert identity["revision"] == receipt["source_revision"]
    assert identity["sha256"] == receipt["source_archive_sha256"]
    assert identity["external_hardening_patch_sha256"] == receipt["hardening_patch_sha256"]
    assert sealed["opencv-defined-access.patch"] == receipt["hardening_patch_sha256"]
    # Core and DNN must change when OpenCL is compiled in. Companion modules
    # can legitimately reproduce identical bytes after a fresh build.
    for name in ("core", "dnn"):
        path = f"libopencv_{name}.so.4.13.0"
        assert receipt["libraries"][f"install/lib/{path}"] != baseline[f"rust/bin/lib/{path}"], "baseline CPU OpenCV reuse forbidden"


def stage_opencv_libraries(provider, receipt, root):
    lib = root / "rust/bin/lib"
    lib.mkdir(parents=True, exist_ok=True)
    for old in lib.glob("libopencv_*"):
        old.unlink()
    entries = sorted((provider / "install/lib").glob("libopencv_*.so*"))
    assert len(entries) == 15, "five libraries and both aliases are mandatory"
    aliases = []
    # Lexical order places .so aliases before their versioned objects. Copy
    # every real library first, without dereferencing any destination alias.
    for path in entries:
        assert path.resolve().parent == provider / "install/lib"
        assert f"install/lib/{path.resolve().name}" in receipt["libraries"]
        dest = lib / path.name
        if path.is_symlink():
            aliases.append((dest, path.resolve().name))
        else:
            header = path.open("rb").read(20)
            assert header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\xb7\x00"
            shutil.copy2(path, dest)
    for dest, target in aliases:
        dest.symlink_to(target)
    staged = []
    for path in entries:
        dest = lib / path.name
        actual = digest(dest)
        assert actual == receipt["sealed"][f"install/lib/{path.name}"], "staged OpenCV content changed"
        staged.append(dict(path=str(dest.relative_to(root)), sha256=actual))
    return staged


def build_opencv():
    room(6 * 2**30)
    provider = WORK / "opencv"
    # Never cache or restore the OpenCV native build/install tree.
    assert not provider.exists(), "OpenCV must be rebuilt from scratch for this exact source"
    prepared = WORK / "opencv-source"
    run([TOOLS / "build-xiaoge-opencv", "--root", ROOT, "--output", prepared, "--prepare-only"])
    source = prepared / "opencv-fe38fc608f6acb8b68953438a62305d8318f4fcd"
    cross = WORK / "opencv-cross"; cross.mkdir()
    for language in ("cc", "cxx"):
        shutil.copy2(WORK / language, cross / f"{language}.sh")
    run([TOOLS / "build-runtime-opencv", "--root", ROOT, "--source", source, "--output", provider,
         "--opencl", "--ffmpeg", WORK / "ffmpeg/install", "--sdk", SDK, "--cross", cross,
         "--compiler-root", "/usr/aarch64-linux-gnu"], env=environment())
    receipt = json.loads((provider / "receipt.json").read_text())
    baseline = json.loads((WORK / "opencv-baseline.json").read_text())
    require_fresh_opencv(provider, receipt, baseline)
    room(256 * 2**20)
    # Every alias and header comes from this install, never synthesized CPU
    # headers or links to baseline libraries. Keep the full sealed evidence.
    licenses = ROOT / "rust/bin/licenses/opencv"
    if licenses.exists(): shutil.rmtree(licenses)
    licenses.mkdir(parents=True)
    staged = stage_opencv_libraries(provider, receipt, ROOT)
    for relative in receipt["sealed"]:
        path = provider / relative
        dest = licenses / "provider" / relative
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dest)
    for path in (provider / "licenses").iterdir():
        shutil.copy2(path, licenses / path.name)
    for name in ("receipt.json", "source.json", "native-libraries.sha256", "opencv-defined-access.patch"):
        shutil.copy2(provider / name, licenses / name)
    # Validate exactly the bytes staged into the sealed runtime evidence tree.
    run(["sha256sum", "--check", licenses / "native-libraries.sha256"], cwd=licenses / "provider", stdout=subprocess.DEVNULL)
    provenance = dict(schema=1, source_commit=SHA, baseline_source=BASE, rebuilt=True, reused=False,
                      architecture="aarch64", opencl=True, agnos_version="19.8-carrot-bt1",
                      provider_receipt_sha256=digest(provider / "receipt.json"),
                      libraries=receipt["libraries"], files=staged,
                      scope="Fresh combined five-library pinned hardened source build, dynamic system OpenCL. Consumer AGNOS ELF closure and package installation are checked before sealing/publication. GPU execution remains user device acceptance.")
    save(licenses / "provenance.json", provenance)
    reuse_file = ROOT / "rust/bin/licenses/runtime-build/provider-reuse.json"
    reuse = json.loads(reuse_file.read_text())
    assert not any(rebuild_opencv_artifact(row["path"]) for row in reuse["artifacts"])
    reuse["opencv"] = provenance
    save(reuse_file, reuse)
    metadata_file = reuse_file.with_name("source-build-metadata.json")
    metadata = json.loads(metadata_file.read_text())
    metadata["opencv"] = provenance
    save(metadata_file, metadata)


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
               OPENPILOT_OPENCV_ROOT=str(WORK / "opencv"), OPENPILOT_WEBCAM_OPENCV_ROOT=str(WORK / "opencv"), OPENPILOT_ORT_ROOT=str(WORK / "ort/onnxruntime-linux-aarch64-1.23.2"), STARTUP_UI_RAYLIB_ROOT=str(WORK / "raylib"), ENCODER_LIBYUV_LIB=str(WORK / "libyuv"), FFMPEG_DIR=str(WORK / "ffmpeg/install"))
    return env


def host_tools():
    room(6 * 2**30)
    assert not git("status", "--porcelain")
    env = environment()
    # Cross-build dependencies also compile host units. A separate target tree
    # prevents an upstream build script's cached ARM link paths leaking into
    # packaging tools when that script does not track FFMPEG_DIR changes.
    arm_providers = {"LOCATIOND_EIGEN_INCLUDE", "PARAMSD_EIGEN_INCLUDE", "FFMPEG_DIR", "ENCODER_LIBYUV_LIB", "OPENPILOT_OPENCV_ROOT", "OPENPILOT_WEBCAM_OPENCV_ROOT", "OPENPILOT_ORT_ROOT", "STARTUP_UI_RAYLIB_ROOT"}
    host_env = {key: value for key, value in env.items() if key not in arm_providers}
    host_env["CARGO_TARGET_DIR"] = str(HOST_TARGET)
    run(["cargo", "test", "--locked", "--manifest-path", "rust/Cargo.toml", "--release", "-p", "openpilot-dev-tools", "--lib", "opencv_build::tests"], env=host_env)
    room(6 * 2**30)
    run(["cargo", "build", "--locked", "--manifest-path", "rust/Cargo.toml", "--release", "-p", "openpilot-dev-tools", "--bin", "package-runtime-release", "--bin", "seal-runtime-package", "--bin", "runtime-elf-audit", "--bin", "publish-runtime-release", "--bin", "build-xiaoge-opencv", "--bin", "build-runtime-opencv", "-p", "openpilot-manager", "--bin", "openpilot-manager"], env=host_env)


def smoke_opencv(env):
    room(2 * 2**30)
    evidence = WORK / "opencv-smoke"; evidence.mkdir()
    command = ["cargo", "test", "--locked", "--manifest-path", "rust/Cargo.toml", "--release",
               "--target", "aarch64-unknown-linux-gnu", "-p", "openpilot-opencv-runtime",
               "--features", "native-skip-miri", "--test", "dnn_target", "--no-run", "--message-format=json"]
    with (evidence / "build.jsonl").open("w") as log:
        run(command, env=env, stdout=log)
    artifacts = [json.loads(line) for line in (evidence / "build.jsonl").read_text().splitlines()]
    executables = [row["executable"] for row in artifacts if row.get("reason") == "compiler-artifact"
                   and row["target"]["name"] == "dnn_target" and row.get("executable")]
    assert len(executables) == 1
    binary = Path(executables[0])
    root = ROOT / "rust/crates/opencv-runtime"
    probe = evidence / "libOpenCL-no-platform.so"
    run([WORK / "cc", "-shared", "-fPIC", root / "tests/fixtures/opencl_probe.c", "-o", probe])
    priority = evidence / "priority-contracts"
    run([WORK / "cxx", "-std=c++17", "-Wall", "-Wextra", "-Werror", "-I", root / "native",
         root / "tests/fixtures/opencl_priority.cc", "-o", priority])
    context_device = evidence / "context-device-contracts"
    run([WORK / "cxx", "-std=c++17", "-Wall", "-Wextra", "-Werror", "-I", root / "native",
         root / "tests/fixtures/dnn_context_device.cc", "-o", context_device])
    # Execute the exact staged five-library provider with genuine AGNOS loader
    # and C runtime. The fixture exposes no devices and never loads Qualcomm.
    qemu = ["qemu-aarch64-static", "-L", SDK, "-E",
            f"LD_LIBRARY_PATH={ROOT}/rust/bin/lib:{SDK}/usr/lib/aarch64-linux-gnu:{SDK}/lib"]
    tests = []
    for name in ("cpu_default_and_explicit_cpu_preserve_math_and_owned_outputs",
                 "failed_forward_invalidates_execution_evidence"):
        with (evidence / f"{name}.log").open("w") as log:
            run([*qemu, "-E", f"OPENCV_OPENCL_RUNTIME={probe}", binary, "--exact", name, "--nocapture"],
                stdout=log, stderr=subprocess.STDOUT, timeout=180)
        assert "1 passed; 0 failed;" in (evidence / f"{name}.log").read_text(), "smoke test was filtered or failed"
        tests.append(name)
    for mode in ("cpu", "gpu"):
        marker = evidence / f"{mode}-loader-marker"
        with (evidence / f"{mode}-policy.log").open("w") as log:
            run([*qemu, "-E", f"OPENCV_POLICY_CHILD={mode}", "-E", f"OPENCV_OPENCL_RUNTIME={probe}",
                 "-E", f"OPENCV_POLICY_MARKER={marker}", binary, "--exact", "policy_child", "--nocapture"],
                stdout=log, stderr=subprocess.STDOUT, timeout=180)
        assert "1 passed; 0 failed;" in (evidence / f"{mode}-policy.log").read_text()
        assert marker.exists() == (mode == "gpu"), "CPU must not initialize OpenCL; GPU rejection must exercise the loader"
        tests.append(f"{mode}-policy")
    with (evidence / "priority-contracts.log").open("w") as log:
        run([*qemu, priority], stdout=log, stderr=subprocess.STDOUT, timeout=180)
    tests.append("priority-contracts")
    with (evidence / "context-device-contracts.log").open("w") as log:
        run([*qemu, context_device], stdout=log, stderr=subprocess.STDOUT, timeout=180)
    tests.append("context-device-contracts")
    require_fresh_opencv(WORK / "opencv", json.loads((WORK / "opencv/receipt.json").read_text()),
                         json.loads((WORK / "opencv-baseline.json").read_text()))
    result = dict(schema=1, source_commit=SHA, architecture="aarch64", agnos_version="19.8-carrot-bt1",
                  provider_receipt_sha256=digest(WORK / "opencv/receipt.json"),
                  tests=tests, test_binary_sha256=digest(binary), probe_sha256=digest(probe),
                  evidence={path.name: digest(path) for path in evidence.glob("*.log")},
                  device_execution=False, actual_gpu_execution=False,
                  scope="AGNOS QEMU CPU inference/recovery, no-OpenCL initialization on CPU, GPU no-platform rejection, priority ownership/verification and context device-wrapper identity boundaries. No device or GPU performance claim.")
    save(evidence / "result.json", result)
    save(ROOT / "rust/bin/licenses/runtime-build/opencv-smoke.json", result)


def build():
    room(20 * 2**30)
    assert not git("status", "--porcelain")
    env = environment()
    providers()
    assert {"openpilot-xiaoge", "openpilot-webcam", "openpilot-signalcolord"} <= set(NAMES)
    # Recompile every selected project package, including the shared Params
    # registry. Unchanged third-party Cargo dependencies retain their cache.
    project_packages = sorted({BUILD[i + 1] for i, flag in enumerate(BUILD) if flag == "-p"}
                              | {"openpilot-params", "openpilot-opencv-runtime", "openpilot-logging"})
    clean = ["cargo", "clean", "--manifest-path", "rust/Cargo.toml", "--release", "--target", "aarch64-unknown-linux-gnu"]
    for package in project_packages: clean += ["-p", package]
    run(clean, env=env)
    room(20 * 2**30)
    eigen = subprocess.check_output([sys.executable, "-c", "import eigen; print(eigen.INCLUDE_DIR)"], text=True).strip()
    env.update(LOCATIOND_EIGEN_INCLUDE=eigen, PARAMSD_EIGEN_INCLUDE=eigen)
    run([*BUILD, "--message-format=json"], env=env, stdout=(WORK / "build.jsonl").open("w"))
    installer_cmd = ["cargo", "build", "--manifest-path", "rust/Cargo.toml", "--locked", "--release", "--target", "aarch64-unknown-linux-musl", "-p", "openpilot-runtime-installer", "--bin", "openpilot-runtime-install"]
    installer_env = dict(env, CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="rust-lld", CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C target-feature=+crt-static")
    room(3 * 2**30)
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
    smoke_opencv(env)
    save(build_dir / "runtime-staging.json", dict(schema=1, source_commit=SHA, source_tree=git("rev-parse", "HEAD^{tree}"), source_clean=True, architecture="aarch64", agnos_version="19.8-carrot-bt1", runtime_build_command=BUILD, files=rows,
         opencv_provider_receipt_sha256=digest(WORK / "opencv/receipt.json"), rebuilt_opencv_consumers=["openpilot-xiaoge", "openpilot-webcam", "openpilot-signalcolord"],
         rebuilt_project_packages=project_packages, params_keys_sha256=digest(ROOT / "openpilot/common/params_keys.h")))
    shutil.copy2(WORK / "installer-build.json", build_dir / "installer-build.json")
    for name in ("openpilot-encoderd", "openpilot-camerad", "openpilot-loggerd", "openpilot-driving-modeld", "openpilot-manager", "openpilot-card", "openpilot-ui", "openpilot-pandad", "openpilot-proclogd-runtime"):
        assert SHA.encode() in (ROOT / "rust/bin" / name).read_bytes(), name
    roots = json.loads((CONFIG / "elf-roots.json").read_text())
    roots.extend(f"rust/bin/{name}" for name in NAMES if f"rust/bin/{name}" not in roots)
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
    assert os.environ["GITHUB_EVENT_NAME"] == "push"
    require_delivery_ref()
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


def require_delivery_ref():
    event = os.environ["GITHUB_EVENT_NAME"]
    ref = os.environ["GITHUB_REF"]
    if event == "workflow_dispatch":
        assert ref == "refs/heads/codex/feat-453-xiaoge-gpu", "manual preflight is limited to issue #453"
    else:
        assert event == "push" and ref == "refs/heads/dev-md"
        assert "[runtime-package]" in git("show", "-s", "--format=%B", SHA)
        assert len(git("show", "-s", "--format=%P", SHA).split()) == 2, "runtime publication requires an exact marked merge"


if __name__ == "__main__":
    os.environ["RUSTUP_TOOLCHAIN"] = "1.94.0"
    require_delivery_ref()
    if sys.argv[1] == "source-policy": check_source_policy()
    elif sys.argv[1] == "prepare": stage_baseline(); sysroot(); environment()
    elif sys.argv[1] == "host-tools": host_tools()
    elif sys.argv[1] == "build": build()
    elif sys.argv[1] == "publish": sysroot(); publish()
    else: raise SystemExit("expected source-policy, prepare, host-tools, build or publish")
