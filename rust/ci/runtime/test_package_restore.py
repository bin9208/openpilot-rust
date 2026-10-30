"""Focused hosted regressions; never initialize the delivery driver's globals."""
import ast
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tarfile
import tempfile
import unittest
from unittest import mock

from sealed_archive import extract_sealed_member


SOURCE = Path(__file__).with_name("package.py")
FUNCTIONS = {"restore_release", "install", "digest", "source_path_allowed"}
tree = ast.parse(SOURCE.read_text())
namespace = dict(Path=Path, hashlib=hashlib, json=json, os=os, re=re, tarfile=tarfile,
                 extract_sealed_member=extract_sealed_member)
exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node, ast.FunctionDef)
                             and node.name in FUNCTIONS], type_ignores=[]), str(SOURCE), "exec"), namespace)


def member(name, mode=0o644, kind=tarfile.REGTYPE, link=""):
    entry = tarfile.TarInfo(name)
    entry.mode = mode
    entry.type = kind
    entry.linkname = link
    entry.mtime = 1700000000
    entry.size = 7 if kind == tarfile.REGTYPE else 0
    return entry


def archive(entries):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as payload:
        for entry in entries:
            payload.addfile(entry, io.BytesIO(b"payload") if entry.isreg() else None)
    buffer.seek(0)
    return tarfile.open(fileobj=buffer)


class SealedExtractionTests(unittest.TestCase):
    def test_exact_regular_modes_survive_data_filter_and_umask(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            entries = [member(f"rust/bin/file-{mode:o}", mode)
                       for mode in (0o664, 0o444, 0o555, 0o644, 0o755)]
            previous = os.umask(0o077)
            try:
                with archive(entries) as payload:
                    for entry in payload:
                        extract_sealed_member(payload, entry, root)
                        path = root / entry.name
                        self.assertEqual(path.read_bytes(), b"payload")
                        self.assertEqual(stat.S_IMODE(path.stat().st_mode), entry.mode)
                        self.assertEqual(path.stat().st_mtime, 1700000000)
            finally:
                os.umask(previous)

    def test_existing_writable_git_file_receives_sealed_readonly_mode(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            target = root / "openpilot/source-asset"
            target.parent.mkdir()
            for mode in (0o444, 0o555, 0o664, 0o644, 0o755):
                with self.subTest(mode=mode):
                    # Fresh Git checkout assets are writable even when their
                    # sealed archive identity is read-only or group-writable.
                    if target.exists():
                        target.chmod(0o644)
                    target.write_bytes(b"longer stale Git checkout content")
                    target.chmod(0o644)
                    with archive([member("openpilot/source-asset", mode)]) as payload:
                        extract_sealed_member(payload, next(iter(payload)), root)
                    self.assertEqual(target.read_bytes(), b"payload")
                    self.assertEqual(stat.S_IMODE(target.stat().st_mode), mode)

    def test_relative_symlink_before_target_is_preserved_without_chmod(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            entries = [member("rust/bin/lib/alias", 0o664, tarfile.SYMTYPE, "target"),
                       member("rust/bin/lib/target", 0o444),
                       member("rust/bin/sub/alias", 0o755, tarfile.SYMTYPE, "../lib/target")]
            with archive(entries) as payload:
                for entry in payload:
                    extract_sealed_member(payload, entry, root)
            self.assertEqual(os.readlink(root / "rust/bin/lib/alias"), "target")
            self.assertEqual(os.readlink(root / "rust/bin/sub/alias"), "../lib/target")
            self.assertEqual((root / "rust/bin/sub/alias").read_bytes(), b"payload")
            self.assertEqual(stat.S_IMODE((root / "rust/bin/lib/target").stat().st_mode), 0o444)

    def test_noncanonical_paths_are_rejected_without_writes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "checkout"
            root.mkdir()
            for name in ("../escape", "/absolute", "rust/bin/../../../escape",
                         "rust/bin/../escape", "rust//bin/file", "./rust/bin/file",
                         "rust/bin/file/", "rust\\bin\\file"):
                with self.subTest(name=name), archive([member(name)]) as payload:
                    with self.assertRaises(tarfile.FilterError):
                        extract_sealed_member(payload, next(iter(payload)), root)
                self.assertEqual(list(root.iterdir()), [])

    def test_escaping_and_noncanonical_links_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for target in ("/absolute", "../../../escape", "a/../target", "a\\target", ""):
                entry = member("rust/bin/alias", 0o755, tarfile.SYMTYPE, target)
                with self.subTest(target=target), archive([entry]) as payload:
                    with self.assertRaises(tarfile.FilterError):
                        extract_sealed_member(payload, next(iter(payload)), root)
                self.assertEqual(list(root.iterdir()), [])

    def test_special_modes_and_noncanonical_types_are_rejected(self):
        entries = [member("rust/bin/file", mode, kind, "target")
                   for mode in (0o4644, 0o2644, 0o1644, 0o10644)
                   for kind in (tarfile.REGTYPE, tarfile.SYMTYPE)]
        entries += [member("rust/bin/file", kind=kind, link="target")
                    for kind in (tarfile.LNKTYPE, tarfile.DIRTYPE, tarfile.FIFOTYPE,
                                 tarfile.CHRTYPE, tarfile.BLKTYPE, tarfile.CONTTYPE, b"X")]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for entry in entries:
                # Direct TarInfo also covers types the tar parser may consume as metadata.
                with self.subTest(mode=entry.mode, kind=entry.type), archive([]) as payload:
                    with self.assertRaises(tarfile.FilterError):
                        extract_sealed_member(payload, entry, root)
                self.assertEqual(list(root.iterdir()), [])

    def test_preexisting_leaf_and_parent_symlinks_cannot_modify_targets(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "checkout"
            outside = Path(temp) / "outside"
            (root / "rust/bin").mkdir(parents=True)
            outside.mkdir()
            sentinel = outside / "sentinel"
            sentinel.write_bytes(b"unchanged")
            sentinel.chmod(0o644)
            (root / "rust/bin/leaf").symlink_to(sentinel)
            (root / "rust/bin/parent").symlink_to(outside, target_is_directory=True)
            (root / "rust/bin/internal").symlink_to(".", target_is_directory=True)
            for name in ("leaf", "parent/sentinel", "internal/leaf"):
                with self.subTest(name=name), archive([member("rust/bin/" + name, 0o755)]) as payload:
                    with self.assertRaises((tarfile.FilterError, OSError)):
                        extract_sealed_member(payload, next(iter(payload)), root)
                self.assertEqual(sentinel.read_bytes(), b"unchanged")
                self.assertEqual(stat.S_IMODE(sentinel.stat().st_mode), 0o644)

    def test_archive_symlink_cannot_redirect_later_extraction(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "rust/bin/real").mkdir(parents=True)
            sentinel = root / "rust/bin/real/sentinel"
            sentinel.write_bytes(b"unchanged")
            sentinel.chmod(0o644)
            entries = [member("rust/bin/alias", 0o755, tarfile.SYMTYPE, "real"),
                       member("rust/bin/alias/sentinel", 0o755)]
            with archive(entries) as payload:
                members = iter(payload)
                extract_sealed_member(payload, next(members), root)
                with self.assertRaises(OSError):
                    extract_sealed_member(payload, next(members), root)
            self.assertEqual(sentinel.read_bytes(), b"unchanged")
            self.assertEqual(stat.S_IMODE(sentinel.stat().st_mode), 0o644)

    def test_internal_leaf_symlink_is_never_written_or_chmodded(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "rust/bin").mkdir(parents=True)
            sentinel = root / "rust/bin/target"
            sentinel.write_bytes(b"unchanged")
            sentinel.chmod(0o444)
            (root / "rust/bin/alias").symlink_to("target")
            with archive([member("rust/bin/alias", 0o755)]) as payload:
                with self.assertRaises(OSError):
                    extract_sealed_member(payload, next(iter(payload)), root)
            self.assertEqual(sentinel.read_bytes(), b"unchanged")
            self.assertEqual(stat.S_IMODE(sentinel.stat().st_mode), 0o444)
            self.assertEqual(os.readlink(root / "rust/bin/alias"), "target")

    def test_existing_hardlink_cannot_modify_another_file(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "rust/bin").mkdir(parents=True)
            sentinel = root / "sentinel"
            sentinel.write_bytes(b"unchanged")
            sentinel.chmod(0o644)
            os.link(sentinel, root / "rust/bin/linked")
            with archive([member("rust/bin/linked", 0o755)]) as payload:
                with self.assertRaises(tarfile.FilterError):
                    extract_sealed_member(payload, next(iter(payload)), root)
            self.assertEqual(sentinel.read_bytes(), b"unchanged")
            self.assertEqual(stat.S_IMODE(sentinel.stat().st_mode), 0o644)


class RestorationIntegrationTests(unittest.TestCase):
    def test_restore_release_preserves_modes_and_keeps_descriptor_guards(self):
        for fault in (None, "source", "name", "archive", "manifest", "asset", "dirty"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temp:
                root = Path(temp) / "checkout"
                release = Path(temp) / "release"
                source = root / "rust/crates/dev-tools/src/package.rs"
                source.parent.mkdir(parents=True)
                release.mkdir()
                source.write_text((SOURCE.parents[2] / "crates/dev-tools/src/package.rs").read_text())
                sha = "a" * 40
                path = release / f"openpilot-rust-{sha}-aarch64.tar.gz"
                entries = [member("rust/bin/license", 0o664), member("rust/runtime-manifest.json")]
                if fault == "asset":
                    entries.append(member("unreviewed.txt"))
                with tarfile.open(path, "w:gz") as payload:
                    for entry in entries:
                        payload.addfile(entry, io.BytesIO(b"payload"))
                descriptor = dict(source_commit=sha, manifest_sha256=hashlib.sha256(b"payload").hexdigest(),
                                  archive=dict(name=path.name, sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
                if fault == "source": descriptor["source_commit"] = "b" * 40
                if fault == "name": descriptor["archive"]["name"] = "other.tar.gz"
                if fault == "archive": descriptor["archive"]["sha256"] = "0" * 64
                if fault == "manifest": descriptor["manifest_sha256"] = "0" * 64
                (release / "runtime-release.json").write_text(json.dumps(descriptor))
                (release / "openpilot-runtime-install-aarch64").write_bytes(b"helper")
                archive_digest = hashlib.sha256(path.read_bytes()).hexdigest()
                descriptor_bytes = (release / "runtime-release.json").read_bytes()
                with mock.patch.dict(namespace, ROOT=root, RELEASE=release, SHA=sha,
                                     room=mock.Mock(), run=mock.Mock(),
                                     git=lambda *args: " M tracked-source" if fault == "dirty" else ""):
                    if fault:
                        with self.assertRaises(AssertionError):
                            namespace["restore_release"]()
                    else:
                        namespace["restore_release"]()
                        self.assertEqual(stat.S_IMODE((root / "rust/bin/license").stat().st_mode), 0o664)
                self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), archive_digest)
                self.assertEqual((release / "runtime-release.json").read_bytes(), descriptor_bytes)

    def test_online_and_offline_children_run_from_the_installed_checkout(self):
        for online in (False, True):
            with self.subTest(online=online), tempfile.TemporaryDirectory() as temp:
                work = Path(temp)
                release = work / "release"
                release.mkdir()
                (release / "runtime-release.json").write_text(json.dumps(
                    dict(source_commit="exact-source", archive=dict(name="archive.tar.gz", sha256="sealed"))))
                (release / "openpilot-runtime-install-aarch64").write_bytes(b"helper")
                calls = []

                def run(args, **kwargs):
                    if args[0] == "qemu-aarch64-static":
                        # Boundary under test: child cwd, rather than run()'s producer default.
                        calls.append(kwargs.get("cwd"))

                with mock.patch.dict(namespace, WORK=work, ROOT=work / "producer", RELEASE=release,
                                     SDK=work / "sdk", SHA="exact-source", room=mock.Mock(),
                                     run=run, save=mock.Mock(), digest=lambda path: "manifest", shutil=mock.Mock()):
                    namespace["install"](online)
                checkout = work / ("install-online" if online else "install-offline")
                self.assertEqual(calls, [checkout, checkout])

    def test_only_reviewed_helper_and_regression_paths_are_admitted(self):
        allowed = namespace["source_path_allowed"]
        paths = ("rust/ci/runtime/sealed_archive.py", "rust/ci/runtime/test_package_restore.py",
                 "openpilot/selfdrive/modeld/jetlink/runtime.py",
                 "openpilot/selfdrive/modeld/jetlink/transition.py",
                 "openpilot/selfdrive/modeld/jetlink/validation.py",
                 "rust/crates/drivingd/examples/jetlink_raw_probe.rs", "rust/tools/check_jetlink_reference.py",
                 "docs/user/en/jetlink.md", "docs/user/ko/jetlink.md",
                 "docs/user/en/rust-port.md", "docs/user/ko/rust-port.md")
        for path in paths:
            with self.subTest(path=path):
                self.assertTrue(allowed(path))
                self.assertFalse(allowed(path + ".extra"))
        for path in ("rust/ci/runtime/unrelated.py", "openpilot/selfdrive/modeld/jetlink/unreviewed.py",
                     "openpilot/selfdrive/modeld/jetlink/cinque-v3.json",
                     "rust/crates/dev-tools/src/ffmpeg_build.rs", "third_party/ffmpeg/configure"):
            with self.subTest(path=path):
                self.assertFalse(allowed(path))


if __name__ == "__main__":
    unittest.main()
