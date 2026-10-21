#!/usr/bin/env python3
"""Hosted-only recovery of the unchanged sealed 1f00 runtime, never a rebuild."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile


REPO = "bin9208/openpilot-rust"
SHA = "1f00f0b7c4ccab4c23c5845aef490406058aec6c"
BUILD_RUN = 39600241810
PACKAGE_JOB = 119222652122
ARTIFACT = 12348580983
ARTIFACT_NAME = f"dev-md-runtime-{SHA}"
ZIP_SHA = "932a6cf134c1ac86d275f4d475cf4fa6d3ce600e8866a79db19750c415f6c48c"
ARCHIVE_SHA = "12a7d2cb5d5b4cccca9cf016ceb28da51aad52171ff55fbbcbd5a8367dc12197"
MANIFEST_SHA = "ca720e8fcf093edb189f89ff76414c38cfb4df5df5f863f231ebe64f80ab8d05"
ARCHIVE = f"openpilot-rust-{SHA}-aarch64.tar.gz"
INSTALLER = "openpilot-runtime-install-aarch64"
AGNOS = "19.8-carrot-bt1"
GATES = {
    "rust.yml": (39600241890, ("rust checks", "rust aarch64 build")),
    "fast-checks.yml": (39600241868, ("fast checks",)),
    "integration.yml": (39600242209, ("integration gate",)),
    "user-docs.yaml": (39600242020, ("check mapped user docs",)),
}


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def run(args, **kwargs):
    print("+", " ".join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), check=True, **kwargs)


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def room(work, growth):
    free = shutil.disk_usage(work).free
    assert free >= 25 * 2**30 + growth, f"disk reserve: free={free}, growth={growth}"


def api(endpoint, paginate=False):
    args = ["gh", "api", f"repos/{REPO}/{endpoint}"]
    if paginate:
        args += ["--paginate", "--slurp"]
    return json.loads(subprocess.check_output(args))


def success(value):
    assert value["status"] == "completed" and value["conclusion"] == "success", value.get("html_url")


def source_run(value, run_id, workflow):
    assert value["id"] == run_id and value["head_sha"] == SHA
    assert value["head_branch"] == "dev-md" and value["event"] == "push"
    assert value["repository"]["full_name"] == REPO
    assert value["path"] == f".github/workflows/{workflow}"


def verify_origin(work):
    build = api(f"actions/runs/{BUILD_RUN}")
    source_run(build, BUILD_RUN, "dev-md-runtime.yml")
    # Its publication job failed; require the successful package job, not a
    # fabricated success conclusion for the complete original workflow.
    assert build["status"] == "completed"
    job = api(f"actions/jobs/{PACKAGE_JOB}")
    assert job["run_id"] == BUILD_RUN and job["id"] == PACKAGE_JOB and job["head_sha"] == SHA
    assert job["name"] == "Build and seal actual AGNOS runtime"
    success(job)
    artifact = api(f"actions/artifacts/{ARTIFACT}")
    assert artifact["id"] == ARTIFACT and artifact["name"] == ARTIFACT_NAME and not artifact["expired"]
    assert artifact["digest"] == f"sha256:{ZIP_SHA}"
    origin = artifact["workflow_run"]
    assert origin["id"] == BUILD_RUN and origin["head_sha"] == SHA and origin["head_branch"] == "dev-md"
    assert origin["repository_id"] == build["repository"]["id"]
    assert origin["head_repository_id"] == build["head_repository"]["id"]
    gates = []
    for workflow, (run_id, names) in GATES.items():
        evidence = api(f"actions/runs/{run_id}")
        source_run(evidence, run_id, workflow)
        success(evidence)
        jobs = [item for page in api(f"actions/runs/{run_id}/jobs?filter=latest&per_page=100", True)
                for item in page["jobs"]]
        for name in names:
            selected = [item for item in jobs if item["name"] == name]
            assert len(selected) == 1 and selected[0]["head_sha"] == SHA, name
            success(selected[0])
            gates.append(dict(name=name, run_id=run_id, job_id=selected[0]["id"], url=selected[0]["html_url"]))
    save(work / "origin-recovery.json", dict(source_commit=SHA, build_run=BUILD_RUN, package_job=PACKAGE_JOB,
         artifact=artifact, original_gates=gates))
    return artifact


def relative(name):
    path = PurePosixPath(name)
    assert name and "\\" not in name and "\0" not in name and not path.is_absolute(), name
    assert str(path) == name and all(part not in (".", "..") for part in path.parts), name
    return Path(*path.parts)


def contained(root, name):
    path = root / relative(name)
    assert path.resolve().is_relative_to(root.resolve()), name
    for parent in path.parents:
        if parent == root:
            break
        assert not parent.is_symlink(), f"directory alias: {parent}"
    return path


def regular_mode(path, mode):
    # Apply permissions through a no-follow descriptor, never a symlink target.
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), "rb") as file:
        assert stat.S_ISREG(os.fstat(file.fileno()).st_mode)
        os.fchmod(file.fileno(), mode)


def verify_download(work, artifact):
    archive = work / "original-artifact.zip"
    room(work, artifact["size_in_bytes"] + 64 * 2**20)
    if not archive.exists():
        with archive.open("xb") as output:
            run(["gh", "api", f"repos/{REPO}/actions/artifacts/{ARTIFACT}/zip"], stdout=output)
    assert archive.stat().st_size == artifact["size_in_bytes"] and digest(archive) == ZIP_SHA
    expected = {f"release/{name}" for name in (
        ARCHIVE, f"{ARCHIVE}.sha256", "runtime-release.json", INSTALLER,
        f"{INSTALLER}.sha256", "runtime-installer-build.json")}
    expected |= {"delivery-tools/identity.json", "delivery-tools/publish-runtime-release", "offline-install.json"}
    seen = set()
    with zipfile.ZipFile(archive) as original:
        for entry in original.infolist():
            if entry.is_dir():
                assert entry.filename in ("release/", "delivery-tools/")
                continue
            assert entry.filename in expected and entry.filename not in seen, entry.filename
            assert stat.S_IFMT(entry.external_attr >> 16) in (0, stat.S_IFREG), "linked/special ZIP entry"
            path = contained(work, entry.filename)
            assert path.is_file() and not path.is_symlink() and path.stat().st_size == entry.file_size
            with original.open(entry) as source:
                assert hashlib.file_digest(source, "sha256").hexdigest() == digest(path), entry.filename
            seen.add(entry.filename)
    assert seen == expected, "original artifact file set differs"


def restore(archive_path, root, assets, timestamp):
    corrections, members, seen = [], [], set()
    with tarfile.open(archive_path, "r:gz") as archive:
        for member in archive:
            path = contained(root, member.name)
            assert member.name in assets or member.name == "rust/runtime-manifest.json" or member.name.startswith("rust/bin/"), member.name
            assert member.name not in seen, "duplicate runtime member"
            seen.add(member.name)
            assert member.isfile() or member.issym(), "unsupported runtime member type"
            assert 0 <= member.mode <= 0o777, "special permission bits in runtime archive"
            assert member.uid == member.gid == 0 and not member.uname and not member.gname
            assert member.mtime == timestamp, "runtime archive timestamp differs from source"
            if member.issym():
                assert member.linkname and not PurePosixPath(member.linkname).is_absolute()
                assert "\\" not in member.linkname and "\0" not in member.linkname and member.size == 0
                assert (path.parent / member.linkname).resolve().is_relative_to(root.resolve()), "escaping runtime alias"
            else:
                assert not path.is_symlink() and not path.is_dir(), "regular member would follow a link/directory"
            filtered = tarfile.data_filter(member, str(root))
            assert filtered is not None
            archive.extract(filtered, root, filter="data")
            if member.isfile():
                assert stat.S_ISREG(path.lstat().st_mode)
                actual = stat.S_IMODE(path.lstat().st_mode)
                if actual != member.mode:
                    regular_mode(path, member.mode)
                    row = dict(path=member.name, filtered_mode=oct(actual), restored_mode=oct(member.mode))
                    corrections.append(row)
                    print("mode correction:", json.dumps(row), flush=True)
            members.append((member.name, member.mode, member.issym()))
    for name, mode, linked in members:
        path = contained(root, name)
        assert path.is_file() and path.is_symlink() == linked
        assert stat.S_IMODE(path.stat().st_mode) == mode, f"restored mode differs: {name}"
    return dict(source_commit=SHA, members=len(members), mode_corrections=corrections)


def pristine(root):
    assert git(root, "rev-parse", "HEAD") == SHA
    assert git(root, "remote", "get-url", "origin") in (f"https://github.com/{REPO}", f"https://github.com/{REPO}.git")
    assert not git(root, "status", "--porcelain", "--untracked-files=normal"), "runtime source must be clean"


def original_publisher(args, receipt, driver_sha):
    result = subprocess.run(list(map(str, args)), capture_output=True, text=True)
    try:
        body = json.loads(result.stdout) if result.returncode == 0 else None
    except json.JSONDecodeError:
        body = None
    save(receipt, dict(source_commit=SHA, driver_commit=driver_sha, publisher_sha256=digest(args[0]),
         exit=result.returncode, result=body, stdout=result.stdout[:65536], stderr=result.stderr[:65536]))
    print(result.stdout, end="")
    print(result.stderr, end="", file=sys.stderr)
    result.check_returncode()
    assert isinstance(body, dict) and body["source_commit"] == SHA
    return body


def recover(root, work, action):
    driver = Path(__file__).resolve()
    assert not driver.is_relative_to(root) and not work.is_relative_to(root)
    driver_root = driver.parents[3]
    driver_sha = git(driver_root, "rev-parse", "HEAD")
    assert driver_sha == os.environ["GITHUB_SHA"] and driver_sha != SHA
    assert not git(driver_root, "status", "--porcelain"), "recovery driver checkout must be clean"
    pristine(root)
    save(work / "driver-recovery.json", dict(driver_commit=driver_sha, driver_sha256=digest(driver),
         runtime_source_commit=SHA, action=action, recovery_run=os.environ["GITHUB_RUN_ID"],
         recovery_attempt=os.environ["GITHUB_RUN_ATTEMPT"], original_build_run=BUILD_RUN,
         original_package_job=PACKAGE_JOB, original_artifact=ARTIFACT, original_artifact_zip_sha256=ZIP_SHA))
    verify_download(work, verify_origin(work))
    release = work / "release"
    descriptor = json.loads((release / "runtime-release.json").read_text())
    assert descriptor["source_commit"] == SHA and descriptor["architecture"] == "aarch64" and descriptor["agnos_version"] == AGNOS
    assert descriptor["archive"]["name"] == ARCHIVE and descriptor["archive"]["sha256"] == ARCHIVE_SHA
    assert descriptor["manifest_sha256"] == MANIFEST_SHA
    archive = release / ARCHIVE
    assert digest(archive) == ARCHIVE_SHA and archive.stat().st_size == descriptor["archive"]["bytes"]
    identity = json.loads((work / "delivery-tools/identity.json").read_text())
    publisher = work / "delivery-tools/publish-runtime-release"
    helper = release / INSTALLER
    assert identity["source_commit"] == SHA and digest(publisher) == identity["publisher_sha256"]
    assert digest(helper) == identity["installer_sha256"]
    offline = json.loads((work / "offline-install.json").read_text())
    assert offline["source_commit"] == SHA and offline["archive_sha256"] == ARCHIVE_SHA and offline["manifest_sha256"] == MANIFEST_SHA
    assert offline["installer_exit"] == offline["installed_agnos_launcher_exit"] == 0 and offline["device_execution"] is False
    sdk = work / "sdk"
    assert sdk.is_dir() and not sdk.is_symlink() and (sdk / "VERSION").read_text().strip() == AGNOS
    loader = sdk / "lib/ld-linux-aarch64.so.1"
    assert loader.resolve().is_relative_to(sdk.resolve())
    with loader.open("rb") as source:
        header = source.read(20)
    assert header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\xb7\x00"
    source = (root / "rust/crates/dev-tools/src/package.rs").read_text().split("const SOURCE_ASSETS:", 1)[1].split("];", 1)[0]
    assets = set(re.findall(r'"([^"\n]+)"', source))
    assert len(assets) == 19
    room(work, descriptor["archive"]["unpacked_bytes"] + 256 * 2**20)
    result = restore(archive, root, assets, int(git(root, "show", "-s", "--format=%ct", SHA)))
    save(work / "mode-restore.json", result)
    assert digest(root / "rust/runtime-manifest.json") == MANIFEST_SHA and digest(archive) == ARCHIVE_SHA
    run(["git", "-C", root, "add", "--renormalize", "--", *sorted(assets)])
    pristine(root)
    regular_mode(publisher, 0o755)
    regular_mode(helper, 0o755)
    args = [publisher, "--root", root, "--assets", release, "--branch", "dev-md"]
    checked = original_publisher([*args, "--check-only"], work / "publisher-check-recovery.json", driver_sha)
    assert checked["check_only"] is True
    assert digest(publisher) == identity["publisher_sha256"] and digest(helper) == identity["installer_sha256"]
    if action == "check":
        return
    room(work, descriptor["archive"]["bytes"] + 256 * 2**20)
    published = original_publisher(args, work / "publisher-recovery.json", driver_sha)
    assert published["download_verified"] is True
    assert digest(publisher) == identity["publisher_sha256"]
    assert digest(archive) == ARCHIVE_SHA and digest(helper) == identity["installer_sha256"]
    room(work, descriptor["archive"]["unpacked_bytes"] + 512 * 2**20)
    checkout = work / "install-online-recovery"
    assert not checkout.exists()
    run(["git", "clone", "--no-hardlinks", "--no-checkout", root, checkout])
    run(["git", "-C", checkout, "checkout", "--detach", SHA])
    run(["git", "-C", checkout, "remote", "set-url", "origin", f"https://github.com/{REPO}.git"])
    pristine(checkout)
    # Default public lookup only: no offline archive or descriptor override.
    run(["qemu-aarch64-static", helper, "--root", checkout])
    run(["qemu-aarch64-static", "-L", sdk, "-E",
         f"LD_LIBRARY_PATH={checkout}/rust/bin/lib:{sdk}/usr/lib/aarch64-linux-gnu:{sdk}/lib",
         checkout / "rust/bin/openpilot-launch", "--root", checkout, "--verify-package"])
    assert digest(checkout / "rust/runtime-manifest.json") == MANIFEST_SHA
    assert git(checkout, "rev-parse", "HEAD") == SHA
    assert not git(checkout, "status", "--porcelain", "--untracked-files=no"), "installed tracked source changed"
    # The installer atomically exposes its generation through this symlink;
    # /bin/ ignores directories but Git still reports the installed symlink.
    untracked = git(checkout, "ls-files", "--others", "--exclude-standard").splitlines()
    assert set(untracked) <= {"rust/bin"}, untracked
    active_bin = checkout / "rust/bin"
    assert active_bin.is_symlink() and active_bin.readlink() == Path(".runtime-active/rust/bin")
    assert active_bin.resolve().is_relative_to((checkout / "rust/.runtime-packages").resolve())
    pristine(root)
    save(work / "online-install.json", dict(source_commit=SHA, archive_sha256=ARCHIVE_SHA, manifest_sha256=MANIFEST_SHA,
         installer_exit=0, installed_agnos_launcher_exit=0, device_execution=False, recovery_driver_commit=driver_sha))


class RestoreTests(unittest.TestCase):
    def archive(self, folder, members):
        path = folder / "payload.tar.gz"
        with tarfile.open(path, "w:gz") as archive:
            for member, data in members:
                archive.addfile(member, io.BytesIO(data) if member.isfile() else None)
        return path

    def test_exact_regular_modes_including_confirmed_group_write_and_relative_alias(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            root = folder / "checkout"; root.mkdir()
            members = []
            for mode in (0o444, 0o555, 0o644, 0o755, 0o664):
                member = tarfile.TarInfo(f"rust/bin/mode-{mode:o}")
                member.mode, member.size = mode, 1
                members.append((member, b"x"))
            alias = tarfile.TarInfo("rust/bin/alias")
            alias.type, alias.mode, alias.linkname = tarfile.SYMTYPE, 0o664, "mode-664"
            members.insert(0, (alias, b""))
            result = restore(self.archive(folder, members), root, set(), 0)
            for mode in (0o444, 0o555, 0o644, 0o755, 0o664):
                self.assertEqual(stat.S_IMODE((root / f"rust/bin/mode-{mode:o}").stat().st_mode), mode)
            self.assertTrue((root / "rust/bin/alias").is_symlink())
            self.assertEqual((root / "rust/bin/alias").readlink(), Path("mode-664"))
            self.assertIn(dict(path="rust/bin/mode-664", filtered_mode="0o644", restored_mode="0o664"), result["mode_corrections"])

    def test_reject_escapes_special_modes_and_types(self):
        for name, mode, kind, link in (
            ("../escape", 0o644, tarfile.REGTYPE, ""),
            ("/absolute", 0o644, tarfile.REGTYPE, ""),
            ("rust/bin/../../escape", 0o644, tarfile.REGTYPE, ""),
            ("rust/bin/file", 0o4644, tarfile.REGTYPE, ""),
            ("rust/bin/file", 0o2644, tarfile.REGTYPE, ""),
            ("rust/bin/file", 0o1644, tarfile.REGTYPE, ""),
            ("rust/bin/file", 0o644, tarfile.LNKTYPE, "rust/bin/target"),
            ("rust/bin/file", 0o644, tarfile.FIFOTYPE, ""),
            ("rust/bin/file", 0o644, tarfile.CHRTYPE, ""),
            ("rust/bin/file", 0o644, tarfile.BLKTYPE, ""),
            ("rust/bin/file", 0o755, tarfile.DIRTYPE, ""),
            ("rust/bin/link", 0o777, tarfile.SYMTYPE, "../../../escape"),
            ("rust/bin/link", 0o777, tarfile.SYMTYPE, "/etc/passwd"),
        ):
            with self.subTest(name=name, mode=mode, kind=kind, link=link), tempfile.TemporaryDirectory() as temp:
                folder = Path(temp)
                root = folder / "checkout"; root.mkdir()
                member = tarfile.TarInfo(name)
                member.mode, member.type, member.linkname = mode, kind, link
                with self.assertRaises((AssertionError, tarfile.FilterError)):
                    restore(self.archive(folder, [(member, b"")]), root, set(), 0)

    def test_existing_symlink_or_directory_alias_is_not_followed(self):
        for directory in (False, True):
            with self.subTest(directory=directory), tempfile.TemporaryDirectory() as temp:
                folder = Path(temp)
                root = folder / "checkout"; (root / "rust/bin").mkdir(parents=True)
                outside = folder / "outside"; outside.mkdir()
                target = outside / "target"; target.write_bytes(b"preserve")
                original_mode = stat.S_IMODE(target.stat().st_mode)
                (root / "rust/bin/link").symlink_to(outside if directory else target)
                member = tarfile.TarInfo("rust/bin/link/target" if directory else "rust/bin/link")
                member.mode, member.size = 0o664, 1
                with self.assertRaises(AssertionError):
                    restore(self.archive(folder, [(member, b"x")]), root, set(), 0)
                self.assertEqual(target.read_bytes(), b"preserve")
                self.assertEqual(stat.S_IMODE(target.stat().st_mode), original_mode)


def main():
    assert os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted", "execute only in GitHub-hosted Actions"
    assert os.environ.get("GITHUB_REPOSITORY") == REPO
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--action", choices=("check", "publish"), default="check")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(RestoreTests))
        raise SystemExit(not result.wasSuccessful())
    assert args.root and args.work, "--root and --work required"
    assert os.environ["GITHUB_EVENT_NAME"] == "workflow_dispatch"
    assert os.environ["GITHUB_REF"] == "refs/heads/codex/feat-453-xiaoge-gpu"
    assert os.environ.get("GIT_LFS_SKIP_SMUDGE") == "1"
    recover(args.root.resolve(strict=True), args.work.resolve(strict=True), args.action)


if __name__ == "__main__":
    main()
