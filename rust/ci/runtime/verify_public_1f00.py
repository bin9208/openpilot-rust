#!/usr/bin/env python3
"""Verify the already published immutable 1f00 release; never publish or replace."""
import argparse
import json
import os
from pathlib import Path
import re
import zipfile

import recover_1f00 as recovery


def verify(root, work):
    work.mkdir(exist_ok=True)
    recovery.pristine(root)
    recovery.verify_origin(work)
    driver = recovery.git(Path(__file__).resolve().parents[3], "rev-parse", "HEAD")
    assert driver == os.environ["GITHUB_SHA"] and driver != recovery.SHA
    artifact = recovery.api("actions/artifacts/12353138633")
    assert artifact["workflow_run"]["id"] == 39617674785
    assert artifact["workflow_run"]["head_sha"] == "4c4e092ef2aa5c38eddeac086396faf7fa2bd9d1"
    assert artifact["digest"] == "sha256:cbfb29c592767421afc4262efc0427887e985a080758140c3ed3961baf300081"
    proof_zip = work / "publication-proof.zip"
    with proof_zip.open("xb") as output:
        recovery.run(["gh", "api", f"repos/{recovery.REPO}/actions/artifacts/12353138633/zip"], stdout=output)
    assert recovery.digest(proof_zip) == artifact["digest"].removeprefix("sha256:")
    with zipfile.ZipFile(proof_zip) as archive:
        proof = json.loads(archive.read("publisher-recovery.json"))
    assert proof["exit"] == 0 and proof["source_commit"] == recovery.SHA
    assert proof["publisher_sha256"] == "76b6f06fc080806352d45b5a9ddf110dd93bf213a3e0ebf73e1d05df9c1fc3c4"
    published = proof["result"]
    assert published["download_verified"] is True and published["source_commit"] == recovery.SHA
    release = recovery.api(f"releases/tags/runtime-{recovery.SHA}")
    assert release["id"] == 419667279 and release["target_commitish"] == recovery.SHA
    assert release["prerelease"] and not release["draft"]
    tag = recovery.api(f"git/ref/tags/runtime-{recovery.SHA}")
    assert tag["object"]["type"] == "commit" and tag["object"]["sha"] == recovery.SHA
    assert len(release["assets"]) == len(published["assets"]) == 6
    assert {asset["name"] for asset in release["assets"]} == set(published["assets"])
    for asset in release["assets"]:
        expected = published["assets"][asset["name"]]
        assert asset["size"] == expected["bytes"] and asset["digest"] == "sha256:" + expected["sha256"]
    recovery.save(work / "publication-proof-recovery.json", dict(runtime_source_commit=recovery.SHA,
        verification_driver_commit=driver, original_publication_run=39617674785,
        original_publisher_result=published, public_release=release))
    recovery.room(work, 2 * 2**30)
    for name in (recovery.INSTALLER, "runtime-release.json"):
        url = f"https://github.com/{recovery.REPO}/releases/download/runtime-{recovery.SHA}/{name}"
        recovery.run(["curl", "--fail", "--location", "--retry", "5", "--retry-all-errors", "--output", work / name, url])
        assert recovery.digest(work / name) == published["assets"][name]["sha256"]
    descriptor = json.loads((work / "runtime-release.json").read_text())
    assert descriptor["source_commit"] == recovery.SHA and descriptor["manifest_sha256"] == recovery.MANIFEST_SHA
    assert descriptor["archive"]["sha256"] == recovery.ARCHIVE_SHA
    sdk = work / "sdk"
    assert sdk.is_dir() and not sdk.is_symlink()
    assert (sdk / "VERSION").read_text().strip() == recovery.AGNOS
    loader = sdk / "lib/ld-linux-aarch64.so.1"
    assert loader.resolve().is_relative_to(sdk.resolve())
    with loader.open("rb") as source:
        header = source.read(20)
    assert header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\xb7\x00"
    recovery.room(work, descriptor["archive"]["bytes"] + descriptor["archive"]["unpacked_bytes"] + 512 * 2**20)
    helper = work / recovery.INSTALLER
    recovery.regular_mode(helper, 0o755)
    checkout = work / "install-online-verification"
    recovery.run(["git", "clone", "--no-hardlinks", "--no-checkout", root, checkout])
    recovery.run(["git", "-C", checkout, "lfs", "install", "--local", "--skip-smudge"])
    recovery.run(["git", "-C", checkout, "checkout", "--detach", recovery.SHA])
    recovery.run(["git", "-C", checkout, "remote", "set-url", "origin", f"https://github.com/{recovery.REPO}.git"])
    recovery.pristine(checkout)
    recovery.run(["qemu-aarch64-static", helper, "--root", checkout], cwd=checkout)
    recovery.run(["qemu-aarch64-static", "-L", sdk, "-E",
        f"LD_LIBRARY_PATH={checkout}/rust/bin/lib:{sdk}/usr/lib/aarch64-linux-gnu:{sdk}/lib",
        checkout / "rust/bin/openpilot-launch", "--root", checkout, "--verify-package"], cwd=checkout)
    recovery.save(work / "online-command-recovery.json", dict(source_commit=recovery.SHA,
        installer_exit=0, installed_agnos_launcher_exit=0, device_execution=False))
    assert recovery.digest(checkout / "rust/runtime-manifest.json") == recovery.MANIFEST_SHA
    source = (root / "rust/crates/dev-tools/src/package.rs").read_text().split("const SOURCE_ASSETS:", 1)[1].split("];", 1)[0]
    assets = set(re.findall(r'"([^"\n]+)"', source))
    assert len(assets) == 19
    manifest = json.loads((checkout / "rust/runtime-manifest.json").read_text())
    hashes = {row["path"]: row["sha256"] for row in manifest["files"]}
    assert all(recovery.digest(checkout / name) == hashes[name] for name in assets)
    before = recovery.git(checkout, "status", "--porcelain", "--untracked-files=no")
    # Match the producer's canonical LFS/index refresh; this never resets or
    # discards content, and any genuine change remains staged and fails below.
    recovery.run(["git", "-C", checkout, "add", "--renormalize", "--", *sorted(assets)])
    after = recovery.git(checkout, "status", "--porcelain", "--untracked-files=no")
    recovery.save(work / "install-source-recovery.json", dict(before=before, after=after))
    assert not after, f"installed tracked source changed: {after}"
    assert recovery.git(checkout, "rev-parse", "HEAD") == recovery.SHA
    assert set(recovery.git(checkout, "ls-files", "--others", "--exclude-standard").splitlines()) <= {"rust/bin"}
    active = checkout / "rust/bin"
    assert active.is_symlink() and active.readlink() == Path(".runtime-active/rust/bin")
    assert active.resolve().is_relative_to((checkout / "rust/.runtime-packages").resolve())
    recovery.pristine(root)
    recovery.save(work / "online-install.json", dict(source_commit=recovery.SHA,
        archive_sha256=recovery.ARCHIVE_SHA, manifest_sha256=recovery.MANIFEST_SHA,
        installer_exit=0, installed_agnos_launcher_exit=0, device_execution=False,
        verification_driver_commit=driver, original_publication_run=39617674785))


if __name__ == "__main__":
    assert os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
    assert os.environ.get("GITHUB_REPOSITORY") == recovery.REPO and os.environ["GITHUB_EVENT_NAME"] == "workflow_dispatch"
    assert os.environ["GITHUB_REF"] == "refs/heads/codex/feat-453-xiaoge-gpu"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--work", required=True, type=Path)
    args = parser.parse_args()
    verify(args.root.resolve(strict=True), args.work.resolve(strict=True))
