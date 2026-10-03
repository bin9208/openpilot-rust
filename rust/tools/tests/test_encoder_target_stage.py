import hashlib
from pathlib import Path
import zipfile

import pytest

from stage_encoder_target import Package, extract_native, verify_archive, verify_lock


def package(digest: str = 'unused') -> Package:
  return Package('fixture', '1.0', 'source-pin', digest, 1)


def test_exact_package_pin_and_locked_source_are_required(tmp_path: Path) -> None:
  wheel = tmp_path / 'fixture.whl'
  wheel.write_bytes(b'owned package')
  with pytest.raises(ValueError, match='SHA256'):
    verify_archive(wheel, package())
  pin = Package('fixture', '1.0', 'source-pin', hashlib.sha256(wheel.read_bytes()).hexdigest(), wheel.stat().st_size)
  verify_archive(wheel, pin)
  verify_lock({'package': [{'name': 'fixture', 'version': '1.0', 'source': {'git': 'repo#source-pin'}}]}, pin)
  with pytest.raises(ValueError, match='locked source'):
    verify_lock({'package': [{'name': 'fixture', 'version': '1.0', 'source': {'git': 'repo#wrong'}}]}, pin)


def test_extracts_native_files_without_python_launchers(tmp_path: Path) -> None:
  wheel = tmp_path / 'fixture.whl'
  with zipfile.ZipFile(wheel, 'w') as archive:
    archive.writestr('fixture/__init__.py', 'raise RuntimeError("must never run")')
    archive.writestr(package().prefix + 'include/header.h', b'header')
    archive.writestr(package().prefix + 'lib/libfixture.a', b'archive')
  records = extract_native(wheel, package(), tmp_path / 'native')
  assert len(records) == 2
  assert (tmp_path / 'native/include/header.h').read_bytes() == b'header'
  assert not (tmp_path / 'native/fixture').exists()


@pytest.mark.parametrize('relative,symlink', [('../outside', False), ('/absolute', False), ('lib/link', True)])
def test_rejects_unsafe_archive_members(tmp_path: Path, relative: str, symlink: bool) -> None:
  wheel = tmp_path / 'fixture.whl'
  with zipfile.ZipFile(wheel, 'w') as archive:
    info = zipfile.ZipInfo(package().prefix + relative)
    if symlink:
      info.external_attr = 0o120777 << 16
    archive.writestr(info, b'outside')
  with pytest.raises(ValueError, match='unsafe'):
    extract_native(wheel, package(), tmp_path / 'native')
