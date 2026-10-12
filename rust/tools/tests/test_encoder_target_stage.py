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


def test_swscale_rejects_unpinned_source_before_build(tmp_path: Path) -> None:
  from stage_encoder_target import build_swscale

  archive = tmp_path / 'source.tar.xz'
  archive.write_bytes(b'wrong source')
  with pytest.raises(ValueError, match='source SHA256'):
    build_swscale(tmp_path / 'build', tmp_path / 'install', archive, 'missing-cc', 'missing-ar', 'missing-ranlib')
  assert not (tmp_path / 'build/ffmpeg-7.1').exists()


def test_swscale_rejects_incompatible_pinned_avutil_headers(tmp_path: Path) -> None:
  from stage_encoder_target import verify_swscale_headers

  source, install = tmp_path / 'source', tmp_path / 'install'
  for name in ('version.h', 'pixfmt.h'):
    for root in (source / 'libavutil', install / 'include/libavutil'):
      root.mkdir(parents=True, exist_ok=True)
      (root / name).write_bytes(b'compatible source')
  (install / 'include/libavutil/pixfmt.h').write_bytes(b'wrong ABI')
  with pytest.raises(ValueError, match='avutil header'):
    verify_swscale_headers(source, install)
