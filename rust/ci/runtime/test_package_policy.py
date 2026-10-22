"""Focused cloud tests; AST extraction never initializes the delivery driver."""
import ast
import copy
import hashlib
import json
from pathlib import Path
import tempfile
from unittest import mock
import os
import shutil
import unittest


SOURCE = Path(__file__).with_name("package.py")
FUNCTIONS = {"runner_environment_allowed", "source_path_allowed", "rebuild_opencv_artifact", "require_fresh_opencv", "require_delivery_ref", "stage_opencv_libraries", "digest"}
tree = ast.parse(SOURCE.read_text())
namespace = dict(Path=Path, hashlib=hashlib, json=json, os=os, shutil=shutil, SHA="exact-source", BASE="baseline")
exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node, ast.FunctionDef)
                             and node.name in FUNCTIONS], type_ignores=[]), str(SOURCE), "exec"), namespace)


class SourcePolicyTests(unittest.TestCase):
    def test_only_hosted_or_named_registered_runner_is_admitted(self):
        allowed = namespace["runner_environment_allowed"]
        for environment, name, expected in (
            ("github-hosted", "GitHub Actions", True),
            ("self-hosted", "bin-actions", True),
            ("self-hosted", "other-runner", False),
            ("self-hosted", "bin-actions-extra", False),
            ("self-hosted", None, False),
            (None, "bin-actions", False),
            ("", "", False),
        ):
            with self.subTest(environment=environment, name=name):
                self.assertEqual(allowed(environment, name), expected)

    def test_cpu_copy_reduction_exceptions_are_exact_paths(self):
        allowed = namespace["source_path_allowed"]
        for path in (
            "rust/crates/loggerd/src/daemon.rs",
            "rust/crates/loggerd/src/encoder.rs",
            "rust/crates/loggerd/src/encoder_tests.rs",
            "rust/crates/signal-observer/src/process.rs",
            "rust/crates/opencv-runtime/src/native/image.rs",
            "rust/crates/opencv-runtime/native/image.cc",
            "rust/crates/opencv-runtime/tests/preprocessing.rs",
            "rust/tools/check_xiaoge_native_memory.py",
            "docs/naver/dev-md-cpu-headroom-422.md",
        ):
            with self.subTest(path=path):
                self.assertTrue(allowed(path))
                self.assertFalse(allowed(path + ".extra"))
                self.assertFalse(allowed(path + "/child"))

    def test_gpu_exceptions_are_exact_paths(self):
        allowed = namespace["source_path_allowed"]
        for path in (
            "rust/crates/dev-tools/src/opencv_build.rs",
            "rust/crates/opencv-runtime/src/bridge.rs",
            "rust/crates/opencv-runtime/build.rs",
            "rust/crates/opencv-runtime/PROVENANCE.md",
            "rust/crates/opencv-runtime/native/dnn_context.cc",
            "rust/crates/opencv-runtime/native/dnn_context.h",
            "rust/crates/opencv-runtime/native/dnn_context_device.h",
            "rust/crates/opencv-runtime/native/opencl_priority.h",
            "rust/crates/opencv-runtime/src/native/net/tests.rs",
            "rust/crates/opencv-runtime/tests/fixtures/opencl_priority.cc",
            "rust/crates/opencv-runtime/tests/fixtures/dnn_context_device.cc",
            ".github/workflows/rust.yml",
            "rust/crates/opencv-runtime/native/net.cc",
            "rust/crates/opencv-runtime/native/bridge.h",
            "rust/crates/opencv-runtime/src/native/net.rs",
            "rust/crates/opencv-runtime/tests/native.rs",
            "rust/crates/opencv-runtime/tests/dnn_target.rs",
            "rust/crates/opencv-runtime/tests/fixtures/opencl_probe.c",
            "docs/naver/dev-md-xiaoge-gpu.md",
            "openpilot/common/params_keys.h",
            "openpilot/selfdrive/carrot/xiaoge/v_asm_web.html",
            "docs/user/en/lane-change.md", "docs/user/ko/lane-change.md",
            "docs/user/en/settings.md", "docs/user/ko/settings.md",
            "rust/ci/runtime/test_package_policy.py",
        ):
            with self.subTest(path=path):
                self.assertTrue(allowed(path))
                self.assertFalse(allowed(path + ".extra"))

    def test_models_other_providers_and_directories_remain_rejected(self):
        for path in (
            "rust/crates/opencv-runtime/", "rust/crates/opencv-runtime/native/image.cc.extra",
            "rust/crates/loggerd/src/other.rs", "rust/crates/signal-observer/src/neural.rs",
            "rust/crates/opencv-runtime/native/opencl_priority_other.h",
            "rust/crates/opencv-runtime/src/native/net/other.rs",
            "rust/crates/opencv-runtime/tests/fixtures/other.c",
            "rust/crates/opencv-runtime/native/opencv-defined-access.patch",
            "rust/crates/dev-tools/src/ffmpeg_build.rs", "rust/crates/webcam/build.rs",
            "rust/crates/signal-observer/src/lib.rs", "rust/ci/runtime/unrelated.py",
            "openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx",
            "openpilot/selfdrive/carrot/xiaoge/other.html", "openpilot/common/params_keys.h/child",
        ):
            with self.subTest(path=path):
                self.assertFalse(namespace["source_path_allowed"](path))

    def test_only_opencv_companions_are_removed_from_baseline_reuse(self):
        replaced = namespace["rebuild_opencv_artifact"]
        for name in ("core", "imgproc", "dnn", "videoio", "imgcodecs"):
            for suffix in ("so", "so.413", "so.4.13.0"):
                self.assertTrue(replaced(f"rust/bin/lib/libopencv_{name}.{suffix}"))
        self.assertTrue(replaced("rust/bin/licenses/opencv/provenance.json"))
        self.assertTrue(replaced("rust/bin/licenses/opencv/OpenCV-Apache-2.0.txt"))
        for path in ("rust/bin/lib/libavcodec.so.60", "rust/bin/lib/libonnxruntime.so",
                     "rust/bin/licenses/ffmpeg/provenance.json", "rust/bin/lib/not-opencv.so"):
            self.assertFalse(replaced(path))


class LibraryStagingTests(unittest.TestCase):
    def test_sorted_aliases_stage_into_an_empty_package_without_baseline_targets(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "checkout"
            provider = Path(temp) / "provider"
            install = provider / "install/lib"
            install.mkdir(parents=True)
            destination = root / "rust/bin/lib"
            destination.mkdir(parents=True)
            companion = destination / "libavcodec.so.60"
            companion.write_bytes(b"unchanged companion")
            header = bytearray(20)
            header[:6] = b"\x7fELF\x02\x01"
            header[18:20] = b"\xb7\x00"
            for name in ("core", "dnn", "imgproc", "imgcodecs", "videoio"):
                stem = f"libopencv_{name}.so"
                (install / f"{stem}.4.13.0").write_bytes(header + name.encode())
                (install / f"{stem}.413").symlink_to(f"{stem}.4.13.0")
                (install / stem).symlink_to(f"{stem}.413")
            entries = sorted(install.glob("libopencv_*.so*"))
            self.assertEqual(entries[0].name, "libopencv_core.so")
            self.assertTrue(entries[0].is_symlink())
            self.assertFalse(any(destination.glob("libopencv_*")), "no baseline OpenCV target may exist")
            digest = namespace["digest"]
            sealed = {f"install/lib/{path.name}": digest(path) for path in entries}
            receipt = dict(sealed=sealed, libraries={path: value for path, value in sealed.items()
                                                    if path.endswith(".so.4.13.0")})

            def digest_after_all_entries_exist(path):
                # A digest must never depend on a baseline target or on glob
                # ordering: all real files and aliases must already be staged.
                self.assertTrue(all((destination / entry.name).is_file() for entry in entries))
                return digest(path)

            symlink_to = Path.symlink_to

            def alias_after_all_real_files_exist(path, target, *args, **kwargs):
                self.assertTrue(all((destination / Path(relative).name).is_file()
                                    for relative in receipt["libraries"]))
                return symlink_to(path, target, *args, **kwargs)

            with mock.patch.dict(namespace, digest=digest_after_all_entries_exist), \
                 mock.patch.object(Path, "symlink_to", alias_after_all_real_files_exist):
                rows = namespace["stage_opencv_libraries"](provider, receipt, root)
            self.assertEqual(len(rows), 15)
            for entry in entries:
                staged = destination / entry.name
                self.assertEqual(digest(staged), sealed[f"install/lib/{entry.name}"])
                if entry.is_symlink():
                    self.assertTrue(staged.is_symlink())
                    self.assertEqual(staged.readlink(), Path(entry.resolve().name))
            self.assertEqual(companion.read_bytes(), b"unchanged companion")


class FreshProviderTests(unittest.TestCase):
    def fixture(self, root):
        receipt = dict(status="BUILT", architecture="aarch64", source_unchanged=True,
                       opencl=True, source_commit="exact-source", fresh_build=True,
                       source_revision="fe38fc608f6acb8b68953438a62305d8318f4fcd",
                       source_archive_sha256="6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61",
                       hardening_patch_sha256="7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec")
        files = {
            "source-files.json": "{}\n",
            "aarch64.cmake": "# synthetic fixture\n",
            "opencv-defined-access.patch": (SOURCE.parents[2] / "crates/opencv-runtime/native/opencv-defined-access.patch").read_text(),
            "configure.log": "-- OpenCL: YES (no extra features)\n",
            "configure.command.json": json.dumps(["cmake", "-DWITH_OPENCL=ON", "-DOPENCL_LIBRARY:STRING=", "-DENABLE_CCACHE=OFF"]),
            "build/CMakeCache.txt": "OPENCL_LIBRARY:STRING=\n",
            "build/cvconfig.h": "#define HAVE_OPENCL\n",
            "opencl-capability.json": json.dumps(dict(enabled=True, cmake_opencl="YES", linkage="dynamic", target_icd_packaged=False)),
            "licenses/OpenCL-Headers-LICENSE.txt": "synthetic notice\n",
        }
        for phase in ("configure", "build", "install"):
            files[f"{phase}.exit.json"] = '{"code":0}\n'
        for phase in ("build", "install"):
            files[f"{phase}.command.json"] = json.dumps(["cmake", "--" + phase, "/fresh"])
        identity = dict(revision=receipt["source_revision"], sha256=receipt["source_archive_sha256"],
                        external_hardening_patch_sha256=receipt["hardening_patch_sha256"],
                        native_source_files_sha256=hashlib.sha256(files["source-files.json"].encode()).hexdigest())
        files["source.json"] = json.dumps(identity)
        receipt["source_files_sha256"] = identity["native_source_files_sha256"]
        for name in ("core", "imgproc", "dnn", "videoio", "imgcodecs"):
            files[f"install/lib/libopencv_{name}.so.4.13.0"] = f"new {name} fixture\n"
            for suffix in ("so", "so.413"):
                files[f"install/lib/libopencv_{name}.{suffix}"] = f"new {name} fixture\n"
            files[f"install/include/opencv4/opencv2/{name}.hpp"] = f"{name} synthetic header\n"
        for relative, content in files.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        receipt["sealed"] = {path: namespace["digest"](root / path) for path in files}
        receipt["libraries"] = {path: value for path, value in receipt["sealed"].items() if path.endswith(".so.4.13.0")}
        (root / "native-libraries.sha256").write_text("".join(f"{value}  {path}\n" for path, value in receipt["sealed"].items()))
        baseline = {"rust/bin/lib/" + Path(path).name: "baseline-cpu" for path in receipt["libraries"]}
        return receipt, baseline

    def test_fresh_sealed_provider_is_accepted(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            receipt, baseline = self.fixture(root)
            namespace["require_fresh_opencv"](root, receipt, baseline)

    def test_baseline_or_unsealed_cpu_provider_cannot_be_relabelled(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            receipt, baseline = self.fixture(root)
            for name in ("core", "dnn"):
                reused = dict(baseline)
                path = f"libopencv_{name}.so.4.13.0"
                reused[f"rust/bin/lib/{path}"] = receipt["libraries"][f"install/lib/{path}"]
                with self.subTest(name=name), self.assertRaisesRegex(AssertionError, "baseline CPU"):
                    namespace["require_fresh_opencv"](root, receipt, reused)
            for field, value in (("opencl", False), ("source_commit", "baseline"),
                                 ("fresh_build", False), ("source_files_sha256", "other-source"), ("sealed", {})):
                invalid = copy.deepcopy(receipt)
                invalid[field] = value
                with self.subTest(field=field), self.assertRaises(AssertionError):
                    namespace["require_fresh_opencv"](root, invalid, baseline)
            for path, content in (("configure.log", "-- OpenCL: NO\n"),
                                  ("build/cvconfig.h", "#define HAVE_OPENCL\n#define HAVE_OPENCL_STATIC\n"),
                                  ("build/CMakeCache.txt", "OPENCL_LIBRARY:STRING=/other/libOpenCL.so\n")):
                original = (root / path).read_text()
                (root / path).write_text(content)
                invalid = copy.deepcopy(receipt)
                invalid["sealed"][path] = namespace["digest"](root / path)
                (root / "native-libraries.sha256").write_text("".join(f"{value}  {name}\n" for name, value in invalid["sealed"].items()))
                with self.subTest(path=path), self.assertRaises(AssertionError):
                    namespace["require_fresh_opencv"](root, invalid, baseline)
                (root / path).write_text(original)
            (root / "configure.log").write_text("-- OpenCL: NO\n")
            with self.assertRaises(AssertionError):
                namespace["require_fresh_opencv"](root, receipt, baseline)

    def test_delivery_driver_always_rebuilds_opencv_before_consumers_and_sealing(self):
        build = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "build")
        calls = [node.func.id for node in ast.walk(build) if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)]
        self.assertIn("providers", calls)
        providers = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "providers")
        self.assertTrue(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                            and node.func.id == "build_opencv" for node in ast.walk(providers)))
        source = ast.get_source_segment(SOURCE.read_text(), build)
        self.assertLess(source.index("providers()"), source.index('[*BUILD,'))
        self.assertLess(source.index('[*BUILD,'), source.index('"seal-runtime-package"'))
        self.assertLess(source.index("smoke_opencv(env)"), source.index('"seal-runtime-package"'))

    def test_agnos_smoke_uses_staged_provider_and_no_platform_probe(self):
        smoke = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "smoke_opencv")
        source = ast.get_source_segment(SOURCE.read_text(), smoke)
        for expected in ("qemu-aarch64-static", "opencl_probe.c", "opencl_priority.cc",
                         "cpu_default_and_explicit_cpu_preserve_math_and_owned_outputs",
                         "failed_forward_invalidates_execution_evidence", "policy_child",
                         "OPENCV_POLICY_CHILD", "OPENCV_OPENCL_RUNTIME", "OPENCV_POLICY_MARKER",
                         "root", "receipt.json"):
            self.assertIn(expected, source)


class DeliveryRefTests(unittest.TestCase):
    def test_only_verified_dispatch_of_marked_dev_md_merge_is_allowed(self):
        for event, ref, message, parents, supplied, accepted in (
            ("workflow_dispatch", "dev-md", "merge [runtime-package]", "one two", "exact-source", True),
            ("workflow_dispatch", "codex/feat-453-xiaoge-gpu", "[runtime-package]", "one two", "exact-source", False),
            ("workflow_dispatch", "dev-md", "merge", "one two", "exact-source", False),
            ("workflow_dispatch", "dev-md", "[runtime-package]", "one", "exact-source", False),
            ("workflow_dispatch", "dev-md", "[runtime-package]", "one two", "other-source", False),
            ("push", "dev-md", "[runtime-package]", "one two", "exact-source", False),
            ("pull_request", "dev-md", "[runtime-package]", "one two", "exact-source", False),
            ("workflow_dispatch", "dev", "[runtime-package]", "one two", "exact-source", False),
        ):
            def git(*args):
                return parents if "--format=%P" in args else message
            with self.subTest(event=event, ref=ref, message=message, parents=parents), \
                 mock.patch.dict(os.environ, GITHUB_EVENT_NAME=event, GITHUB_REF="refs/heads/" + ref,
                                 RUNTIME_SOURCE_SHA=supplied), \
                 mock.patch.dict(namespace, git=git):
                if accepted:
                    namespace["require_delivery_ref"]()
                else:
                    with self.assertRaises(AssertionError):
                        namespace["require_delivery_ref"]()


if __name__ == "__main__":
    unittest.main()
