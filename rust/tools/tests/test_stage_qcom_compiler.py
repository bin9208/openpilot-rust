from __future__ import annotations

import hashlib
import io
from pathlib import Path
import tarfile

import pytest

from stage_qcom_compiler import InvalidProvider, extract_python, require_arm_elf, verify_file


def test_provider_digest_rejects_changed_bytes(tmp_path: Path) -> None:
    # Given an owned file with an admitted digest.
    source = tmp_path / "provider"
    source.write_bytes(b"original")
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    assert verify_file(source, digest) == digest

    # When its bytes change, staging rejects them before loading a provider.
    source.write_bytes(b"tampered")
    with pytest.raises(InvalidProvider, match="SHA256 mismatch"):
        verify_file(source, digest)


@pytest.mark.parametrize("name", ["../escape", "/absolute", "python/../../escape", "unexpected/file"])
def test_python_archive_rejects_escaping_members(tmp_path: Path, name: str) -> None:
    # Given an archive with a member outside the expected Python root.
    archive = tmp_path / "python.tar.gz"
    with tarfile.open(archive, "w:gz") as target:
        member = tarfile.TarInfo(name)
        member.size = 1
        target.addfile(member, io.BytesIO(b"x"))

    # When extraction is requested, no out-of-root file is admitted.
    with pytest.raises(InvalidProvider, match="unexpected Python archive member"):
        extract_python(archive, tmp_path / "output")


def test_python_archive_rejects_external_symlink(tmp_path: Path) -> None:
    # Given a nominal Python-root member linking outside its extraction root.
    archive = tmp_path / "python.tar.gz"
    with tarfile.open(archive, "w:gz") as target:
        member = tarfile.TarInfo("python/external")
        member.type = tarfile.SYMTYPE
        member.linkname = "/outside"
        target.addfile(member)

    # When the standard data filter runs, it rejects the unsafe link.
    with pytest.raises(tarfile.AbsoluteLinkError):
        extract_python(archive, tmp_path / "output")


def test_compiler_input_rejects_host_elf(tmp_path: Path) -> None:
    # Given owned ELF header data for x86-64, not a compiler implementation.
    source = tmp_path / "host-elf"
    source.write_bytes(b"\x7fELF\x02\x01" + b"\0" * 12 + (62).to_bytes(2, "little"))

    # When ARM provider admission runs, it rejects the wrong machine.
    with pytest.raises(InvalidProvider, match="expected AArch64 ELF"):
        require_arm_elf(source)
