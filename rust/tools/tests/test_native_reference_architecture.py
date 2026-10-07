from __future__ import annotations

import hashlib
import json
from pathlib import Path
import platform
import subprocess
import zipfile

import pytest

import native_logging_build


@pytest.fixture
def wheels(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
  archives = {}
  entries = []
  for architecture in ("x86_64", "aarch64"):
    filename = f"json11-fixture-py3-none-manylinux_2_28_{architecture}.whl"
    path = tmp_path / filename
    with zipfile.ZipFile(path, "w") as archive:
      archive.writestr("dependency.data/purelib/json11/install/lib/libjson11.a", architecture.encode())
    url = "https://fixture.invalid/" + filename
    archives[url] = path.read_bytes()
    digest = hashlib.sha256(archives[url]).hexdigest()
    entries.append(f'{{ url = {json.dumps(url)}, hash = "sha256:{digest}" }}')
  (tmp_path / "uv.lock").write_text('[[package]]\nname = "comma-deps-json11"\nversion = "fixture"\nwheels = [\n'
                                   + ",\n".join(entries) + "\n]\n")
  calls = []

  def download(command: list[str], check: bool) -> subprocess.CompletedProcess[str]:
    assert command[:2] == ["curl", "-fsSL"] and command[3] == "-o" and check
    calls.append(command[2])
    Path(command[4]).write_bytes(archives[command[2]])
    return subprocess.CompletedProcess(command, 0)

  monkeypatch.setattr(native_logging_build.subprocess, "run", download)
  return tmp_path, archives, calls


@pytest.mark.parametrize("architecture", ["x86_64", "aarch64"])
def test_staged_library_comes_from_the_locked_native_architecture(wheels, monkeypatch: pytest.MonkeyPatch, architecture: str) -> None:
  root, _archives, calls = wheels
  monkeypatch.setattr(platform, "machine", lambda: architecture)
  install, provenance = native_logging_build.stage_json11(root, root / "output")
  assert (install / "lib/libjson11.a").read_bytes() == architecture.encode()
  assert provenance["json11_architecture"] == architecture
  assert len(calls) == 1
  assert provenance["json11_sha256"] == hashlib.sha256(_archives[calls[0]]).hexdigest()


def test_unsupported_host_fails_before_downloading_an_unrelated_library(wheels, monkeypatch: pytest.MonkeyPatch) -> None:
  root, _archives, calls = wheels
  monkeypatch.setattr(platform, "machine", lambda: "armv7l")
  with pytest.raises(ValueError, match="armv7l"):
    native_logging_build.stage_json11(root, root / "output")
  assert calls == []


def test_target_choice_keeps_the_locked_hash_check(wheels, monkeypatch: pytest.MonkeyPatch) -> None:
  root, archives, _calls = wheels
  monkeypatch.setattr(platform, "machine", lambda: "aarch64")
  for url in archives:
    archives[url] = b"modified native archive"
  with pytest.raises(AssertionError):
    native_logging_build.stage_json11(root, root / "output")
